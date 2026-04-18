# Architecture Research

**Domain:** PyO3 Kafka Consumer Framework - Batch and Async Handler Execution
**Researched:** 2026-04-18
**Confidence:** MEDIUM

## Executive Summary

KafPy is a Rust-backed Kafka consumer where the Rust core handles protocol (rdkafka + Tokio) and Python holds business logic (handlers). Adding batch and async handlers requires: (1) a `BatchExecutor` that accumulates messages into vectors until `max_batch_size` OR `max_batch_wait` timeout, (2) async handler support via `pyo3_async_runtime` GIL-ref spawn returning a future that `.await` releases the GIL properly within Tokio, (3) a `HandlerMode` enum (single-sync, single-async, batch-sync, batch-async) that unifies the execution interface, and (4) batch result modeling where "full success" advances all offsets atomically and "full failure" routes all to retry/DLQ. Integration points are: `WorkerPool::worker_loop` for single-message processing, `OffsetTracker` for batch ack, and `QueueManager` for inflight tracking.

## System Overview

```
┌──────────────────────────────────────────────────────────────────────┐
│                     ConsumerDispatcher                                │
│  (ConsumerRunner + Dispatcher + RoutingChain — owns message stream)   │
└──────────────────────────────┬───────────────────────────────────────┘
                               │ spawns worker_loop(s) via tokio::spawn
                               ▼
┌──────────────────────────────────────────────────────────────────────┐
│                         WorkerPool                                    │
│  N Tokio tasks polling mpsc::Receiver<OwnedMessage>                   │
│  Per worker: active_message tracking for graceful shutdown            │
└──────────────┬─────────────────────────────────┬────────────────────┘
               │                                 │
        ┌──────▼──────┐                   ┌──────▼──────┐
        │  worker 0   │                   │  worker N   │
        │  (rx.recv)  │                   │  (rx.recv)  │
        └──────┬──────┘                   └──────┬──────┘
               │                                 │
         ┌─────▼─────────────────────────────┐    │
         │         worker_loop (tokio::select!)  │
         │  1. poll mpsc receiver            │
         │  2. invoke PythonHandler::invoke() │
         │  3. executor.execute() for policy │
         │  4. retry/DLQ routing on failure  │
         │  5. offset_coordinator.record_ack()│
         │  6. queue_manager.ack()            │
         └─────┬─────────────────────────────┘
               │                                 │
     ┌─────────┴─────────┐            ┌──────────┴──────────┐
     │   PythonHandler   │            │   AsyncPythonHandler│
     │  (spawn_blocking) │            │  (pyo3-async-rt)   │
     └───────────────────┘            └───────────────────┘
```

### Component Responsibilities

| Component | Responsibility | Implementation |
|-----------|----------------|----------------|
| `ConsumerDispatcher` | Owns consumer stream, routes to handler queues, pause/resume | `src/dispatcher/mod.rs` |
| `Dispatcher` | Routes `OwnedMessage` to per-handler Tokio mpsc channels | `src/dispatcher/mod.rs` |
| `QueueManager` | Tracks queue depth and inflight per handler via AtomicUsize | `src/dispatcher/queue_manager.rs` |
| `WorkerPool` | Manages N Tokio workers, graceful shutdown via CancellationToken | `src/worker_pool/mod.rs` |
| `PythonHandler` | Invokes sync Python callable via `spawn_blocking`, GIL only inside `Python::with_gil` | `src/python/handler.rs` |
| `Executor` trait | Post-execution policy decisions (ack/retry/rejected) | `src/python/executor.rs` |
| `OffsetTracker` | Per-TP BTreeSet-based out-of-order buffering, highest-contiguous algorithm | `src/coordinator/offset_tracker.rs` |
| `RetryCoordinator` | 3-tuple (should_retry, should_dlq, delay) per message retry state | `src/coordinator/retry_coordinator.rs` |
| `RoutingChain` | Chains routers: pattern → header → key → python → default | `src/routing/chain.rs` |

## Batch Handler Integration

### Batch Accumulation

A `BatchExecutor` (or batch-mode variant of the executor) accumulates messages into a `Vec<OwnedMessage>` until either:
- `max_batch_size` messages accumulated
- `max_batch_wait` duration elapsed (Tokio `sleep` race against message arrival)

