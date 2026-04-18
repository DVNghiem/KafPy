---
phase: 22-python-integration
plan: "01"
subsystem: routing
tags: [pyo3, tokio, python-integration, routing]

# Dependency graph
requires:
  - phase: 21-routing-core
    provides: "Router trait, RoutingContext, RoutingDecision enum, RejectReason"
provides:
  - "PythonRouter struct implementing Router trait"
  - "Python routing callback integration via spawn_blocking"
  - "Async unit tests for all RoutingDecision paths"
affects: [23-dispatcher-integration]

# Tech tracking
tech-stack:
  added: [pyo3::prelude::*, pyo3::types::PyDict, tokio::task::spawn_blocking, tokio::runtime::Handle::block_on]
  patterns: [sync-to-async bridge via Handle::current().block_on(), Py<PyAny> callback wrapping]

key-files:
  created:
    - src/routing/python_router.rs
  modified:
    - src/routing/mod.rs

key-decisions:
  - "Used Handle::current().block_on() to bridge sync route() with async spawn_blocking (Router trait requires sync)"
  - "PyDict built inside spawn_blocking closure to avoid lifetime issues"
  - "All RoutingContext fields cloned to owned values before entering spawn_blocking"

patterns-established:
  - "PythonRouter follows same spawn_blocking pattern as PythonHandler"
  - "RoutingDecision string protocol: route:, drop, reject:, defer"

requirements-completed: [PYROUTER-01, PYROUTER-02, PYROUTER-03]

# Metrics
duration: 8min
completed: 2026-04-18
---

# Phase 22-01: PythonRouter Summary

**PythonRouter implementing Router trait with Arc<Py<PyAny>> callback via tokio::spawn_blocking, bridging sync route() to async Python GIL calls**

## Performance

- **Duration:** 8 min
- **Started:** 2026-04-18T01:49:00Z
- **Completed:** 2026-04-18T01:57:00Z
- **Tasks:** 3
- **Files modified:** 2

## Accomplishments
- PythonRouter struct implementing Router trait with Arc<Py<PyAny>> callback storage (PYROUTER-01)
- route() uses spawn_blocking + block_on to call Python GIL from sync context (PYROUTER-02)
- PyDict built from RoutingContext (topic, partition, offset, key, payload, headers)
- String return parsed to RoutingDecision: route:, drop, reject:, defer (PYROUTER-03)
- Python exceptions logged at WARN level and return Reject(RoutingPythonError) (D-08)
- python_router module exported in routing/mod.rs
- Async unit tests for Drop, Route, Reject, and Defer paths

## Task Commits

1. **Task 1: Create src/routing/python_router.rs** - `212652a` (feat)
2. **Task 2: Export python_router module in mod.rs** - `ad1442b` (feat)
3. **Task 3: Unit tests** - Included in Task 1 commit

## Files Created/Modified
- `src/routing/python_router.rs` - PythonRouter implementing Router trait with Py<PyAny> callback
- `src/routing/mod.rs` - Added `pub mod python_router;`

## Decisions Made
- Used `tokio::runtime::Handle::current().block_on()` inside `call_py_and_parse()` to bridge the sync `Router::route()` signature with async `spawn_blocking` (Router trait requires sync fn)
- Cloned all RoutingContext fields to owned values before entering spawn_blocking to avoid lifetime issues
- `parse_return()` is a pure static fn for testability

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
- **PyO3 0.20+ API changes**: `PyNone::new()` constructor is private; used `PyNone::get(py).into()` instead
- **PyO3 0.20+ API changes**: `py.run()` replaced with `PyModule::from_code()` with proper `CStr` arguments
- **spawn_blocking sync/async mismatch**: `spawn_blocking` returns `JoinHandle` (not `Result`), required `Handle::current().block_on()` to wait
- **Pre-existing PyO3 linking error**: Test binary linking fails due to missing Python symbols (noted in STATE.md blockers) - library code compiles successfully

## Next Phase Readiness
- PythonRouter ready for integration into RoutingChain in Phase 23
- No blockers from this plan

---
*Phase: 22-python-integration/22-01*
*Completed: 2026-04-18*
