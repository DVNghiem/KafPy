# Phase 24: HandlerMode & Execution Foundation - Context

**Gathered:** 2026-04-18
**Status:** Ready for planning

<domain>
## Phase Boundary

HandlerMode enum gates all handler dispatch in WorkerPool. BatchPolicy stored on PythonHandler (matching retry_policy pattern). RoutingDecision works unchanged — batch accumulation is handler-internal concern.

</domain>

<decisions>
## Implementation Decisions

### HandlerMode Storage — D-01
- **Follow big tech** — HandlerMode enum stored on PythonHandler struct (same pattern as retry_policy field)
- PythonHandler gets new `mode: HandlerMode` field and `batch_policy: Option<BatchPolicy>` field
- Registration API detects mode automatically or accepts explicit mode override

### BatchPolicy Storage — D-02
- **Follow big tech** — BatchPolicy stored on PythonHandler alongside mode (same as retry_policy)
- Optional per-handler: `None` = no batching (single-message default), `Some(BatchPolicy)` = batch mode
- BatchPolicy: `max_batch_size: usize`, `max_batch_wait_ms: u64`

### WorkerPool Dispatch — D-03
- **Follow big tech** — Single worker_loop with `match handler.mode` dispatch
- SingleSync: existing spawn_blocking path (no batching)
- BatchSync: spawn_blocking with `Vec<OwnedMessage>` (batching handled in worker_loop)
- SingleAsync: pyo3-async-runtimes into_future (Phase 26)
- BatchAsync: into_future with Vec (Phase 26)
- No separate specialized worker paths — mode gates dispatch within existing worker_loop structure

### Routing + Batch Integration — D-04
- **Follow big tech** — RoutingDecision::Route(handler_id) queues messages unchanged
- Handler's batch mode is internal concern — dispatcher doesn't know or care about batching
- BatchAccumulator lives in worker_loop, not in dispatcher
- Per-partition ordering preserved: batches respect partition boundaries (handled in worker_loop)

### Batch Result Model — D-05
- **Follow big tech** — BatchExecutionResult::AllSuccess triggers record_ack per message (same as single)
- BatchExecutionResult::AllFailure routes all to RetryCoordinator (same path as single-message failure)
- PartialFailure deferred to v1.7+ (explicit extension point)

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Codebase References
- `src/python/handler.rs` — PythonHandler struct (add mode + batch_policy fields here)
- `src/python/worker_pool.rs` — WorkerPool::worker_loop (add mode dispatch)
- `src/dispatcher/mod.rs` — ConsumerDispatcher (routing integration unchanged)
- `src/routing/decision.rs` — RoutingDecision::Route(handler_id)

### Prior Phase Context
- `.planning/phases/21-routing-core/21-01-SUMMARY.md` — RoutingContext zero-copy pattern
- `.planning/phases/22-python-integration/22-01-SUMMARY.md` — spawn_blocking GIL pattern

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `PythonHandler::retry_policy` pattern — mode and batch_policy follow identical field pattern
- `spawn_blocking` invoke path in PythonHandler::invoke — unchanged for SingleSync

### Established Patterns
- Optional per-handler config (retry_policy) — same pattern for BatchPolicy
- worker_loop in WorkerPool — dispatch by mode match is natural extension

### Integration Points
- PythonHandler new fields: mode, batch_policy
- WorkerPool::worker_loop: match on handler.mode for dispatch path
- RoutingDecision::Route: unchanged — handler_id routing independent of batch mode

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 24-handlermode-execution-foundation*
*Context gathered: 2026-04-18*