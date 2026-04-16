# Project Research Summary

**Project:** KafPy v1.3 Offset Commit Coordinator
**Domain:** Kafka at-least-once delivery via per-topic-partition highest-contiguous-offset tracking
**Researched:** 2026-04-16
**Confidence:** MEDIUM

## Executive Summary

KafPy v1.3 adds an offset commit coordinator to enable at-least-once delivery semantics. The core mechanism tracks per-topic-partition acknowledged offsets, buffers out-of-order acks using a BTreeSet, and computes the highest contiguous offset for batch commit via rdkafka's `store_offset()` + `commit_consumer_state()` API. This is a well-understood pattern in the Kafka ecosystem; the key insight is that committing must happen only after confirmed successful processing of offset N (not N+1), and out-of-order processing requires buffering until gaps fill.

The recommended architecture introduces a new `src/coordinator/` module with two components: `OffsetTracker` (per-partition ack state machine) and `OffsetCommitter` (background Tokio task managing rdkafka commits). Integration with WorkerPool happens via a new `OffsetCoordinator` trait, keeping offset tracking separate from `Executor` policy concerns. No new crate dependencies are required -- the existing stack (rdkafka 0.38, Tokio 1.40, parking_lot 0.12) is sufficient.

Key risks center on PyO3 lifetime management (GIL hold window, raw pointer double-free) rather than Kafka semantics. The most critical correctness guarantee is the highest-contiguous-offset algorithm: committing beyond the highest contiguous processed offset causes message loss. The most critical production risk is Tokio thread pool exhaustion from synchronous Python calls without `spawn_blocking`.

## Key Findings

### Recommended Stack

The v1.3 offset commit coordinator uses existing infrastructure. No new dependencies required.

**Core technologies:**
- **rdkafka 0.38** -- Kafka protocol, `store_offset()`, and `commit_consumer_state()`. Verified via existing `ConsumerRunner::commit()` pattern in `src/consumer/runner.rs:116-122`. Two-phase model: `store_offset()` is fast/in-memory, `commit_consumer_state()` is the network round-trip.
- **parking_lot 0.12** -- Fast mutex for the tracker HashMap. Existing stack, avoids poison semantics.
- **tokio 1.40** -- Async runtime and channels. `OffsetCommitter` runs as a background Tokio task using a `watch` channel for commit signalling.

**Configuration required:**
- `enable.auto.commit=false` -- disable automatic commit (we control commit manually)
- `enable.auto.offset.store=false` -- disable auto-store (we call `store_offset()` explicitly after successful processing)

**New module structure:**
```
src/coordinator/
  mod.rs              # Module init, public exports
  offset_tracker.rs   # Per-topic-partition ack state machine
  commit_task.rs      # Background Tokio task managing rdkafka commits
  error.rs            # CoordinatorError
```

### Expected Features

**Must have (table stakes):**
- **Per-topic-partition offset tracking** -- Key by `(topic, partition)`, not just topic. Kafka assigns offsets independently per partition.
- **Highest contiguous offset calculation** -- Only commit when all offsets up to N are confirmed. Using `BTreeSet<i64>` for efficient range queries. Gap detection prevents message loss.
- **Out-of-order ack buffering** -- Workers ack concurrently; buffering handles non-deterministic order. Advance cursor when gap fills.
- **store_offset() before commit()** -- Two-phase rdkafka pattern: store in-memory, then flush to broker.
- **No duplicate commit guard** -- Only call `commit()` if `highest_contiguous > committed_offset`.
- **Configuration: commit_interval_ms, commit_max_messages** -- Batching amortizes network cost.

**Should have (differentiators):**
- **CommitExecutor plug-in** -- Wraps `DefaultExecutor` with offset commit policy. Satisfies pluggable executor requirement from v1.2.
- **Graceful shutdown commit** -- Before worker pool drains, commit highest contiguous so no messages are re-delivered on restart.
- **Automatic offset tracking per ExecutionResult** -- User does not manually call store/commit; WorkerPool/Executor integration handles it transparently.

**Defer (v2+):**
- **RetryExecutor integration with offset tracking** -- When retry exhausts, offset should NOT advance.
- **DLQ routing with offset tracking** -- Rejected messages should not prevent commit of earlier offsets.
- **Backpressure-aware commit pacing** -- When dispatcher is under backpressure, commit more aggressively.

### Architecture Approach

