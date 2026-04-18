# Project Research Summary

**Project:** KafPy - PyO3 Kafka Consumer Framework
**Domain:** Kafka message processing with batch and async Python handler execution
**Researched:** 2026-04-18
**Confidence:** MEDIUM

## Executive Summary

KafPy is a Rust-backed Kafka consumer framework where the Rust core handles protocol (rdkafka + Tokio) and Python holds business logic via PyO3 bindings. Adding batch and async handler support requires: (1) a `HandlerMode` enum to represent execution variants (single-sync, single-async, batch-sync, batch-async), (2) a `BatchAccumulator` that collects messages until `max_batch_size` OR `max_batch_wait_ms` timeout, (3) async handler support via `pyo3-async-runtimes` `GILRef` pattern that releases GIL during await, and (4) a `BatchResult` type to model all-or-nothing or partial-success batch outcomes.

The core challenge is GIL management: sync handlers use `spawn_blocking` (GIL held for brief window), while async handlers use `into_future` (GIL released during await). Both achieve the EXEC-06 minimal-GIL principle, but the paths are architecturally distinct. Batch handlers risk holding GIL for entire batch duration if implemented naively -- mitigation requires either per-message invocation within batch or conservative `max_batch_size` configuration.

Key pitfalls: (1) GIL held for entire batch duration, (2) batch result ambiguity losing per-message outcomes, (3) offset commit at wrong granularity, (4) async coroutine incorrectly invoked via `spawn_blocking` instead of `into_future`, (5) batch timeout + retry interaction corrupting retry state, (6) parallel batch execution breaking per-partition ordering, (7) `CancellationToken` not draining accumulated batch on shutdown.

## Key Findings

### Recommended Stack

The stack is already largely in place. The primary addition is `pyo3-async-runtimes` 0.27.0 which provides `into_future` to convert Python coroutines into Tokio-compatible Futures. The key technology choices:

**Core technologies:**
- `pyo3` 0.27.2 -- already in use; stable GIL API with `Bound<PyAny>` and `Python::with_gil`
- `pyo3-async-runtimes` 0.27.0 -- provides `into_future` for async Python handlers; GIL released during await
- `tokio` 1.40 -- already in use; native rdkafka compat, mpsc channels, `spawn_blocking`

**What IS needed:**
- `pyo3_async_runtimes::tokio::into_future` -- converts Python coroutine to Tokio Future, GIL released during await
- Coroutine detection via Python stdlib `inspect` module (`inspect.iscoroutinefunction`)
- `HandlerMode` enum (SingleSync, SingleAsync, BatchSync, BatchAsync) unified execution interface
- `BatchResult` type (AllSuccess, AllFailure, PartialFailure) -- NOT reuse of existing `ExecutionResult`

**What NOT to use:**
- `async-std` runtime -- would conflict with Tokio
- `pyo3-asyncio` -- deprecated, merged into `pyo3-async-runtimes`
- `futures::executor::block_on` -- wrong executor for PyO3
- `spawn_blocking` for async handlers -- defeats async purpose; blocks Tokio thread

### Expected Features

**Must have (table stakes):**
- `HandlerMode` enum -- core abstraction gating all work; represents 4 execution variants
- Batch accumulation logic -- `BatchAccumulator` struct holding messages with size and time limits
- Batch handler invoke (sync) -- `PythonHandler.invoke` receives `Vec<OwnedMessage>`, Python callable receives `List[Dict]`
- `WorkerPool` mode dispatch -- selects appropriate invoke path based on handler mode
- GIL minimal usage (verified) -- GIL only inside Python execution blocks; confirmed via tokio-console

**Should have (competitive):**
- Async Python handler invoke via `pyo3-async-runtimes` `into_future`/`future_into_py`
- `BatchResult` with partial failure tracking (succeeded/failed per message)
- Python-side unified registration API -- `consumer.on_topic("events", handler, mode="batch", batch_size=100)`

**Defer (v2+):**
- Streaming windowed batches (sliding/session windows) -- very high complexity
- Schema validation at batch boundary -- orthogonal concern

### Architecture Approach

The architecture introduces batch accumulation at the worker level (not dispatcher level), keeping dispatcher infrastructure unchanged. Batch accumulation uses `tokio::select!` racing `rx.recv()` against a timeout. Async handlers are invoked as `async fn` inside the worker loop, awaiting directly rather than via `spawn_blocking`.

