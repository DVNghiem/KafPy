# Phase 27: Shutdown Drain & Polish - Context

**Gathered:** 2026-04-18
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 27 is the final polish phase for v1.6. It verifies graceful shutdown drains accumulated batches, GIL is never held across Rust-side orchestration, and all 4 HandlerMode paths (SingleSync, SingleAsync, BatchSync, BatchAsync) work end-to-end.

</domain>

<decisions>
## Implementation Decisions

### Shutdown Drain — D-01
- **batch_worker_loop already drains** — when CancellationToken fires, the shutdown branch (Branch 3) flushes all partitions, invokes handler for each batch, then exits
- No new shutdown code needed — Phase 25 already implemented the drain
- **Verification task only**: confirm drain is triggered correctly in integration

### GIL Invariant — D-02
- **GIL held transiently only during Python execution** — never across Rust-side orchestration
- SingleSync/BatchSync: `spawn_blocking` holds GIL only inside the blocking task (Python::with_gil scope)
- SingleAsync/BatchAsync: `PythonAsyncFuture::poll` acquires GIL, calls `coro.send(None)`, releases GIL before returning Pending
- **Verification task only**: code review to confirm GIL patterns are correct

### All 4 HandlerMode Paths — D-03
- **SingleSync**: `invoke` via `spawn_blocking` (existing, Phase 24)
- **BatchSync**: `invoke_batch` via `spawn_blocking` with Vec<OwnedMessage> (Phase 25)
- **SingleAsync**: `invoke_async` via `PythonAsyncFuture` (Phase 26)
- **BatchAsync**: `invoke_batch_async` via `PythonAsyncFuture` with Vec<OwnedMessage> (Phase 26)
- **Verification task only**: confirm all 4 paths route correctly from WorkerPool::new

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Codebase References
- `src/worker_pool/mod.rs` — batch_worker_loop shutdown branch (lines 655-688), worker_loop shutdown branch (lines 416-418)
- `src/python/handler.rs` — invoke_mode dispatch for all 4 modes
- `src/python/async_bridge.rs` — PythonAsyncFuture GIL transient pattern (Phase 26)
- `src/python/executor.rs` — Executor trait
- `.planning/phases/25-batch-accumulation-flush/25-CONTEXT.md` — BatchAccumulator flush_all behavior
- `.planning/phases/26-async-python-handlers/26-CONTEXT.md` — PythonAsyncFuture GIL strategy

### Prior Phase Context
- `.planning/phases/24-handlermode-execution-foundation/24-CONTEXT.md` — HandlerMode enum, BatchPolicy
- `.planning/phases/25-batch-accumulation-flush/25-CONTEXT.md` — BatchAccumulator, batch_worker_loop

</canonical_refs>

<codebase_context>
## Existing Code Insights

### Reusable Assets
- `batch_worker_loop` shutdown branch already flushes all partitions (Phase 25)
- `worker_loop` idle shutdown just breaks (no accumulator to drain)
- `PythonAsyncFuture` GIL pattern from Phase 26: transient GIL per poll, released before returning Pending

### Integration Points
- WorkerPool::new already routes BatchSync/BatchAsync to batch_worker_loop, others to worker_loop
- All 4 modes should route correctly once Phase 26 completes

### Established Patterns
- CancellationToken wired to shutdown_token in WorkerPool::new
- `spawn_blocking` pattern for GIL management in sync path

</codebase_context>

<deferred>
## Deferred Ideas

None — Phase 27 is verification and polish only.

</deferred>

---

*Phase: 27-shutdown-drain-polish*
*Context gathered: 2026-04-18*
