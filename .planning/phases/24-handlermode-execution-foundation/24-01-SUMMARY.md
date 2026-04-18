---
phase: "24"
plan: "01"
subsystem: execution-modes
tags: [handler-mode, batch-policy, worker-pool, dispatch]
dependency_graph:
  requires: []
  provides: [HandlerMode enum, BatchPolicy struct, PythonHandler mode + batch_policy fields, invoke_mode dispatch]
  affects: [Phase 25 batch accumulation, Phase 26 async handlers]
tech_stack:
  added: [HandlerMode enum, BatchPolicy struct, invoke_mode dispatch]
  patterns: [retry_policy field pattern for mode + batch_policy storage]
key_files:
  created: []
  modified:
    - src/python/handler.rs
    - src/worker_pool/mod.rs
    - src/pyconsumer.rs
decisions:
  - "HandlerMode stored on PythonHandler (same pattern as retry_policy)"
  - "BatchPolicy stored as Option<BatchPolicy> (None = no batching)"
  - "Single worker_loop with mode dispatch match — no separate specialized paths"
  - "invoke_mode match dispatches to invoke() for SingleSync, unimplemented!() for future modes"
  - "RoutingDecision::Route unchanged — batch is handler-internal concern"
metrics:
  duration: "5min"
  completed: "2026-04-18"
  tasks_completed: 3
  tasks_total: 3
  files_created: 0
  files_modified: 3
---

# Phase 24 Plan 01: HandlerMode & Execution Foundation Summary

## One-liner

HandlerMode enum (SingleSync/SingleAsync/BatchSync/BatchAsync) and BatchPolicy struct added to PythonHandler, mode dispatch wired into WorkerPool::worker_loop, all call sites updated.

## Tasks Completed

| Task | Name | Commit | Files |
|------|------|--------|-------|
| 1 | HandlerMode enum, BatchPolicy struct, PythonHandler fields | 81ef596 | handler.rs |
| 2 | invoke_mode dispatch in worker_loop | 87b675e | worker_pool/mod.rs |
| 3 | Update call sites (worker_pool test, pyconsumer.rs) | 87b675e | worker_pool/mod.rs, pyconsumer.rs |

## What Was Built

**HandlerMode enum** — `SingleSync`, `SingleAsync`, `BatchSync`, `BatchAsync` with Default (SingleSync). Documented future phases in enum variants.

**BatchPolicy struct** — `max_batch_size: usize`, `max_batch_wait_ms: u64` with Default (1, 0 — effectively no batching).

**PythonHandler extended** — `mode: HandlerMode` and `batch_policy: Option<BatchPolicy>` fields, updated constructor, added `mode()` and `batch_policy()` accessors.

**invoke_mode dispatch** — `PythonHandler::invoke_mode()` matches `self.mode()` to dispatch path. SingleSync → `invoke()` (existing spawn_blocking). Other modes → `unimplemented!()` for Phase 25/26.

**worker_loop updated** — calls `handler.invoke_mode()` instead of `handler.invoke()` directly.

**Call sites updated** — `dummy_handler()` in worker_pool tests, `pyconsumer.rs` FFI binding.

## Verification

- `cargo check --lib` passes (0 errors, 23 pre-existing warnings)
- HandlerMode enum: `pub enum HandlerMode { SingleSync, SingleAsync, BatchSync, BatchAsync }` ✓
- BatchPolicy struct: `pub struct BatchPolicy { max_batch_size, max_batch_wait_ms }` ✓
- PythonHandler has mode and batch_policy fields ✓
- invoke_mode match in worker_loop ✓

## Next Phase Readiness

Phase 25 (Batch Accumulation & Flush) builds on HandlerMode/BatchPolicy. invoke_mode stubs ready for batch implementation.

---

*Phase: 24-handlermode-execution-foundation/24-01*
*Completed: 2026-04-18*