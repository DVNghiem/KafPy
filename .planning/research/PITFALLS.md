# Pitfalls Research: Batch and Async Python Handler Execution

**Domain:** PyO3 Tokio message processing — adding batch and async Python coroutine support to an existing single-message consumer framework
**Researched:** 2026-04-18
**Confidence:** MEDIUM — Based on PyO3 0.27, pyo3-async-runtimes, Tokio patterns, and analysis of existing codebase

---

## Critical Pitfalls

### Pitfall 1: GIL Held for Entire Batch Duration

**What goes wrong:**
The existing `PythonHandler::invoke` correctly releases the Tokio thread per message via `spawn_blocking`. But when batching accumulates N messages in a Rust-side buffer before invoking Python, the GIL is held for the entire batch invocation. If the batch contains 100 messages and the Python handler takes 10ms per message, the GIL is held for 1 second straight — blocking all other Python code on the process, including any async Python tasks that need the GIL.

**Why it happens:**
`spawn_blocking` acquires the GIL once at entry and holds it until the closure returns. For single-message handlers, this is a brief window. For batch handlers, the entire batch processing runs inside one `spawn_blocking` call, one `Python::with` entry, and one GIL acquisition. Python's C API does not automatically release and reacquire the GIL for you.

**How to avoid:**
Consider calling Python per-message inside the batch, still within `spawn_blocking`, to allow the GIL to be released between messages. Alternatively, chunk the batch into sub-batches with GIL release between them. Document the batch GIL hold time as a configuration concern and set `max_batch_size` conservatively for CPU-bound handlers.

**Warning signs:**
- Batch size increases and Python process appears to freeze or block other operations
- `tokio-console` shows one `spawn_blocking` task holding for long durations
- Async Python handlers deadlock because the GIL is held by a synchronous batch worker

**Phase to address:** EXEC-01 through EXEC-05 — batch handler design must account for GIL hold duration

---

### Pitfall 2: Batch Result Ambiguity — Per-Message Outcomes Lost

**What goes wrong:**
The existing `ExecutionResult` models one result per one message: `Ok`, `Error { reason, exception, traceback }`, or `Rejected { reason, reason_str }`. With a batch of N messages, a single `ExecutionResult` cannot represent mixed outcomes (e.g., 98 messages succeed, 1 fails, 1 is rejected). If you return `Ok` for a batch where one message failed, offsets are incorrectly acked. If you return `Error`, correctly processed messages are unnecessarily retried.

**Why it happens:**
The existing worker loop does:
```rust
let result = handler.invoke(&ctx, msg.clone()).await;
let _outcome = executor.execute(&ctx, &msg, &result);
match result {
    ExecutionResult::Ok => { offset_coordinator.record_ack(...) }
    // ...
}
```
Each message gets one result and one offset ack. Batch execution collapses N messages into one call site.

**How to avoid:**
Model batch results explicitly:
```rust
// Option A: Full failure — treat batch as one unit, fail all on any error
enum BatchResult {
    AllOk(Vec<i64> /* acked offsets */),
    AllFailed { reason: FailureReason, offsets: Vec<i64> },
}

// Option B: Partial success tracking — executor decides what to do with mixed results
enum BatchResult {
    AllOk(Vec<i64>),
    PartialOk { succeeded: Vec<i64>, failed: Vec<(i64, FailureReason)> },
    AllFailed { reason: FailureReason, offsets: Vec<i64> },
}
```

Preserve delivery guarantees by deferring per-message outcomes to the retry/retry logic. Option A is simpler; Option B is more efficient but more complex.

**Warning signs:**
- `ExecutionResult` used as the return type of a batch handler invocation
- Offset acks happening inside the Python batch callback (should be after it returns)
- No per-message outcome tracking in the worker loop after batch returns

**Phase to address:** EXEC-07 — batch result modeling must define what "success" and "failure" mean for a batch

---

### Pitfall 3: Offset Commit Semantics Break for Batch

**What goes wrong:**
The offset commit model (`OffsetTracker::ack` per message, `should_commit` checking `committed_offset + 1`) assumes messages are processed one at a time and offsets advance sequentially per partition. With a batch, multiple offsets for the same partition arrive at the commit layer almost simultaneously. If batch execution is async, out-of-order completion becomes likely: offset 50 might commit before offset 49 (still running async).

The existing code guards with `BTreeSet` pending buffering, but the commit eligibility logic `committed_offset + 1` still applies. The risk: offset 50 commits (fills gap), then offset 49 fails and goes to DLQ — but 50 is already committed.

