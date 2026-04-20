# Project Research Summary

**Project:** KafPy v2.0 Code Quality Refactor
**Domain:** PyO3/Rust Kafka Framework Refactoring
**Researched:** 2026-04-20
**Confidence:** HIGH

## Executive Summary

KafPy is a Rust-first Kafka framework with idiomatic Python bindings via PyO3, currently at v1.9 after 7 milestones of feature development. The codebase shows classic growing-pains patterns: a 1309-line `worker_pool/mod.rs` with severe duplication, a 650-line `dispatcher/mod.rs` with mixed responsibilities, and implicit state machines where `Option<OwnedMessage>` models lifecycle state without compiler enforcement. The good news: module boundaries between consumer/dispatcher/worker_pool/coordinator are sound. The refactoring surface is intra-module, not inter-module, making this a safe high-value target.

The recommended approach is a phased extract-refactor-swap strategy: extract duplicated logic first (lowest risk, immediate value), then split god modules by responsibility, then introduce state machine enums and type safety. The PyO3 boundary should be preserved as a thin wrapper around a `RuntimeBuilder` factory that lives in pure Rust. Any refactoring touching `Arc`, `Mutex`, `spawn_blocking`, or channel capacity must be verified with `Send+Sync` compile-time checks and throughput benchmarks.

The primary risk is behavioral change during refactoring: channel capacity changes silently alter backpressure behavior, GIL boundary violations cause deadlocks, and shutdown ordering violations cause circular waits. Each phase has specific verification checkpoints documented in the pitfalls research.

---

## Key Findings

### Recommended Stack

**From STACK.md:**

The Rust refactoring toolchain is mature and well-integrated. `clippy` is the primary linter with 800+ lints covering correctness, style, complexity, and performance. Key lints for this refactor: `cognitive_complexity` (threshold 25), `type_complexity` (200), `too_many_arguments` (8), `struct_excessive_bools` (flags bool fields suggesting state enums). `cargo-audit` and `cargo-deny` handle security and dependency compliance. `cargo-geiger` tracks unsafe code which is critical for PyO3 bridges.

**Core tools:**
- `clippy` (bundled) — primary linter, detects god objects, cognitive complexity, code smells
- `cargo-audit` 0.22.1 — security vulnerability scanning
- `cargo-deny` 0.19.4 — dependency graph, license compliance
- `cargo-machete` 0.9.2 — unused dependency detection
- `cargo-geiger` 0.13.0 — unsafe code tracking (critical for PyO3 boundary)

### Expected Features

**From FEATURES.md (Code Smell Catalog):**

**Critical code smells to address:**

1. **Duplication in `worker_pool/mod.rs`** — Three distinct duplication patterns:
   - `ExecutionResult::Error` vs `Rejected` arms (lines 279-469) share identical retry/DLQ logic
   - Batch flush boilerplate repeated 6 times across `batch_worker_loop`
   - Message-to-PyDict conversion duplicated across `invoke()`, `invoke_batch()`, `invoke_async()`

2. **God objects requiring split:**
   - `worker_pool/mod.rs` (1309 lines) — 6 distinct responsibilities
   - `dispatcher/mod.rs` (650 lines) — consumer + dispatcher + tests
   - `pyconsumer.rs` (344 lines) — runtime assembly at PyO3 boundary

3. **Implicit state machines:**
   - `worker_loop`'s `active_message: Option<OwnedMessage>` as state machine
   - `batch_worker_loop`'s `backpressure_active: bool` flag

4. **Type safety issues:**
   - `HandlerId = String` type alias (should be newtype)
   - `OwnedMessage` naming confusion (topic-based, not identity-based)

**Refactoring strategies identified:**
- Extract-Refactor-Swap for god objects (safe, incremental)
- Template Method for duplicated logic (handle_execution_failure helper)
- State Machine Extraction replacing Option/Bool with explicit enums
- Newtype pattern for HandlerId

### Architecture Approach

**From ARCHITECTURE.md:**

The current module structure is functional but shows cohesion problems. Key issues:

1. **`coordinator/` conflates four distinct responsibilities:** Offset tracking, commit task, retry coordination, and shutdown — these should be split into `offset/`, `retry/`, and `shutdown/` modules

2. **`batch/` module missing:** `PartitionAccumulator` lives in `worker_pool/` but is a general accumulation primitive that belongs in its own module; `BatchAccumulator` belongs in `python/`

