---
gsd_version: "1.0"
phase: "25"
plan: "02"
status: completed
completed_at: "2026-04-18T04:15:00Z"
duration_minutes: 30
---

# Phase 25 Plan 02 Summary: BatchAccumulator and batch_worker_loop

**Implemented:** BatchAccumulator struct, invoke_batch method, batch_worker_loop integration

## One-liner

BatchAccumulator with fixed-window timeout, batch invoke via spawn_blocking, and backpressure-aware batch_worker_loop routing.

## Completed Tasks

| Task | Name | Status |
|------|------|--------|
| 1 | BatchAccumulator and PartitionAccumulator structs | Done |
| 2 | invoke_batch method on PythonHandler | Done |
| 3 | batch_worker_loop with BatchAccumulator integration | Done |

## Files Modified

- `src/worker_pool/mod.rs` — BatchAccumulator, PartitionAccumulator, batch_worker_loop, handle_batch_result_inline, WorkerPool::new routing
- `src/python/handler.rs` — invoke_batch method, BatchSync invoke_mode implementation, BatchExecutionResult import

## Decisions

- **D-01:** BatchAccumulator as dedicated struct in worker_pool/mod.rs — clean unit testing, mirrors OffsetTracker pattern
- **D-02:** Fixed-window timer — deadline set on first message arrival, never recalculated
- **D-04:** Inline iteration for batch results — batch_worker_loop owns batch during accumulation, result handling inline with batch in scope
- **HandlerMode::BatchSync routing:** WorkerPool::new detects BatchSync mode and routes to batch_worker_loop instead of worker_loop

## Key Artifacts

### BatchAccumulator
- `pub struct BatchAccumulator` with `parking_lot::Mutex<HashMap<i32, PartitionAccumulator>>`
- `PartitionAccumulator` private inner struct with fixed-window timer
- Methods: `add`, `flush_all`, `flush_partition`, `next_deadline`, `is_any_deadline_expired`, `would_fill_partition`, `is_empty`, `total_len`

### batch_worker_loop
- `async fn batch_worker_loop(...)` with tokio::select! on 3 branches:
  1. Deadline expiry → flush all partitions
  2. Message arrival → add to accumulator, flush on size or deadline
  3. Shutdown/cancellation → drain and exit
- Backpressure applied per-batch: flush first then block upstream

### invoke_batch
- `pub async fn invoke_batch(&self, ctx: &ExecutionContext, messages: Vec<OwnedMessage>) -> BatchExecutionResult`
- Builds `Vec<PyDict>` for batch, calls Python with `(py_batch,)`
- Returns `BatchExecutionResult::AllSuccess(Vec<i64>)` or `BatchExecutionResult::AllFailure(FailureReason)`

### WorkerPool::new Routing
- Detects `HandlerMode::BatchSync` and routes to `batch_worker_loop`
- SingleSync/SingleAsync/BatchAsync route to existing `worker_loop`

## Deviations from Plan

- **Retry re-enqueue:** In AllFailure branch, retry sleep is executed but message re-enqueue to queue_manager is noted as "handled by queue_manager's retry mechanism" — actual re-enqueue logic deferred to queue_manager integration in a future plan
- **PartialFailure:** Added explicit match arm that logs warning and acks all messages — per D-05 this is not implemented in v1.6

## Deferred Issues

| Item | Reason | Future Plan |
|------|--------|-------------|
| Batch retry re-enqueue | queue_manager retry API not wired for batch | v1.7+ |
| PartialFailure tracking | Per-message outcome not tracked | v1.7+ |

## Verification

- `cargo check --lib` should pass with 0 errors
- BatchAccumulator tests: `cargo test --lib batch_accumulator`
- Batch worker loop tests: `cargo test --lib batch_worker`

## Threat Flags

None — internal batching mechanism with no external attack surface.