**Why it happens:**
The worker loop currently does `offset_coordinator.record_ack(topic, partition, offset)` immediately after `ExecutionResult::Ok`. With async batch handlers, the acknowledgment can't happen until the async future completes. If offset 50 completes before offset 49, 50 gets acked first. The BTreeSet buffering handles out-of-order `ack` calls, but the semantic question is whether "batch success" means "all N messages succeeded" — and if one fails mid-batch, what happens to the offsets already committed?

**How to avoid:**
Tie offset commit to batch boundary, not per-message:
1. Accumulate messages into a batch buffer in Rust
2. Invoke Python batch handler
3. Only after batch returns `AllOk`, call `offset_coordinator.record_ack` for all offsets in the batch
4. If batch fails, none are acked (or failed offsets are marked via `mark_failed`)

Alternatively, use the existing `mark_failed` path for failed offsets and continue, but only `record_ack` for offsets that completed successfully.

**Warning signs:**
- `offset_coordinator.record_ack` inside the Python callback execution path
- Batch result handling that doesn't interact with the offset coordinator at all
- Per-message `record_ack` calls happening from multiple workers simultaneously for the same partition

**Phase to address:** EXEC-06 — GIL minimal usage requires careful coordination of when offsets are committed

---

### Pitfall 4: Async Python Handlers + Tokio `select!` — Lost Messages

**What goes wrong:**
`pyo3-async-runtimes` allows Rust to call async Python coroutines. But the `PythonHandler::invoke` pattern uses `tokio::task::spawn_blocking`, which blocks a Tokio thread until the blocking operation completes. An async Python coroutine is a Python-level future — calling it via `spawn_blocking` does not make it run concurrently on Tokio. The coroutine runs to completion synchronously inside `spawn_blocking`.

If you instead use `future_into_py` to bridge a Python async coroutine into Tokio, the resulting `Send + Sync` future can be awaited. But `future_into_py` requires the Python coroutine to be `await`ed within a `Python::with` scope that holds the GIL. If you `await` the Python future without holding the GIL (outside `Python::with`), it panics.

**Why it happens:**
`pyo3-async-runtimes` bridges Python async to Rust async by running the Python coroutine inside a `Python::with` block. The GIL is held only for the duration of `future_into_py`. Inside the returned `Future`, the Python future may yield (await Python-side awaits), at which point the GIL is released. But the bridge is subtle: you must use the provided `AsyncClient` or `spawn` utilities, not raw `spawn_blocking`.

**How to avoid:**
For async Python handlers, use the pyo3-async-runtimes idioms:
```rust
// Wrapping an async Python handler into a Tokio-aware future
use pyo3_async_runtimes::tokio::future_into_py;

let py_future = Python::with_gil(|py| {
    let coroutine = callback.as_ref(py);
    coroutine.call1(py, (py_msg,))?
});

// Now py_future is a Rust Future — await it on Tokio
let result = future_into_py(py_future).await?;
```

Do not use `spawn_blocking` for async handlers — `spawn_blocking` is for synchronous blocking code. For async handlers, the Tokio task should `await` the Python future directly.

**Warning signs:**
- `spawn_blocking` used with a coroutine object
- `Python::with` inside an `async fn` that does not use `future_into_py`
- `PyErr` appearing in async function return types (should be converted before async boundary)

**Phase to address:** EXEC-04 — async Python handler support via pyo3-async-runtimes

---

### Pitfall 5: Batch Timeout Interaction with Retries

**What goes wrong:**
Batch accumulation waits up to `max_batch_wait_time` to fill a batch. If messages are retryable failures that are being retried, they may sit in the batch accumulator while new messages are added. When the timeout fires and the batch dispatches with some retried messages mixed with new messages, retry logic and offset tracking become confused: the retry attempt count may be wrong, and the batch retry semantics are unclear (retry whole batch or just the failed message?).

**Why it happens:**
The retry coordinator (`RetryCoordinator::record_failure`) tracks per-message state by topic-partition-offset. When a message fails and is scheduled for retry, it gets re-dispatched. If batch accumulation is happening, the retry and the new messages accumulate together. The `record_failure` logic (max_attempts, backoff delay) is per-message, but batch dispatch bundles them.

**How to avoid:**
Separate retry streams from new message streams at the batch accumulator level. Retried messages should skip batch accumulation and go directly to a retry-specific queue. Alternatively, tag each message with its retry attempt count in the batch and handle retries separately in the batch handler result processing.

**Warning signs:**
- `RetryCoordinator::record_failure` called from within batch execution
- Batch handler result processing that mixes retried messages with new messages without attempt count tracking
- Messages appearing in a batch that have already been retried N times

**Phase to address:** EXEC-02 — batch accumulation design must account for retry state

---

