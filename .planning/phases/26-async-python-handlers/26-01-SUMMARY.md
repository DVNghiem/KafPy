---
phase: "26"
plan: "01"
type: execute
wave: 1
subsystem: python
tags: [async, coroutine, gil, future, pyo3]
dependency_graph:
  requires:
    - src/python/execution_result.rs
    - src/failure/reason.rs
  provides:
    - src/python/async_bridge.rs
tech_stack:
  added:
    - pyo3 async coroutine polling
    - std::future::Future implementation
    - GIL acquire/release per-poll pattern
  patterns:
    - Custom Future adapter bridging Python coroutines to Tokio
key_files:
  created:
    - src/python/async_bridge.rs
  modified:
    - src/python/mod.rs
decisions:
  - "PyStopIteration/PyStopAsyncIteration caught explicitly as normal completion (not errors)"
  - "Python::attach used instead of deprecated Python::with_gil"
  - "Coroutine closed on Drop to prevent resource leaks"
metrics:
  duration: "~10 minutes"
  completed: "2026-04-18"
  tasks_completed: 2
  files_changed: 2
  lines_added: ~128
---

# Phase 26 Plan 01: Async Python Handler Bridge — Summary

## One-liner

Custom CFFI Future bridge converting Python coroutines to Tokio-compatible Futures with transient GIL acquisition.

## What Was Built

**`src/python/async_bridge.rs`** — `PythonAsyncFuture` struct with `Future` implementation:

| Behavior | Return |
|----------|--------|
| `coro.send(None)` yields value | `Poll::Pending` |
| `StopIteration` / `StopAsyncIteration` | `Poll::Ready(ExecutionResult::Ok)` |
| Any other `PyErr` | `Poll::Ready(ExecutionResult::Error { reason: Terminal(HandlerPanic), ... })` |

**Key properties:**
- GIL acquired only during `send(None)` call and immediately released
- Coroutine closed on `Drop` to prevent leaks
- `ExecutionResult::Ok` on normal coroutine return (StopIteration is NOT an error)
- Waker registered on each `Pending` return so Tokio re-polls

**`src/python/mod.rs`** — Added `pub mod async_bridge;` export.

## Verification

- `cargo check --lib` — passes (0 errors)
- `cargo clippy --lib` — passes, no warnings in `async_bridge.rs`

## Deviations from Plan

None — plan executed exactly as written.

## Threat Model Compliance

| Threat | Mitigation | Status |
|--------|------------|--------|
| T-26-01 (DoS via coroutine timeout) | Deferred to Phase 27 | Handled later |
| T-26-02 (Exception leak) | All PyErr caught and converted to ExecutionResult::Error | Compliant |
| T-26-03 (Resource leak) | `coro.close()` called on Drop | Compliant |

## Known Stubs

None.

## Commits

- `75f9dd8` feat(26-01): add PythonAsyncFuture async coroutine bridge