```rust
// New: batch accumulation layer
pub struct BatchAccumulator {
    messages: Vec<OwnedMessage>,
    max_size: usize,
    max_wait: Duration,
}

impl BatchAccumulator {
    pub async fn accumulate(&mut self, mut rx: mpsc::Receiver<OwnedMessage>) {
        let timeout = tokio::time::sleep(self.max_wait);
        tokio::pin!(timeout);

        loop {
            tokio::select! {
                Some(msg) = rx.recv() => {
                    self.messages.push(msg);
                    if self.messages.len() >= self.max_size {
                        return; // batch ready
                    }
                }
                _ = &mut timeout => {
                    return; // timer expired
                }
            }
        }
    }
}
```

### Queue Management Changes

For batch handlers, `queue_manager.ack()` needs a batch variant:

```rust
// Current (single): queue_manager.ack(&topic, 1);
// Batch needed:     queue_manager.ack_batch(&topic, batch_size);

impl QueueManager {
    pub fn ack_batch(&self, topic: &str, count: usize) {
        // Decrement inflight by count atomically
        // Decrement queue_depth by count atomically
    }
}
```

### Integration with Existing Queue System

Batch handlers register via the same `register_handler()` flow. The `mpsc::Receiver<OwnedMessage>` feeds into a batch accumulator. The `BackpressurePolicy` still applies per-message at dispatch time (DISP-08/DISP-15), not at batch time.

**Key insight:** The queue depth and inflight counters in `QueueManager` track individual messages, not batches. When a batch is dispatched, the channel sends N messages. The semaphore permit (DISP-15) must be acquired N times for N messages.

### Batch-Size Backpressure Interaction

When `max_batch_size=50` and queue has only 10 messages, the accumulator waits for `max_batch_wait` timeout. Backpressure still applies to dispatch — if queue is full at dispatch time, `send_with_policy` returns `Backpressure` before the batch accumulator ever sees the messages.

## Async Python Handler Integration

### Tokio Runtime Compatibility

The Tokio runtime is already running (the whole system is Tokio-based). Async Python handlers require `pyo3-async-runtimes` which provides a `GILRef` type that can be `.await`ed to safely access Python objects from async contexts.

The critical requirement: **GIL must only be held within the `.await` window**, not across yield points.

```rust
// Current sync handler (PythonHandler::invoke):
tokio::task::spawn_blocking(move || {
    Python::with_gil(|py| {
        // GIL held for entire callback
        callback.call1(py, (py_msg,))
    })
}).await

// Async handler approach (pyo3-async-runtimes):
// GIL acquired inside GILRef await, released between yield points
let gil_ref = GILRef::acquire().await;
let result = gil_ref.python(|py| {
    callback.call1(py, (py_msg,))
}).await;
drop(gil_ref); // GIL released here
```

### Integration Points

Async handlers integrate at the `PythonHandler` / new `AsyncPythonHandler` level. The `worker_loop` remains unchanged — it awaits `handler.invoke()`. The difference is that async handlers yield to Tokio while waiting for GIL, allowing other tasks to run.

**Runtime requirement:** `pyo3-async-runtimes` `GILRef::acquire()` requires a Tokio context. Since `worker_loop` runs inside Tokio tasks, this is satisfied.

### Async Handler Registration

Python coroutines are stored as `Py<PyAny>` (same as sync callables). The runtime detects whether the callable is a coroutine via `inspect.iscoroutinefunction()` and routes to the appropriate invoker:

```rust
pub enum HandlerMode {
    SyncSingle(Arc<PythonHandler>),
    AsyncSingle(Arc<AsyncPythonHandler>),
    BatchSync(Arc<BatchSyncHandler>),
    BatchAsync(Arc<BatchAsyncHandler>),
}
```

## Offset Commit Flow Changes

### Single-Message Commit (Existing)

```
handler.invoke() → ExecutionResult::Ok
    → retry_coordinator.record_success()
    → offset_coordinator.record_ack()  // BTreeSet insert, advance contiguous cursor
    → queue_manager.ack()               // inflight--
    → (later) should_commit() → true → store_offset() + commit()
```

### Batch Commit Requirements