### Pitfall 6: Per-Partition Ordering Guarantees Broken by Parallel Batch Execution

**What goes wrong:**
Kafka guarantees per-partition ordering. If the worker loop dispatches messages from the same partition in parallel batches (e.g., batch of offsets 100-109 dispatched to one worker, batch of 110-119 to another worker simultaneously), ordering is violated for that partition. Batch execution that splits a partition's messages across workers loses ordering guarantees.

**Why it happens:**
The existing design uses a single `mpsc::Receiver` per handler (per handler_id). The worker pool fans out to multiple workers each with their own receiver. Batch accumulation happens per-worker from its receiver. Two workers processing the same partition concurrently breaks ordering.

**How to avoid:**
Ensure batch accumulation respects partition boundaries: messages from the same partition must go to the same batch (and thus the same worker). This may require a per-partition dispatch channel feeding into batch accumulators, rather than per-handler channels. Alternatively, enforce that batch handlers are single-worker only for the partition's message stream.

**Warning signs:**
- Multiple `mpsc::Receiver` instances for the same handler_id
- Worker pool distributing messages from the same partition to different workers
- Consumer latency increasing because ordering constraints force single-threaded processing despite batch configuration

**Phase to address:** EXEC-01 through EXEC-05 — batch architecture must maintain per-partition ordering

---

### Pitfall 7: `CancellationToken` Scope Does Not Cover Accumulated Batch

**What goes wrong:**
If a batch is mid-accumulation (waiting for `max_batch_wait_time` to fill), a `CancellationToken` cancel signal arrives. The accumulator should drain and dispatch what it has, not drop messages. But if the batch accumulator is a separate async task waiting on a timer, it may not respond to the token until the timer fires. Messages can be stuck in a batch accumulator that never dispatches.

**Why it happens:**
Batch accumulation typically uses `tokio::time::timeout` or a timer future. `CancellationToken::cancelled()` is a separate future. If the timer future wins (batch fills before cancellation), dispatch happens. If cancellation fires first but the timer is still pending, the accumulator task exits without dispatching the accumulated messages.

**How to avoid:**
Use `tokio::select!` with `cancelled` as a branch alongside the batch-full and batch-timeout conditions:
```rust
tokio::select! {
    biased; // so cancellation always wins if it fires
    _ = shutdown_token.cancelled() => {
        // Drain accumulated batch before exiting
        if !batch.is_empty() {
            self.dispatch_batch(batch).await;
        }
        return;
    }
    result = self.accumulator.recv() => {
        match result {
            Some(msg) => batch.push(msg),
            None => return,
        }
    }
    _ = tokio::time::sleep(max_batch_wait_time) => {
        self.dispatch_batch(batch).await;
    }
}
```

**Warning signs:**
- `tokio::time::timeout` used without a `CancellationToken` branch
- Batch accumulator task that can be cancelled without dispatching accumulated messages
- Messages lost on graceful shutdown (not in a queue, not in a batch, not in flight)

**Phase to address:** EXEC-01 — batch handler registration and shutdown coordination

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|---------|-------------------|----------------|-----------------|
| Returning `ExecutionResult` from batch handler | Reuses existing type | Cannot represent partial batch outcomes; breaks offset semantics | Never — requires new `BatchResult` type |
| Using `spawn_blocking` for async coroutine | Simpler code path | Coroutine runs sync inside blocking task, no async benefit | Never for async handlers |
| Committing offsets inside Python callback | Simpler Rust code | GIL held longer, Python controls commit timing, breaks Rust-side orchestration | Never |
| Single `ExecutionResult` for batch | Simpler type | Cannot track per-message outcomes | Only for "all-or-nothing" batch semantics |
| One batch per handler across all partitions | Simpler accumulator design | Violates per-partition ordering | Never |

---

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| `spawn_blocking` + async coroutine | Treating a `Coroutine` object as a blocking callable | Use `pyo3-async-runtimes` `future_into_py` to bridge to Tokio, then `await` |
| Batch + OffsetTracker | Committing offsets before batch returns | Only `record_ack` after batch handler returns `AllOk` |
| Batch + RetryCoordinator | Retried messages mixed in accumulating batch | Separate retry queue from new message queue at accumulator |
| Batch + BackpressurePolicy | Not checking queue depth during batch accumulation | Batch accumulator should respect per-handler queue capacity |
| Async Python handler + Tokio `select!` | Awaiting Python future outside `Python::with` scope | Use `future_into_py` which handles the GIL bridge internally |

