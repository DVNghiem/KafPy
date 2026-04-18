# Phase 25: Batch Accumulation & Flush - Context

**Gathered:** 2026-04-18
**Status:** Ready for planning

<domain>
## Phase Boundary

BatchAccumulator accumulates messages per handler until `max_batch_size` OR `max_batch_wait_ms` timeout. Respects per-partition ordering (messages from same partition accumulate together, batches formed per-partition then combined). Flush triggers handler invocation. Batch results (AllSuccess/AllFailure) flow through existing RetryCoordinator.

</domain>

<decisions>
## Implementation Decisions

### BatchAccumulator Structure — D-01
- **Follow big tech** — dedicated `BatchAccumulator` struct in `worker_pool/` (separate from worker_loop)
- Rationale: clean unit testing of accumulation logic, timeout, flush conditions in isolation; matches OffsetTracker/QueueManager precedent
- Accumulates messages per-partition (preserving ordering within partition); batches combined by partition then flushed

### Flush Trigger: Fixed-Window Timeout — D-02
- **Follow big tech** — fixed-window timeout (not sliding/recalculating)
- Timer starts when first message enters empty accumulator
- Deadline fixed at first arrival time + max_batch_wait_ms
- Flush fires at deadline regardless of late message arrivals
- If batch fills to max_batch_size before deadline, flush immediately
- Per-partition: each partition has its own accumulator and timer

### Backpressure During Accumulation — D-03
- **Follow big tech** — flush immediately on backpressure
- When backpressure fires during nonempty accumulator: flush current batch immediately, then signal backpressure upstream (stop pulling from queue)
- Rationale: accumulator never overflows; clean separation between accumulation and backpressure concerns
- Timer keeps running during backpressure wait (deadline still applies)

### Batch Result Extraction — D-04
- **Follow big tech** — inline iteration in worker_loop
- `worker_loop` matches `BatchExecutionResult` and iterates:
  - `AllSuccess(Vec<Offset>)` → for each offset, call `offset_coordinator.record_ack()` + `queue_manager.ack()` + `retry_coordinator.record_success()`
  - `AllFailure(FailureReason)` → for each message in batch, call `retry_coordinator.record_failure()` individually
- Rationale: explicit and easy to trace/log each ack/retry; avoids new trait method on Executor

### PartialFailure — D-05
- **Skip PartialFailure in Phase 25** — only `AllSuccess` and `AllFailure`
- `PartialFailure` is an explicit extension point for v1.7+
- Document but don't implement in this phase

### Batch Result Model — D-06 (carried from Phase 24)
- **From Phase 24** — `BatchExecutionResult::AllSuccess` triggers `record_ack` per message (same as single)
- **From Phase 24** — `BatchExecutionResult::AllFailure` routes all to RetryCoordinator (same path as single-message failure)

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Codebase References
- `src/worker_pool/mod.rs` — worker_loop (add BatchAccumulator integration here)
- `src/python/handler.rs` — PythonHandler (add batch invoke methods here)
- `src/python/execution_result.rs` — ExecutionResult (add BatchExecutionResult enum)
- `src/coordinator/offset_tracker.rs` — OffsetTracker (pattern for per-TP state)
- `src/coordinator/retry_coordinator.rs` — RetryCoordinator (record_failure/record_success per message)
- `src/dispatcher/queue_manager.rs` — QueueManager (backpressure API, ack API)

### Prior Phase Context
- `.planning/phases/24-handlermode-execution-foundation/24-CONTEXT.md` — HandlerMode, BatchPolicy, batch result model decisions

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `OffsetTracker` per-partition state pattern — BatchAccumulator per-partition state mirrors this
- `tokio::time::timeout` — already used; races select! branches in worker_loop
- `CancellationToken` — already wired for shutdown; used to abort timer on shutdown

### Established Patterns
- Worker loop `select!` structure — handles both message arrival and cancellation
- `spawn_blocking` invoke path — unchanged for BatchSync; used for batch handler invocation
- RetryCoordinator 3-tuple — already handles per-message retry/DLQ decisions

### Integration Points
- BatchAccumulator lives between `rx.recv()` and `handler.invoke_mode()`
- `BatchSync` mode invokes handler via `spawn_blocking` with `Vec<OwnedMessage>`
- `BatchExecutionResult` flows into existing retry/ack path after extraction

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 25-batch-accumulation-flush*
*Context gathered: 2026-04-18*