The coordinator introduces a new `src/coordinator/` module that orchestrates cross-cutting commit concerns spanning `WorkerPool` (ack source) and `ConsumerRunner` (commit sink). `OffsetTracker` implements an `OffsetCoordinator` trait that WorkerPool calls on each `ExecutionResult::Ok`. `OffsetCommitter` runs as a background Tokio task, decoupled from the hot worker path, using a `watch` channel to receive commit signals. Key design decision: `OffsetCoordinator` is a separate trait from `Executor`, keeping two distinct concerns (executor policy vs. offset tracking) properly separated.

**Major components:**
1. **OffsetTracker** -- Per-partition ack state machine. Maintains `committed_offset`, `pending_offsets` (BTreeSet), `failed_offsets` (BTreeSet). Implements highest-contiguous-offset algorithm with gap detection.
2. **OffsetCommitter** -- Background Tokio task. Receives commit signals via watch channel, throttles via `min_commit_interval`, calls `runner.store_offset()` + `runner.commit()`.
3. **OffsetCoordinator trait** -- `record_ack(topic, partition, offset)` interface. `WorkerPool` holds `Arc<dyn OffsetCoordinator>`. Keeps offset tracking decoupled from `Executor`.

### Critical Pitfalls

1. **Raw pointer double-free with `Py<PyAny>`** -- Cloning a `Py<PyAny>` via `as_ptr()` + `from_owned_ptr` doubles the reference count. Fix: use `Py::clone(&handler)` for any cross-boundary copy.
2. **GIL hold window creep** -- Calling Python directly in the async message loop blocks the Tokio thread while holding the GIL. Fix: always route Python execution through `tokio::task::spawn_blocking`.
3. **Storing `Bound<'_, PyAny>` instead of `Py<PyAny>`** -- `Bound` carries a lifetime tied to a specific `Python` token. Cannot be stored across async boundaries. Fix: use `callback.unbind()` at the boundary to convert to `Py<PyAny>`.
4. **Fake parallelism from Tokio tasks** -- Spawning multiple `tokio::spawn` tasks that all call Python creates GIL serialization. Throughput does not improve. Fix: use a `Semaphore` to limit concurrent Python executions.
5. **Python exceptions escaping async context** -- If a Python exception propagates out of a `Python::with` block across an async boundary, the exception state is lost. Fix: always handle `PyErr` within the `Python::with` scope.

## Implications for Roadmap

Based on research, suggested phase structure:

### Phase 1: OffsetTracker Core
**Rationale:** Pure data structure with no dependencies on other new components. Can be built and unit-tested independently. The highest-contiguous-offset algorithm is the core correctness invariant.
**Delivers:** `src/coordinator/offset_tracker.rs` with `PartitionState`, `OffsetTracker`, `ack()`, `highest_contiguous()`, `should_commit()`. BTreeSet-based pending buffer with gap detection.
**Implements:** Architecture component `OffsetTracker`
**Avoids:** Risk of building on a flawed foundation -- the contiguous algorithm must be correct before any commit wiring.

### Phase 2: OffsetCommitter Background Task
**Rationale:** Depends on Phase 1 (OffsetTracker) and ConsumerRunner (existing). The committer task wraps rdkafka's commit API with interval throttling. Single-producer/single-consumer watch channel is the correct pattern here.
**Delivers:** `src/coordinator/commit_task.rs` with `OffsetCommitter`, background Tokio task, watch channel integration, `min_commit_interval` throttle.
**Implements:** Architecture component `OffsetCommitter`

### Phase 3: ConsumerRunner store_offset Method
**Rationale:** No dependencies. Adds `store_offset(topic, partition, offset)` method to ConsumerRunner. Required before manual commit can work. rdkafka requires explicit `store_offset()` before `commit_consumer_state()` for manual offset management.
**Delivers:** `ConsumerRunner::store_offset()` method. Consumer config updated with `enable.auto.commit=false` and `enable.auto.offset.store=false`.
**Uses:** Stack element `rdkafka 0.38` -- `store_offsets()` API

### Phase 4: OffsetCoordinator Trait Integration
**Rationale:** Depends on Phase 1. Adds `OffsetCoordinator` trait to `src/python/executor.rs`. Establishes the interface contract between WorkerPool and OffsetTracker.
**Delivers:** `OffsetCoordinator` trait: `record_ack(topic, partition, offset)`. `OffsetTracker` implements the trait. `WorkerPool::new()` accepts `Arc<dyn OffsetCoordinator>`.
**Avoids:** Mixing two separate concerns in `Executor` -- keeping offset tracking independent of executor policy.