3. **PyO3 boundary is thick:** `pyconsumer.rs` (344 lines) assembles the entire runtime. This should become a thin wrapper around a `RuntimeBuilder` in `runtime/` module

4. **State modeling is implicit:** `bool` flags and `Option<T>` model lifecycle state instead of explicit enums

**Recommended structure (8 new modules):**
```
batch/                    # PartitionAccumulator (moved from worker_pool/)
runtime/                  # RuntimeBuilder (extracted from pyconsumer.rs)
offset/                   # OffsetTracker, OffsetCommitter (split from coordinator/)
retry/                    # RetryCoordinator (moved from coordinator/)
shutdown/                 # ShutdownCoordinator (moved from coordinator/)
```

**Patterns to apply:**
1. Thin PyO3 Boundary via Runtime Factory — extract composition from `pyconsumer.rs`
2. State Machine as Enum — replace `Option<OwnedMessage>` and `backpressure_active: bool`
3. Newtype for HandlerId — prevent String interchangeability bugs
4. Sealed traits for internal extension points

### Critical Pitfalls

**From PITFALLS.md:**

1. **Send+Sync Guarantees Breaking** — Moving `Arc<Mutex<...>>` to `parking_lot::Mutex` or using `Rc` instead of `Arc` silently changes thread-safety semantics. Use `fn assert_send_sync<T: Send + Sync>()` compile-time checks.

2. **Channel Semantic Changes** — Changing `mpsc::channel` capacity from 1000 to 100 or switching from `try_send` to `send` fundamentally alters backpressure and message loss behavior. Never change capacity without documenting impact on `BackpressureAction::Drop` behavior. The semaphore-before-dispatch pattern (DISP-15) is critical.

3. **PyO3 GIL Boundary Violations** — `Arc<Py<PyAny>>` must remain GIL-independent. All Python invocations must go through `spawn_blocking` or `PythonAsyncFuture`. Moving `Python::attach` outside the `move ||` closure captures GIL state incorrectly.

4. **Shutdown Ordering Violations** — Current order: dispatcher cancel FIRST, then worker cancel. Refactoring that reverses this causes circular waits. Maintain `biased` directive on `select!` in `ConsumerRunner::run()`.

5. **Offset Commit State Machine Breaking** — `highest_contiguous_offset` logic depends on `should_commit` checking `has_terminal` flag per-partition. Once `has_terminal=true`, that partition stops committing until restart. `store_offset` must precede `commit` — this is the two-phase offset management contract.

---

## Implications for Roadmap

Based on research, suggested phase structure:

### Phase 1: Extract Duplicated Logic
**Rationale:** Lowest risk, immediate value. No module boundaries change, only internal function extraction. Establishes extraction pattern before higher-risk splits.

**Delivers:**
- `message_to_pydict()` helper in `python/handler.rs`
- `handle_execution_failure()` in `worker_pool/mod.rs`
- `flush_partition_batch()` helper in `batch_worker_loop`
- Baseline clippy metrics with `cognitive_complexity`, `type_complexity` thresholds

**Avoids:** Duplication pattern that makes later refactoring dangerous

**Research flag:** Standard pattern — skip deep research

### Phase 2: Split `worker_pool/` God Module
**Rationale:** `PartitionAccumulator` and `BatchAccumulator` are clearly extractable. Creates `batch/` module and `python/batch.rs`. No cross-boundary changes.

**Delivers:**
- `worker_pool/accumulator.rs` — PartitionAccumulator, BatchAccumulator
- `worker_pool/loop.rs` — worker_loop function
- `worker_pool/batch_loop.rs` — batch_worker_loop, handle_batch_result_inline
- `worker_pool/mod.rs` becomes thin re-export layer

**Uses:** clippy cognitive_complexity lints for extraction triggers

**Research flag:** Standard pattern — skip deep research

### Phase 3: Split `dispatcher/` and Extract `runtime/`
**Rationale:** `ConsumerDispatcher` mixes consumer orchestration with dispatcher logic. Also extracts `RuntimeBuilder` from `pyconsumer.rs` to make pure-Rust code testable without Python.

**Delivers:**
- `dispatcher/pause.rs` — pause/resume logic
- `dispatcher/consumer_dispatcher.rs` — ConsumerDispatcher
- `runtime/mod.rs` — RuntimeBuilder factory
- Thin `pyconsumer.rs` wrapper (~60 lines target)

**Addresses:** PyO3 boundary thickening, dispatcher god object

**Research flag:** PyO3 boundary changes need integration test verification