For batch handlers, the commit semantics must preserve "all or nothing" at the batch level:

```
BatchExecutionResult::AllSuccess(batch)
    → for each msg in batch: offset_coordinator.record_ack()  // N acks
    → queue_manager.ack_batch(topic, N)                      // N inflight decrements

BatchExecutionResult::AllFailure(batch)
    → for each msg in batch: offset_coordinator.mark_failed()
    → retry_coordinator.record_failure_batch() // retry state per message
    → DLQ routing per message
```

**Critical constraint:** `OffsetTracker` already has per-offset tracking via BTreeSet. For batch, each message in the batch gets its own ack recorded individually. The highest-contiguous algorithm works unchanged — batch success advances offsets the same as single success.

**Partial batch failure** (some messages succeed, some fail) is modeled as two sub-batches: success sub-batch acks normally, failure sub-batch goes through retry/DLQ.

## New Components Required

### 1. `BatchAccumulator` (src/batch/accumulator.rs)

Accumulates messages into batches. Plugs into `WorkerPool` as an alternative polling strategy.

### 2. `BatchHandler` (src/batch/handler.rs)

Invokes sync Python handler on a `Vec<OwnedMessage>`. Returns `BatchExecutionResult`.

### 3. `AsyncPythonHandler` (src/python/async_handler.rs)

Invokes async Python coroutine via `pyo3-async-runtimes` `GILRef`. Returns `Result<ExecutionResult, HandlerError>`.

### 4. `BatchExecutionResult` (src/batch/result.rs)

```rust
pub enum BatchExecutionResult {
    AllSuccess { count: usize },
    AllFailure { count: usize, results: Vec<ExecutionResult> },
    PartialFailure { successes: usize, failures: Vec<ExecutionResult> },
}
```

### 5. `HandlerMode` enum (src/handler/mode.rs)

Unifies execution interface across all four modes (single-sync, single-async, batch-sync, batch-async).

### 6. `AsyncExecutor` trait (src/python/async_executor.rs)

Post-execution policy for async handlers (extends `Executor`).

## Data Flow Changes

### Single-Message Flow (unchanged)

```
ConsumerDispatcher::run()
  → stream.next() → OwnedMessage
  → dispatcher.send_with_policy() → DispatchOutcome
  → WorkerPool::worker_loop() receives message
  → handler.invoke(&ctx, msg) → ExecutionResult
  → executor.execute() → ExecutorOutcome
  → [on Ok] retry_coordinator.record_success(), offset_coordinator.record_ack(), queue_manager.ack()
  → [on Error] retry_coordinator.record_failure() → (should_retry, should_dlq, delay)
```

### Batch Flow (new)

```
ConsumerDispatcher::run() [unchanged]
  → WorkerPool::batch_worker_loop() receives message
  → BatchAccumulator::accumulate() waits for size or timeout
  → BatchHandler::invoke(batch) → BatchExecutionResult
  → [on AllSuccess] for each: offset_coordinator.record_ack(), queue_manager.ack()
  → [on AllFailure] for each: offset_coordinator.mark_failed(), retry_coordinator.record_failure(), DLQ
  → [on PartialFailure] split and handle each sub-batch
```

### Async Flow (new)

```
ConsumerDispatcher::run() [unchanged]
  → WorkerPool::worker_loop() receives message
  → AsyncPythonHandler::invoke(&ctx, msg) →.await→ ExecutionResult
  → [same post-execution flow as single-message]
```

## Key Architectural Decisions

### Decision 1: Batch accumulation at worker level, not dispatcher level

The dispatcher dispatches individual `OwnedMessage` values as it does today. The batch accumulation happens inside the worker task after dispatch, using a `tokio::select!` loop that races `rx.recv()` against a timeout.

**Why:** Minimizes changes to the dispatcher/queue infrastructure. Backpressure is still per-message at dispatch time.

### Decision 2: Async handler invoked as async fn inside worker loop

The `worker_loop` `async fn` can `.await` an async handler directly. `spawn_blocking` is only used for sync handlers.

**Why:** Keeps the worker loop as the central control point for both sync and async handlers.

### Decision 3: Batch result model is pessimistic (all-or-nothing by default)

