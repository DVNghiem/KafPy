---
phase: "01-extract-duplication"
plan: "01"
subsystem: worker-pool
tags: [rust, extraction, deduplication, pyo3]

# Dependency graph
requires: []
provides:
  - ExecutionAction enum and handle_execution_failure() helper (reduces Error/Rejected duplication)
  - message_to_pydict() helper (consolidates Python callback dict construction)
  - flush_partition_batch() helper (consolidates 6 batch flush patterns)
  - NoopSink consolidated into observability/metrics.rs
affects:
  - worker_pool/mod.rs (main refactoring target)
  - python/handler.rs (message_to_pydict extraction)

# Tech tracking
tech-stack:
  added: []
  patterns:
    - Helper extraction for deduplication (DUP-01 through DUP-04)
    - Async helper functions for shared control flow
    - PyO3 borrow lifetime management (ctx.clone() before spawn_blocking closures)

key-files:
  created: []
  modified:
    - src/worker_pool/mod.rs - 3 helper extractions, reduced ~150 lines of duplication
    - src/python/handler.rs - message_to_pydict extraction
    - src/observability/metrics.rs - NoopSink module added
    - src/observability/mod.rs - NoopSink re-export

key-decisions:
  - "ExecutionAction enum (Ack/Retry/Dlq) replaces inline retry/DLQ branching"
  - "message_to_pydict returns Py<PyAny> (not &PyDict) to support both single and batch paths"
  - "msg parameter is borrowed (&OwnedMessage) not consumed, allowing Retry re-enqueue"

requirements-completed: [DUP-01, DUP-02, DUP-03, DUP-04]

# Metrics
duration: 21min
completed: 2026-04-20
---

# Phase 01-extract-duplication: Extract 4 Helper Functions Summary

**4 helper functions extracted from worker_pool/mod.rs and python/handler.rs, consolidating ~150 lines of duplicated retry/DLQ routing, PyDict construction, and batch flush patterns**

## Performance

- **Duration:** 21 min
- **Started:** 2026-04-20T12:45:49Z
- **Completed:** 2026-04-20T13:07:05Z
- **Tasks:** 4
- **Files modified:** 4

## Accomplishments

- Extracted ExecutionAction enum + handle_execution_failure() to consolidate Error and Rejected retry/DLQ routing branches in worker_loop (DUP-01)
- Extracted message_to_pydict() to consolidate PyDict-from-OwnedMessage construction across invoke, invoke_batch, invoke_async, and invoke_batch_async (DUP-02)
- Extracted flush_partition_batch() to consolidate 6 identical batch flush patterns in batch_worker_loop (DUP-03)
- Moved NoopSink from worker_pool/mod.rs to observability/metrics.rs as a public module with re-export (DUP-04)

## Task Commits

Each task was committed atomically:

1. **Task 1: Extract handle_execution_failure() helper** - `f9d2f6f` (feat)
2. **Task 2: Extract message_to_pydict() helper** - `c0cf3c3` (feat)
3. **Task 3: Extract flush_partition_batch() helper** - `2ca3b8b` (feat)
4. **Task 4: Consolidate NoopSink into observability module** - `9c32260` (feat)

**Plan metadata:** `56cc2e9` (docs: create phase plan for duplication extraction)

## Files Created/Modified

- `src/worker_pool/mod.rs` - Added ExecutionAction enum, handle_execution_failure(), flush_partition_batch(); refactored Error/Rejected branches and 6 batch flush sites
- `src/python/handler.rs` - Added message_to_pydict() helper; refactored invoke, invoke_batch, invoke_async, invoke_batch_async to use it
- `src/observability/metrics.rs` - Added pub mod noop_sink with NoopSink struct
- `src/observability/mod.rs` - Added re-export: pub use metrics::noop_sink::NoopSink

## Decisions Made

- ExecutionAction::Retry carries delay so caller performs tokio::time::sleep (not helper) — keeps retry scheduling in the worker loop where active_message state is managed
- message_to_pydict uses Option<&HashMap> for trace_context (None for async paths, Some for sync paths) to preserve the existing no-trace-context behavior in invoke_async/invoke_batch_async
- msg borrowed (not owned) in handle_execution_failure so Retry case can still re-enqueue; caller retains ownership
- Added ctx.clone() in spawn_blocking closures to avoid lifetime issues with borrowed ExecutionContext references

## Deviations from Plan

**None - plan executed exactly as written**

## Issues Encountered

- **Lifetime issue with ctx in spawn_blocking**: When refactoring invoke() and invoke_batch(), ctx was borrowed by the move closure but used after await. Fixed by cloning ctx before the closure.
- **message_to_pydict return type**: Initially returned &PyDict but callback.call1 requires owned Py<Any>. Fixed by returning Py<PyAny> and using .into() on the PyDict.
- **Duplicate pyo3 imports**: After adding message_to_pydict at the top of handler.rs with its own pyo3 imports, the existing imports at the bottom of the file caused a warning. Removed the duplicate imports at the bottom.

## Verification

Phase-level checks from PLAN.md:
- `cargo clippy --all-targets` - PASSED (only pre-existing warnings)
- `grep "handle_execution_failure" src/worker_pool/mod.rs` - 1 definition + 2 call sites ✓
- `grep "message_to_pydict" src/python/handler.rs` - 1 definition + 4 call sites ✓
- `grep "flush_partition_batch" src/worker_pool/mod.rs` - 1 definition + 6 call sites ✓
- `grep "struct NoopSink" src/worker_pool/mod.rs` - returns empty (moved) ✓

## Next Phase Readiness

- All 4 helper extractions complete and verified with clippy
- NoopSink consolidated for clean observability module
- Ready for Phase 02 planning

---
*Phase: 01-extract-duplication*
*Completed: 2026-04-20*
