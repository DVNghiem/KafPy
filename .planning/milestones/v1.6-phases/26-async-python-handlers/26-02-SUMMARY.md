---
phase: "26"
plan: "02"
type: execute
wave: 2
subsystem: python
tags: [async, handler, batch, invoke_mode]
dependency_graph:
  requires:
    - src/python/async_bridge.rs
    - src/python/handler.rs
  provides:
    - invoke_async method
    - invoke_batch_async method
    - invoke_mode_batch dispatch
tech_stack:
  added:
    - PythonAsyncFuture integration into PythonHandler
    - async batch dispatch pattern
  patterns:
    - Mode-based dispatch to async/sync invoke paths
    - Coroutine created in with_gil, driven by PythonAsyncFuture
key_files:
  modified:
    - src/python/handler.rs
    - src/worker_pool/mod.rs
decisions:
  - "invoke_async builds coroutine inside Python::with_gil, wraps in PythonAsyncFuture"
  - "invoke_batch_async mirrors invoke_batch but uses PythonAsyncFuture for coroutine"
  - "invoke_mode_batch dispatches to correct batch method based on HandlerMode"
  - "batch_worker_loop calls invoke_mode_batch for both BatchSync and BatchAsync"
metrics:
  duration: "~8 minutes"
  completed: "2026-04-18"
  tasks_completed: 2
  files_changed: 2
  lines_added: ~213
  lines_deleted: ~69
---

# Phase 26 Plan 02: Async Invoke Paths — Summary

## One-liner

Wired SingleAsync and BatchAsync handler modes using PythonAsyncFuture for GIL-free async execution.

## What Was Built

### Task 1: invoke_async and invoke_batch_async (handler.rs)

**`invoke_async`** — async method for SingleAsync handlers:
- Clones callback and message fields
- Builds Python message dict inside `Python::with_gil`
- Calls `callback.call1(py, (py_msg,))` to create coroutine object
- Wraps coroutine in `PythonAsyncFuture::from(coro)` and awaits it
- GIL released during await, held only transiently during coroutine.send(None)

**`invoke_batch_async`** — async method for BatchAsync handlers:
- Same pattern as `invoke_batch` but uses PythonAsyncFuture
- Builds `Vec<Py<PyAny>>` of message dicts inside `Python::with_gil`
- Returns `BatchExecutionResult` instead of `ExecutionResult`

**`invoke_mode_batch`** — dispatch helper for batch_worker_loop:
- Routes to `invoke_batch` (BatchSync) or `invoke_batch_async` (BatchAsync)
- Called by batch_worker_loop to handle both batch modes

### Task 2: WorkerPool routing (worker_pool/mod.rs)

**`WorkerPool::new`** — updated `use_batch` condition:
```rust
let use_batch = matches!(
    mode,
    HandlerMode::BatchSync | HandlerMode::BatchAsync
);
```

**`batch_worker_loop`** — updated all invoke calls:
- All 6 occurrences of `handler.invoke_batch(...)` replaced with `handler.invoke_mode_batch(...)`
- This routes BatchSync to sync invoke and BatchAsync to async invoke

## Success Criteria Verification

| Criterion | Status |
|-----------|--------|
| SingleAsync branch calls invoke_async | DONE |
| BatchAsync branch calls invoke_batch_async | DONE |
| batch_worker_loop dispatches to correct invoke path | DONE |
| All 4 HandlerMode paths have implementation | DONE |
| GIL released during await in async paths | DONE (via PythonAsyncFuture) |
| cargo check --lib passes | DONE (0 errors) |

## Deviations from Plan

None — plan executed exactly as written.

## Files Modified

- `src/python/handler.rs` — added invoke_async, invoke_batch_async, invoke_mode_batch
- `src/worker_pool/mod.rs` — updated use_batch routing, batch_worker_loop calls invoke_mode_batch