**Major components:**
1. `BatchAccumulator` -- accumulates messages per-worker; triggers on size OR timeout; respects `CancellationToken`
2. `BatchHandler` / `AsyncPythonHandler` -- invokes Python callable on accumulated batch via `spawn_blocking` or `into_future`
3. `HandlerMode` enum -- unified interface across 4 execution modes; construction-time decision (not runtime dispatch)
4. `BatchExecutionResult` enum -- `AllSuccess { count }`, `AllFailure { count, results }`, `PartialFailure { successes, failures }`

**Integration points:**
- `WorkerPool::worker_loop` -- add `batch_worker_loop` variant; mode enum dispatch
- `PythonHandler::invoke` -- add `invoke_async` using `GILRef` from `pyo3-async-runtimes`
- `OffsetTracker` -- unchanged; batch acks call `record_ack` per message
- `QueueManager` -- add `ack_batch(topic, N)` for batch-level inflight decrement

### Critical Pitfalls

1. **GIL held for entire batch duration** -- If batch invokes Python holding GIL for entire batch (e.g., 100 msgs x 10ms = 1 second), all Python code blocked. Mitigation: per-message invocation within batch, or conservative `max_batch_size` config (<50 for CPU-bound).

2. **Batch result ambiguity** -- Reusing `ExecutionResult` for batch loses per-message outcomes. Mitigation: define `BatchExecutionResult` enum with `AllSuccess`, `AllFailure`, `PartialFailure` variants. Never return single `ExecutionResult` from batch handler.

3. **Offset commit at wrong granularity** -- Committing per-message inside batch breaks all-or-nothing semantics. Mitigation: only call `offset_coordinator.record_ack` after batch handler returns `AllOk`.

4. **Async coroutine via `spawn_blocking`** -- Using `spawn_blocking` for async Python coroutine runs it synchronously, defeating async. Mitigation: use `into_future` which returns a Tokio-compatible Future; `await` it directly in worker loop.

5. **`CancellationToken` not draining accumulated batch** -- On shutdown, batch accumulator may drop messages if cancellation fires during timeout wait. Mitigation: use `tokio::select!` with biased cancellation branch that drains before exiting.

## Implications for Roadmap

Based on research, suggested phase structure:

### Phase 1: HandlerMode Enum and Base Infrastructure
**Rationale:** The `HandlerMode` enum is the core abstraction that gates all other work. No batch or async functionality is possible without it. This phase establishes the foundation.

**Delivers:** `HandlerMode` enum (SingleSync, SingleAsync, BatchSync, BatchAsync), `BatchExecutionResult` type, `BatchAccumulator` struct (no dispatch yet).

**Implements:** Architecture component: `HandlerMode` enum, `BatchResult` type.

**Avoids:** Pitfall 2 (batch result ambiguity) -- `BatchExecutionResult` defined before any batch invocation.

### Phase 2: Sync Batch Handler Implementation
**Rationale:** Sync batch is lower risk than async (existing `spawn_blocking` pattern). Establishes batch accumulation at worker level, per-partition ordering constraint enforcement, and offset commit-at-batch-boundary pattern.

**Delivers:** `BatchSyncHandler::invoke` via `spawn_blocking`, `batch_worker_loop` in `WorkerPool`, `QueueManager::ack_batch`, batch result processing (all-or-nothing).

**Uses:** `pyo3` `Python::with_gil`, `tokio::spawn_blocking`.

**Implements:** Architecture component: `BatchHandler`, `BatchAccumulator` integration.

**Avoids:** Pitfall 1 (GIL held too long) -- configure conservative `max_batch_size`; Pitfall 6 (ordering) -- per-partition batch channels enforced here.

### Phase 3: Async Python Handler Support
**Rationale:** Async support requires `pyo3-async-runtimes` integration (new dependency) and is higher complexity. Defer until sync batch is proven.

**Delivers:** `AsyncPythonHandler::invoke_async` via `into_future`, `AsyncExecutor` trait, async mode dispatch in worker loop.

**Uses:** `pyo3-async-runtimes::tokio::into_future`, `GILRef` pattern.

**Implements:** Architecture component: `AsyncPythonHandler`, `AsyncExecutor` trait.

**Avoids:** Pitfall 4 (spawn_blocking for async) -- correctly uses `into_future`; verify async handler yields during Python-side await.

### Phase 4: Partial Failure and Retry Integration
**Rationale:** Partial batch failure tracking adds significant complexity (per-message outcome mapping to offsets). Only after all-or-nothing batch is working.

