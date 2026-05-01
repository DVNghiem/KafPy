---
phase: 08-async-timeout
plan: 01
subsystem: observability
tags: [kafka, timeout, prometheus, metrics, dlq]

# Dependency graph
requires: []
provides:
  - TimeoutInfo struct for structured timeout propagation
  - ExecutionResult::Timeout variant for distinguishing timeouts from generic errors
  - DlqMetadata with timeout_duration and last_processed_offset fields
  - TimeoutMetrics::record_timeout() with kafpy.handler.timeout_total counter
affects:
  - Phase 09 (Handler Middleware) - timeout metrics wiring
  - Phase 08 (Async Timeout) - plan 02 Python API wiring

# Tech tracking
tech-stack:
  added: []
  patterns:
    - Structured timeout propagation via TimeoutInfo payload
    - Optional metadata fields on DlqMetadata (no breaking change to existing callers)

key-files:
  created: []
  modified:
    - src/python/execution_result.rs - Added TimeoutInfo struct, ExecutionResult::Timeout variant, is_timeout()
    - src/python/handler.rs - invoke_mode_with_timeout returns ExecutionResult::Timeout on timeout
    - src/python/executor.rs - DefaultExecutor handles ExecutionResult::Timeout
    - src/worker_pool/worker.rs - Timeout branch added to worker match statement
    - src/worker_pool/mod.rs - handle_execution_failure passes None for new DlqMetadata fields
    - src/dlq/metadata.rs - Added timeout_duration and last_processed_offset to DlqMetadata
    - src/offset/offset_tracker.rs - Updated DlqMetadata::new calls with None for new fields
    - src/worker_pool/batch_loop.rs - Updated DlqMetadata::new calls with None for new fields
    - src/observability/metrics.rs - Added TimeoutMetrics with record_timeout()

key-decisions:
  - "ExecutionResult::Timeout replaces the previous pattern of returning ExecutionResult::Error with HandlerTimeout exception string"
  - "DlqMetadata new fields are Option types so existing callers pass None (no breaking change)"
  - "timeout_duration stored as u64 seconds per D-02 specification"

patterns-established:
  - "TimeoutInfo payload carries structured timeout data (timeout_ms, last_processed_offset) through ExecutionResult"
  - "ExecutionResult::Timeout treated as Terminal(HandlerPanic) for DLQ routing purposes"

requirements-completed: [TMOUT-02, TMOUT-03]

# Metrics
duration: 15min
completed: 2026-04-29
---

# Phase 08-async-timeout: Plan 01 Summary

**TimeoutInfo struct added to ExecutionResult, DlqMetadata enriched with timeout fields, and TimeoutMetrics counter registered**

## Performance

- **Duration:** 15 min
- **Started:** 2026-04-29T12:06:39Z
- **Completed:** 2026-04-29T12:21:00Z
- **Tasks:** 3
- **Files modified:** 10

## Accomplishments
- Added TimeoutInfo struct with timeout_ms and last_processed_offset fields to src/python/execution_result.rs
- Added ExecutionResult::Timeout variant for structured timeout propagation (replaces HandlerTimeout string in Error)
- Added is_timeout() method to ExecutionResult
- Added timeout_duration and last_processed_offset fields to DlqMetadata
- Updated all DlqMetadata::new() call sites to pass None for new optional fields
- Registered kafpy.handler.timeout_total counter in SharedPrometheusSink
- Implemented TimeoutMetrics::record_timeout(topic, handler_name)

## Task Commits

Each task was committed atomically:

1. **Task 1: Add TimeoutInfo to ExecutionResult** - `68ba1ce` (feat)
2. **Task 2: Add timeout_duration and last_processed_offset to DlqMetadata** - `8801636` (feat)
3. **Task 3: Add TimeoutMetrics and wire counter registration** - `f6fda49` (feat)

**Plan metadata:** `3d096e7` (docs: complete planning)

## Files Created/Modified
- `src/python/execution_result.rs` - TimeoutInfo struct, ExecutionResult::Timeout variant, is_timeout(), error_type_label() updated
- `src/python/handler.rs` - invoke_mode_with_timeout returns ExecutionResult::Timeout on timeout
- `src/python/executor.rs` - DefaultExecutor handles ExecutionResult::Timeout in match
- `src/worker_pool/worker.rs` - ExecutionResult::Timeout branch added to worker match with proper error handling
- `src/worker_pool/mod.rs` - DlqMetadata::new calls updated with None for new fields
- `src/dlq/metadata.rs` - timeout_duration and last_processed_offset fields added
- `src/offset/offset_tracker.rs` - DlqMetadata::new calls updated
- `src/worker_pool/batch_loop.rs` - DlqMetadata::new calls updated
- `src/observability/metrics.rs` - TimeoutMetrics struct with record_timeout()

## Decisions Made

- ExecutionResult::Timeout used instead of Error with HandlerTimeout string - cleaner distinction between actual errors and timeouts
- DlqMetadata new fields are Option types - existing callers pass None, no breaking change to callers
- timeout_duration stored as u64 seconds per D-02 specification (convert from ms at caller)

## Deviations from Plan

None - plan executed exactly as written.

## Auto-fixed Issues

**1. [Rule 1 - Bug] Added missing FailureReason import in worker.rs**
- **Found during:** Task 1 (ExecutionResult::Timeout implementation)
- **Issue:** FailureReason type not imported in worker.rs, compilation failed
- **Fix:** Added `use crate::failure::FailureReason;` import
- **Files modified:** src/worker_pool/worker.rs
- **Verification:** cargo check passes
- **Committed in:** 68ba1ce (Task 1 commit)

**2. [Rule 1 - Bug] Fixed missing & in reason passed to handle_execution_failure and mark_failed**
- **Found during:** Task 1 (ExecutionResult::Timeout implementation)
- **Issue:** handle_execution_failure expects &FailureReason, mark_failed expects &FailureReason
- **Fix:** Changed `reason` to `&reason` in both calls
- **Files modified:** src/worker_pool/worker.rs
- **Verification:** cargo check passes
- **Committed in:** 68ba1ce (Task 1 commit)

**3. [Rule 1 - Bug] Added missing ExecutionResult::Timeout match arms in executor.rs and batch handlers**
- **Found during:** Task 1 (ExecutionResult::Timeout implementation)
- **Issue:** Non-exhaustive patterns - ExecutionResult::Timeout not covered in DefaultExecutor and invoke_batch_async
- **Fix:** Added Timeout match arms treating timeout as failure with Terminal kind
- **Files modified:** src/python/executor.rs, src/python/handler.rs
- **Verification:** cargo check passes
- **Committed in:** 68ba1ce (Task 1 commit)

---

**Total deviations:** 3 auto-fixed (all Rule 1 bugs - compilation errors from new variant)
**Impact on plan:** All auto-fixes were necessary for compilation. No scope creep.

## Issues Encountered
- Rust match exhaustiveness checking caught multiple places that needed ExecutionResult::Timeout handling
- Borrow checker required explicit & on reason parameter in worker.rs Timeout branch

## Next Phase Readiness
- TimeoutInfo struct and ExecutionResult::Timeout variant ready for plan 02 Python API wiring
- DlqMetadata fields available for plan 02 to populate when routing timeouts to DLQ
- TimeoutMetrics::record_timeout() available for plan 02 to wire into worker loop

---
*Phase: 08-async-timeout-plan-01*
*Completed: 2026-04-29*