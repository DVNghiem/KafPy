---
phase: 09-handler-middleware
plan: "03"
subsystem: middleware
tags: [middleware, handler, cross-cutting, python]

# Dependency graph
requires:
  - phase: "09-01"
    provides: "HandlerMiddleware trait and MiddlewareChain struct"
  - phase: "09-02"
    provides: "Logging and Metrics built-in middleware"
provides:
  - Python API @handler(middleware=[Logging(), Metrics()]) per handler
  - PythonMiddleware wrapper bridging Python objects to HandlerMiddleware
  - Middleware chain wired into PythonHandler invoke path
affects: [streaming-handler]

# Tech tracking
tech-stack:
  added: []
  patterns: [decorator-pattern, type-name-detection-for-builtin-middleware]

key-files:
  created:
    - src/middleware/python.rs
  modified:
    - src/middleware/mod.rs
    - src/pyconsumer.rs
    - src/python/handler.rs
    - src/runtime/builder.rs
    - src/worker_pool/worker.rs
    - src/worker_pool/pool.rs
    - kafpy/__init__.py
    - kafpy/runtime.py

key-decisions:
  - "Arc<Py<PyAny>> for middleware storage (enables Clone derivability on HandlerMetadata)"
  - "Middleware chain built at invocation time (not registration) so metrics sink is available"
  - "Type-name detection for built-in Rust middleware (Logging/Metrics) — Python instances just markers"
  - "PythonMiddleware wraps Python objects and calls their before/after/on_error via GIL"

patterns-established:
  - "Python middleware objects are type-name detected at invocation time to dispatch to Rust built-ins"

requirements-completed: [MIDW-04]

# Metrics
duration: "~1 min"
completed: 2026-04-29T13:19:08Z
---

# Phase 09 Plan 03: Python API and Middleware Chain Wiring Summary

**Python API @handler(middleware=[Logging(), Metrics()]) per handler with MiddlewareChain wired into PythonHandler**

## Performance

- **Duration:** ~1 min
- **Started:** 2026-04-29T12:59:00Z
- **Completed:** 2026-04-29T13:19:08Z
- **Tasks:** 4
- **Files modified:** 10

## Accomplishments

- PythonMiddleware PyO3 wrapper bridges Python objects to HandlerMiddleware trait
- build_middleware_chain() detects type name (Logging/Metrics/custom) and creates appropriate Rust middleware
- HandlerMetadata accepts middleware as Option<Vec<Py<PyAny>>> wrapped in Arc for Clone derivability
- PythonHandler.invoke_mode_with_timeout executes middleware chain around handler invocation
- KafPy.handler() and register_handler() accept middleware parameter, passed through to Rust
- BaseMiddleware, Logging, Metrics Python classes exported in kafpy/__init__.py

## Task Commits

Each task was committed atomically:

1. **Task 1: Add PythonMiddleware PyO3 adapter** - `c79266a` (feat)
2. **Task 2: Extend add_handler with middleware parameter** - `1455cb0` (feat)
3. **Task 3: Wire middleware chain in PythonHandler** - `4e326bc` (feat)
4. **Task 4: Export middleware classes in kafpy/__init__.py** - `9a08d45` (feat)

## Files Created/Modified

- `src/middleware/python.rs` - PythonMiddleware wrapper and build_middleware_chain()
- `src/middleware/mod.rs` - Added python module and exports
- `src/pyconsumer.rs` - HandlerMetadata.middleware and add_handler middleware parameter
- `src/python/handler.rs` - PythonHandler.middleware field and chain execution in invoke_mode_with_timeout
- `src/runtime/builder.rs` - Updated PythonHandler::new call with middleware
- `src/worker_pool/worker.rs` - Updated PythonHandler::new call with middleware
- `src/worker_pool/pool.rs` - Updated PythonHandler::new call with middleware
- `kafpy/__init__.py` - BaseMiddleware, Logging, Metrics Python classes
- `kafpy/runtime.py` - handler() and register_handler() accept middleware parameter

## Decisions Made

- Arc<Py<PyAny>> for middleware storage (enables Clone derivability on HandlerMetadata)
- Middleware chain built at invocation time (not registration) so metrics sink is available
- Type-name detection for built-in Rust middleware (Logging/Metrics) — Python instances just markers
- PythonMiddleware wraps Python objects and calls their before/after/on_error via GIL

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None

## Next Phase Readiness

Phase 09 complete — ready for Phase 10 (Streaming Handler).
All MIDW requirements (MIDW-01 through MIDW-04) are complete.

---
*Phase: 09-handler-middleware*
*Completed: 2026-04-29*