### Phase 4: State Machine Extraction
**Rationale:** Replace implicit state (Option, bool flags) with explicit enums. Enables compiler-enforced exhaustiveness checking.

**Delivers:**
- `WorkerState` enum replacing `active_message: Option<OwnedMessage>` in worker_loop
- `BackpressureState` enum replacing `backpressure_active: bool` in batch_worker_loop
- `ShutdownPhase` enum (already partially modeled)
- `RetryState` enum for RetryCoordinator

**Addresses:** Implicit state machine pitfalls (Pitfall 5, 6)

**Research flag:** Standard pattern — skip deep research

### Phase 5: Module Split — `coordinator/` to `offset/` + `retry/` + `shutdown/`
**Rationale:** Coordinator conflates four distinct responsibilities. Split by responsibility boundary. Only depends on consumer/ being stable.

**Delivers:**
- `offset/` — OffsetTracker, OffsetCommitter, OffsetCoordinator trait
- `retry/` — RetryCoordinator (moved from coordinator/)
- `shutdown/` — ShutdownCoordinator (moved from coordinator/)

**Addresses:** Architecture cohesion issue, retry coordinator location question

**Research flag:** Offset commit state machine (Pitfall 5) — needs consumer restart test verification

### Phase 6: Type Safety — HandlerId Newtype + Error Consolidation
**Rationale:** Last phase. Replaces `HandlerId = String` type alias and consolidates scattered error types. Ripples through routing, dispatcher, worker_pool.

**Delivers:**
- `struct HandlerId(String)` newtype in routing/context.rs
- Unified `error.rs` re-exporting all domain errors
- Removed `errors.rs`

**Addresses:** Type safety issue, scattered error types

**Research flag:** HandlerId vs topic distinction — verify they are always equal or distinct

### Phase Ordering Rationale

1. **Duplication extraction first** — establishes safe extraction pattern before structural changes
2. **Intra-module splits before inter-module** — worker_pool split doesn't affect coordinator
3. **PyO3 boundary last** — RuntimeBuilder extraction is additive until old implementation removed
4. **Type safety last** — HandlerId newtype ripples through all modules, do after dust settles

**Research flags by phase:**
| Phase | Needs Research | Standard Pattern |
|-------|---------------|------------------|
| Phase 1 (Duplication) | No | Yes |
| Phase 2 (worker_pool split) | No | Yes |
| Phase 3 (dispatcher + runtime) | PyO3 integration tests | Partial |
| Phase 4 (State machines) | No | Yes |
| Phase 5 (coordinator split) | Offset commit semantics | Partial |
| Phase 6 (Type safety) | HandlerId distinction | No |

---

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | Verified via crates.io, Rust toolchain docs |
| Features (code smells) | HIGH | Verified by reading source, exact line ranges identified |
| Architecture | MEDIUM | Based on codebase analysis and Rust patterns; no external web sources accessible |
| Pitfalls | MEDIUM-HIGH | Based on Rust/PyO3 domain knowledge and observed patterns; external search unavailable |

**Overall confidence:** MEDIUM-HIGH

### Gaps to Address

- **HandlerId vs topic distinction:** Are they always equal or conceptually distinct? If always equal, newtype may be unnecessary complexity.
- **NoopSink duplication:** Is `NoopSink` in `worker_pool/mod.rs` identical to any sink in observability module?
- **coordinator/retry_coordinator.rs location:** Should RetryCoordinator live in `retry/` module since it uses `RetryPolicy` from there?
- **PartitionAccumulator naming:** Should it be renamed to `PerPartitionBuffer` for clarity?

---

## Sources

### Primary (HIGH confidence)
- KafPy source code analysis — code smell catalog, module boundaries, duplication patterns
- crates.io — verified tool versions: clippy 0.0.302, cargo-audit 0.22.1, cargo-deny 0.19.4, cargo-machete 0.9.2, cargo-geiger 0.13.0

### Secondary (MEDIUM confidence)
- The Rust Programming Language Book — modules, async, error handling
- Tokio documentation — channel semantics, select bias, cancellation
- PyO3 User Guide — GIL handling, pyclass, async patterns
- Rust API Guidelines — structuring, error types, API design

### Tertiary (LOW confidence)
- Rustonomicon — unsafe code concepts (needs verification)
- sonarr-rust vs clippy comparison (heavier alternative, not chosen)

---
*Research completed: 2026-04-20*
*Ready for roadmap: yes*