### Phase 5: WorkerPool Integration
**Rationale:** Depends on Phase 1, 3, and 4. Wires the `OffsetCoordinator` into `WorkerPool::worker_loop()` on `ExecutionResult::Ok`. Minimal changes to existing worker_loop.
**Delivers:** WorkerPool calls `offset_coordinator.record_ack()` alongside existing `queue_manager.ack()`. Graceful shutdown triggers final commit for each topic-partition.
**Implements:** Architecture component `WorkerPool` modification

### Phase 6: PyO3 Bridge and End-to-End
**Rationale:** Depends on Phase 5. Wires coordinator into PyConsumer, creates OffsetTracker and OffsetCommitter instances, spawns committer task.
**Delivers:** `src/pyconsumer.rs` wiring. End-to-end integration test with real Kafka + Python callback.
**Addresses:** Pitfall GIL hold window (verify `spawn_blocking` used), raw pointer double-free (verify `Py::clone()` used).

### Phase Ordering Rationale

- Phase 1-2 build the coordinator components in isolation (pure Rust, no Kafka/Python needed for unit tests). This lets the algorithm be validated before integration.
- Phase 3 (ConsumerRunner store_offset) is independent but needed for Phase 6.
- Phase 4 establishes the interface before Phase 5 uses it.
- Phase 6 is the integration phase -- by this point all components are individually tested.
- PyO3 pitfalls (GIL, raw pointers, lifetime) are addressed during Phase 6 integration, not in earlier phases (those are existing issues, not new to v1.3).

### Research Flags

**Phases needing deeper research during planning:**
- **Phase 6 (PyO3 Bridge):** The PyO3 integration patterns (GIL management, lifetime handling) are well-documented but must be verified with integration tests. Plan to use `tokio-console` to observe GIL hold behavior under load.
- **Phase 2 (OffsetCommitter):** Watch channel throttle behavior under high offset throughput -- may need tuning of `min_commit_interval`. Recommend load test at 10K msg/s.

**Phases with standard patterns (skip research-phase):**
- **Phase 1 (OffsetTracker):** BTreeSet contiguous algorithm is standard ecosystem practice, well-documented in FEATURES.md anti-patterns section.
- **Phase 3 (ConsumerRunner store_offset):** Verified via existing `ConsumerRunner::commit()` pattern in codebase.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | MEDIUM | rdkafka 0.38 API verified via existing codebase patterns; offset-store async patterns lack published code examples but are consistent with rdkafka architecture |
| Features | MEDIUM | Kafka offset semantics well-documented in protocol docs; highest-contiguous algorithm is standard; out-of-order buffering logic is standard ecosystem practice |
| Architecture | HIGH | Clear component boundaries; `OffsetCoordinator` separation from `Executor` is a sound design; watch channel pattern is standard Tokio |
| Pitfalls | MEDIUM | Based on PyO3 internals, Rust async patterns, and analysis of existing `pyconsumer.rs` -- no new pitfalls specific to v1.3 beyond existing PyO3 issues |

**Overall confidence:** MEDIUM

### Gaps to Address

- **OffsetTracker restart behavior:** When a consumer restarts, it resumes from the last committed offset. The initial `committed_offset` value on startup (before any commit) needs to be validated against Kafka's "earliest" vs "latest" semantics. Recommend: start at -1, let Kafka handle initial positioning.
- **Commit interval tuning:** `min_commit_interval` default (100ms) may need tuning. Recommend adding a metric for commit frequency and tuning in production-like environment.
- **Integration test coverage:** The highest-contiguous algorithm needs an integration test with real Kafka to verify no message loss under concurrent worker ack patterns.

## Sources

### Primary (HIGH confidence)
- Existing `ConsumerRunner::commit()` implementation in `src/consumer/runner.rs:116-122` -- verified `store_offset` and `commit_consumer_state` API usage
- Existing codebase patterns -- confirmed rdkafka 0.38, parking_lot, tokio versions from `Cargo.toml`
- rdkafka `Consumer` trait documentation via Context7 -- `store_offset` and `commit_consumer_state` API verified

### Secondary (MEDIUM confidence)
- PyO3 documentation -- Python boundary and GIL management patterns
- Kafka protocol docs -- `OffsetCommitRequest` semantics, offset = "next expected offset"
- Tokio documentation -- `spawn_blocking` contract and `watch` channel patterns

### Tertiary (LOW confidence)
- Confluent blog on exactly-once semantics -- covers at-least-once vs exactly-once tradeoffs, needs validation against specific use case
- Community patterns for BTreeSet contiguous offset calculation -- widely used but few published examples specific to this exact algorithm

---
*Research completed: 2026-04-16*
*Ready for roadmap: yes*