---

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| GIL held for large batch | Other Python async tasks block, apparent freeze | Set `max_batch_size` conservatively; consider sub-batching for CPU-bound handlers | Always for batches > ~50 messages |
| Batch timeout too long | High latency for individual messages | Set `max_batch_wait_time` based on SLO; prefer smaller timeouts (5-50ms) | When latency SLO is tight |
| Per-partition serial batch processing | Throughput limited to one partition's message rate | Allow concurrent batch workers for different partitions | At high throughput with many partitions |
| Async handler GIL contention | Async tasks slow despite Tokio concurrency | Limit concurrent async Python handlers; profile with `tokio-console` | When many async handlers are awaited simultaneously |

---

## "Looks Done But Isn't" Checklist

- [ ] **Batch result type:** A `BatchResult` enum exists that can represent partial success (succeeded/failed per message) — verify NOT just reusing `ExecutionResult`
- [ ] **Offset commit at batch boundary:** `offset_coordinator.record_ack` is called only after batch handler returns `AllOk` or per-message outcomes are resolved — verify no per-message `record_ack` inside batch execution
- [ ] **Async handler bridge:** Async Python coroutines use `future_into_py` / pyo3-async-runtimes primitives — verify NOT `spawn_blocking`
- [ ] **Partition ordering preserved:** Batch accumulation is per-partition — verify no two workers process the same partition's messages concurrently
- [ ] **Shutdown drains accumulator:** `CancellationToken` cancellation drains accumulated messages before exiting — verify with integration test
- [ ] **GIL hold time documented:** Batch size limits are configured with GIL hold time in mind — verify max_batch_size chosen to keep GIL hold < 100ms for I/O-bound, < 10ms for CPU-bound
- [ ] **Retry messages bypass accumulator:** Retried messages go directly to retry queue, not batch accumulator — verify with integration test exercising retries

---

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| GIL held for entire batch | MEDIUM — apparent freeze, async handler deadlock | Reduce max_batch_size; add sub-batching; switch to per-message async handlers |
| Batch result ambiguity | HIGH — incorrect offsets committed, messages retried unnecessarily | Add `BatchResult` type with per-message tracking; audit all `ExecutionResult` usages in batch path |
| Offset commit semantics broken | HIGH — committed offsets don't match processed messages | Move `record_ack` to batch boundary; add integration test with mixed batch outcomes |
| Async coroutine in spawn_blocking | MEDIUM — async handler runs synchronously, no concurrency benefit | Switch to `future_into_py` bridge; add test to verify async handler yields |
| Batch timeout loses messages | HIGH — messages lost on shutdown | Fix with `tokio::select!` + biased cancellation; add integration test |
| Partition ordering violated | MEDIUM — Kafka ordering guarantee broken | Add per-partition channel to batch accumulator; add ordering test |

---

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| GIL held for entire batch | EXEC-01/02: Batch handler registration and accumulation | `tokio-console` observation: batch `spawn_blocking` task hold time < threshold |
| Batch result ambiguity | EXEC-07: Batch result modeling | Compile-time verification: `BatchResult` type used, not `ExecutionResult` |
| Offset commit semantics broken | EXEC-06/07: GIL minimal usage + batch result | Integration test: verify offsets committed only after batch success |
| Async coroutine in spawn_blocking | EXEC-04: Async Python handlers | Integration test: async handler yields while awaiting Python-side await |
| Batch timeout loses messages | EXEC-01: Batch handler registration | Integration test: cancel during accumulation, verify no message loss |
| Partition ordering violated | EXEC-01/05: Batch architecture | Unit test: two workers same partition → compile error or test failure |
| Retry in batch accumulator | EXEC-02: Batch accumulation | Integration test: retry scheduled, verify message bypasses accumulator |

---

## Sources

- PyO3 0.27 documentation: `spawn_blocking` GIL behavior, `future_into_py` for async bridging
- `pyo3-async-runtimes` crate: async Python coroutine into Tokio future bridge
- `src/python/handler.rs`: existing single-message `invoke` pattern confirms `spawn_blocking` GIL usage
- `src/worker_pool/mod.rs`: existing worker loop confirms per-message `record_ack` pattern
- `src/coordinator/retry_coordinator.rs`: per-message retry state tracking — needs separation from batch
- `src/coordinator/offset_tracker.rs`: `ack` + `should_commit` model — requires batch-level adjustment
- `src/dispatcher/mod.rs`: per-handler channel registration — partition boundary concern for batch
- `src/routing/chain.rs`: routing decision model — `RoutingDecision` not currently batch-aware
- Tokio documentation: `CancellationToken` + `select!` interaction patterns
- PyO3 GitHub issues: GIL release during Python await (relevant for async coroutine yield points)

---
*Pitfalls research for: Batch and async Python handler execution v1.6*
*Researched: 2026-04-18*