**Delivers:** `BatchExecutionResult::PartialFailure` with per-message tracking, retry/DLQ integration for batch, retry messages bypass accumulator.

**Avoids:** Pitfall 5 (batch timeout + retry interaction) -- retry stream separated from batch accumulation.

### Phase 5: Shutdown Drain and Integration Testing
**Rationale:** Graceful shutdown correctness must be verified. End-to-end tests for batch + async + retry + offset commit scenarios.

**Delivers:** `CancellationToken` draining batch accumulator, integration tests for all scenarios in "Looks Done But Isn't" checklist.

**Avoids:** Pitfall 7 (cancellation loses accumulated messages) -- `tokio::select!` with biased cancellation branch.

### Phase Ordering Rationale

1. **HandlerMode first** -- Without it, no way to distinguish sync/async or single/batch execution paths. All other phases depend on this enum.
2. **Sync batch before async** -- Sync batch uses existing `spawn_blocking` pattern (known good). Async introduces new dependency and GIL-release-during-await complexity.
3. **Batch before partial failure** -- All-or-nothing semantics are simpler to implement and verify. Partial failure tracking is optional enhancement.
4. **Partial failure before shutdown drain** -- Shutdown drain needs to correctly handle all result types including partial failure outcomes.
5. **Integration testing last** -- Unit tests can verify components in isolation, but offset commit semantics and ordering guarantees require end-to-end tests.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 2 (Sync Batch):** `BatchAccumulator` timing details need implementation verification. `tokio::select!` race behavior with high-throughput message arrival not fully characterized.
- **Phase 3 (Async):** `pyo3-async-runtimes` GILRef API exact version compatibility with KafPy's Tokio version -- needs Context7 verification.

Phases with standard patterns (skip research-phase):
- **Phase 1 (HandlerMode):** Enum definition is straightforward pattern matching on existing code.
- **Phase 5 (Shutdown):** `CancellationToken` + `tokio::select!` is well-documented Tokio pattern.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | Based on Context7 verified docs.rs for pyo3-async-runtimes 0.27.0 and confirmed existing dependencies |
| Features | MEDIUM | Based on analysis of rdkafka, aiokafka, Faust competitors; not validated against actual user feedback |
| Architecture | MEDIUM | Structural recommendations sound; BatchAccumulator timing details need implementation verification |
| Pitfalls | MEDIUM | Based on PyO3 0.27, pyo3-async-runtimes, Tokio patterns, and codebase analysis; some pitfalls (e.g., GIL hold duration) require empirical verification |

**Overall confidence:** MEDIUM

### Gaps to Address

- **BatchAccumulator timing:** The `tokio::select!` loop with timer vs. message arrival race behavior under high throughput not fully characterized. May need simulation or prototyping.
- **Async handler GILRef API:** Exact API for `GILRef::acquire()` pattern needs Context7 verification against pyo3-async-runtimes docs.
- **Partial failure efficiency:** Per-message outcome tracking overhead vs. simple all-or-nothing needs benchmarking.

## Sources

### Primary (HIGH confidence)
- [docs.rs: pyo3-async-runtimes 0.27.0](https://docs.rs/pyo3-async-runtimes/0.27.0/pyo3_async_runtimes/tokio/) -- `into_future` API, GIL behavior during await
- [KafPy Cargo.toml](file:///home/nghiem/project/KafPy/Cargo.toml) -- existing stack confirmed
- [KafPy src/python/handler.rs](file:///home/nghiem/project/KafPy/src/python/handler.rs) -- existing `invoke` pattern confirmed
- [KafPy src/worker_pool/mod.rs](file:///home/nghiem/project/KafPy/src/worker_pool/mod.rs) -- worker loop pattern confirmed

### Secondary (MEDIUM confidence)
- [Faust streaming/faust](https://github.com/faust/faust) -- `@app.agent` with `batch=True` decorator; Python streaming framework patterns
- [aiokafka batch API](https://github.com/aio-libs/aiokafka) -- application-level batch accumulation patterns
- [Tokio select! documentation](https://tokio.rs/tokio/tutorial/select) -- batch accumulation timing patterns
- [rdkafka StreamConsumer API](https://docs.confluent.io/kafka-clients/python/current/) -- message yield one-at-a-time confirmed

### Tertiary (LOW confidence)
- [PyO3 GitHub issues: GIL release during Python await](https://github.com/PyO3/pyo3) -- needs specific version verification

---
*Research completed: 2026-04-18*
*Ready for roadmap: yes*