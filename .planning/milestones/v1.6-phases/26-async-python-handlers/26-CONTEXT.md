# Phase 26: Async Python Handlers - Context

**Gathered:** 2026-04-18
**Status:** Ready for planning

<domain>
## Phase Boundary

Async Python handlers execute as Rust Futures via a custom CFFI-based bridge (not pyo3-async-runtimes). GIL is held only during coroutine polling and released at every `.await`. Supports SingleAsync (single message) and BatchAsync (batch of messages) modes.

</domain>

<decisions>
## Implementation Decisions

### Coroutine Detection — D-01
- **At registration time** — `inspect.iscoroutinefunction()` called once when handler is registered
- **Cached on PythonHandler** — mode stored, no re-inspection per invocation
- **Stores `HandlerMode::SingleAsync`** for single-message async, `HandlerMode::BatchAsync` for batch async
- Rationale: avoids per-call overhead; detection is deterministic at registration

### GIL Release Strategy — D-02
- **Custom CFFI bridge** instead of pyo3-async-runtimes
- Uses Python C API directly (`PyGILState_Ensure` / `PyGILState_Release`)
- GIL acquired **only** during coroutine polling — held transiently
- GIL released **at every `.await`** — coroutine yields back to Rust/Tokio
- Custom `Future` implementation drives coroutine forward
- Rationale: explicit control, aligns with existing rdkafka-lite CFFI usage, avoids additional dependency

### Async Error Handling — D-03
- **Try/catch inside `Future::poll`** — Python exceptions caught during coroutine polling
- Surfaced as `ExecutionResult::Error` (SingleAsync) or `BatchExecutionResult::AllFailure` (BatchAsync)
- Keeps Rust error handling clean — all paths return typed results
- Rationale: separation of concerns; coroutine failures handled uniformly as errors

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Codebase References
- `src/python/handler.rs` — PythonHandler, HandlerMode enum, invoke_mode dispatch, invoke_batch (Phase 25)
- `src/python/execution_result.rs` — ExecutionResult, BatchExecutionResult (Phase 25)
- `src/worker_pool/mod.rs` — WorkerPool::new routing by mode, worker_loop, batch_worker_loop (Phase 25)
- `src/python/executor.rs` — Executor trait, DefaultExecutor
- `Cargo.toml` — pyo3-async-runtimes already listed (not used, Phase 26 uses custom CFFI)

### Prior Phase Context
- `.planning/phases/25-batch-accumulation-flush/25-CONTEXT.md` — BatchAccumulator, batch_worker_loop, batch result routing
- `.planning/phases/24-handlermode-execution-foundation/24-CONTEXT.md` — HandlerMode enum, BatchPolicy struct

</canonical_refs>

<codebase_context>
## Existing Code Insights

### Reusable Assets
- `HandlerMode` enum already defined with SingleAsync/BatchAsync variants (Phase 24)
- `invoke_mode` method with placeholder unimplemented! arms for SingleAsync/BatchAsync (Phase 24)
- `PythonHandler::invoke` uses `Python::with_gil` pattern for sync invocation (Phase 24)
- `BatchAccumulator` and `batch_worker_loop` handle per-partition accumulation (Phase 25)
- `BatchExecutionResult::AllFailure` ready for async batch errors (Phase 25)

### Integration Points
- `invoke_mode` dispatch already branches on HandlerMode — add SingleAsync/BatchAsync cases
- `WorkerPool::new` already routes by mode — BatchAsync routes to batch_worker_loop with async invoke
- Error propagation: async path returns ExecutionResult/BatchExecutionResult same as sync path

### Established Patterns
- GIL acquired in `Python::with_gil` scope only for actual Python execution
- `spawn_blocking` for sync batch (releases Tokio thread, holds GIL for duration)
- `into_future` equivalent needed for async (releases GIL at every await)

</codebase_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 26-async-python-handlers*
*Context gathered: 2026-04-18*