Batch execution returns `AllSuccess`, `AllFailure`, or `PartialFailure`. `PartialFailure` splits into success/failure sub-batches processed independently.

**Why:** Per-message outcomes within a batch add complexity. Start with all-or-nothing semantics and defer per-message granularity to future work.

### Decision 4: GIL minimal usage principle extends to async

For async handlers, GIL is held only inside the `gil_ref.python()` closure (within an `.await`), not across yield points. `pyo3-async-runtimes` manages this via `GILRef`.

**Why:** Consistent with EXEC-06 (GIL minimal usage — no GIL held across Rust-side orchestration).

### Decision 5: Handler mode registered at handler creation time

When registering a handler, specify `HandlerMode` (single-sync, single-async, batch-sync, batch-async). The `WorkerPool` constructs the appropriate handler variant at startup.

**Why:** Avoids runtime type checking on every message. Mode is a construction-time decision.

## Anti-Patterns to Avoid

### Anti-Pattern 1: Batch accumulation blocking consumer pause

If batch accumulation holds the message channel receiver until timeout, it blocks `ConsumerDispatcher::run()` from processing pause/resume signals.

**Fix:** Use `tokio::select!` with a cancel token branch so pause/resume can interrupt batch waiting.

### Anti-Pattern 2: GIL held across async yield points

If async handler holds GIL across `.await`, it blocks all Python execution in all threads.

**Fix:** Use `pyo3-async-runtimes` `GILRef::acquire().await` pattern where GIL is acquired and released within await boundaries.

### Anti-Pattern 3: Batch size creates dead end at backpressure boundary

If `max_batch_size=100` and queue depth is 100 when backpressure kicks in, the batch will never fill and the accumulator waits indefinitely.

**Fix:** `max_batch_wait` provides a timeout escape hatch. Also consider: if `max_batch_size > queue_capacity / 2`, warn or error at registration time.

### Anti-Pattern 4: Async handler called with spawn_blocking

If async handlers are called via `spawn_blocking`, they block a Tokio worker thread for the duration, defeating the purpose of async.

**Fix:** Call async handlers directly with `.await` in the worker loop. `spawn_blocking` is only for sync handlers.

## Integration Points Summary

| Point | What's There Today | What's Needed |
|-------|---------------------|---------------|
| `WorkerPool::worker_loop` | Single-message polling and processing | Add `batch_worker_loop` variant; add mode enum dispatch |
| `PythonHandler::invoke` | Sync only via `spawn_blocking` | Add `AsyncPythonHandler::invoke` using `GILRef` |
| `Executor` trait | Single-message post-execution policy | Add `BatchExecutor` for batch-level policy |
| `OffsetTracker` | Per-offset BTreeSet, `record_ack(offset)` | Unchanged — batch acks call `record_ack` per message |
| `QueueManager` | `ack(topic, 1)` | Add `ack_batch(topic, N)` |
| `Dispatcher` | Per-topic channel, backpressure policy | Unchanged |
| `ConsumerDispatcher` | Routes to handler queues | Unchanged |
| `RetryCoordinator` | Per-message 3-tuple state | Unchanged for sync; needs batch variant for batch handlers |

## Confidence Assessment

| Area | Level | Notes |
|------|-------|-------|
| Batch integration with queue management | MEDIUM | Confirmed queue/inflight are per-message counters. Batch ack needs `ack_batch`. OffsetTracker unchanged. |
| Async handler with Tokio | MEDIUM | `pyo3-async-runtimes` GILRef pattern is correct but not verified against KafPy's specific Tokio version. Requires verification. |
| Architecture of new components | MEDIUM | Structural recommendations are sound. BatchAccumulator timing details need implementation verification. |
| GIL usage for async | MEDIUM | Pattern understood from `pyo3-async-runtimes` docs. Needs Context7 verification for exact API. |

## Sources

- [pyo3-async-runtimes GILRef documentation](https://docs.rs/pyo3-async-runtimes/) — async Python handler GIL management
- [Tokio select! documentation](https://tokio.rs/tokio/tutorial/select) — batch accumulation timing
- KafPy existing implementation — `src/python/handler.rs`, `src/worker_pool/mod.rs`, `src/coordinator/offset_tracker.rs`