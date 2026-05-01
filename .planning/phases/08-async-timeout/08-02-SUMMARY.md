---
phase: 08-async-timeout
plan: 02
subsystem: observability
tags: [kafka, timeout, prometheus, metrics, dlq]

# Dependency graph
requires:
  - phase: 08-01
    provides: TimeoutInfo struct, ExecutionResult::Timeout variant, DlqMetadata timeout fields, TimeoutMetrics::record_timeout()
provides:
  - ExecutionResult::Timeout enriched DLQ metadata with timeout_duration and last_processed_offset
  - kafpy.handler.timeout_total counter emission on handler timeout
affects:
  - Phase 09 (Handler Middleware) - timeout metrics wiring
  - Phase 08 (Async Timeout) - plan 03 streaming handler

# Tech tracking
tech-stack:
  added: []
  patterns:
    - Structured timeout metadata propagation via ExecutionResult::Timeout payload
    - Optional metadata fields on DlqMetadata (no breaking change to existing callers)

key-files:
  created: []
  modified:
    - src/worker_pool/mod.rs - handle_execution_failure accepts ExecutionResult, extracts timeout metadata for DLQ
    - src/worker_pool/worker.rs - Added TimeoutMetrics emission on ExecutionResult::Timeout

key-decisions:
  - "handle_execution_failure extracts FailureReason from ExecutionResult - avoids duplicate reason parameter"
  - "timeout_duration converted from ms to seconds at DLQ population (DlqMetadata stores seconds per D-02)"
  - "ExecutionResult::Timeout treated as Terminal(HandlerPanic) for retry/DLQ classification"

patterns-established:
  - "Timeout metadata flows from ExecutionResult::Timeout -> handle_execution_failure -> DlqMetadata"
  - "TimeoutMetrics emitted in worker loop after HANDLER_METRICS.record_error"

requirements-completed: [TMOUT-01, TMOUT-02, TMOUT-03]

# Metrics
duration: 3 min
completed: 2026-04-29
---

# Phase 08-async-timeout: Plan 02 Summary

**Timeout result wired to DLQ metadata and Prometheus counter via handle_execution_failure and worker loop**

## Performance

- **Duration:** 3 min
- **Started:** 2026-04-29T12:21:00Z
- **Completed:** 2026-04-29T12:24:00Z
- **Tasks:** 3
- **Files modified:** 2

## Accomplishments
- handle_execution_failure updated to accept ExecutionResult directly instead of FailureReason
- ExecutionResult::Timeout triggers DlqMetadata enrichment with timeout_duration (ms->sec) and last_processed_offset
- TimeoutMetrics::record_timeout() emitted in worker loop on ExecutionResult::Timeout
- Python API chain verified end-to-end: add_handler(timeout_ms=X) -> PythonHandler::handler_timeout -> invoke_mode_with_timeout

## Task Commits

Each task was committed atomically:

1. **Task 1+2: handle_execution_failure accepts ExecutionResult + TimeoutMetrics emission** - `5d0e942` (feat)
2. **Task 3: TMOUT-01 Python API chain verification** - `0fbd3ce` (test)

**Plan metadata:** `3d096e7` (docs: complete planning)

## Files Created/Modified
- `src/worker_pool/mod.rs` - handle_execution_failure signature changed from `reason: &FailureReason` to `result: &ExecutionResult`, extracts reason internally, populates timeout_duration and last_processed_offset on DLQ
- `src/worker_pool/worker.rs` - Added TimeoutMetrics import, emit record_timeout on ExecutionResult::Timeout after error metrics

## Decisions Made

- handle_execution_failure extracts FailureReason from ExecutionResult internally - simplifies call sites, avoids passing reason separately
- timeout_duration stored as seconds in DlqMetadata (converted from ms at the caller)
- ExecutionResult::Timeout is already classified as Terminal(HandlerPanic) in worker.rs before calling handle_execution_failure

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None

## Next Phase Readiness
- TMOUT-01, TMOUT-02, TMOUT-03 all complete
- Phase 08 (Async Timeout) fully wired — ready for Phase 09 (Handler Middleware)

---
*Phase: 08-async-timeout-plan-02*
*Completed: 2026-04-29*