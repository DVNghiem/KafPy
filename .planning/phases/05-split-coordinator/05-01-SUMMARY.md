---
phase: "05-split-coordinator"
plan: "01"
subsystem: "coordinator"
tags: [rust, refactor, coordinator, offset-tracking, shutdown, retry]

requires:
  - phase: "04-02"
    provides: "ConsumerRunner, OffsetTracker, ShutdownCoordinator fully wired"

provides:
  - "offset/ module: OffsetTracker, OffsetCoordinator, OffsetCommitter, CommitConfig, TopicPartition"
  - "shutdown/ module: ShutdownCoordinator, ShutdownPhase"
  - "retry/ module: RetryCoordinator (moved from coordinator/), RetryPolicy"
  - "coordinator/ as thin re-export layer (backward-compatible)"

affects:
  - "Phase 06+ (any phase that imports from coordinator/)"
  - "worker_pool module (uses coordinator types)"
  - "runtime/builder.rs (uses coordinator types)"

tech-stack:
  added: []
  patterns: ["module split by responsibility boundary", "thin re-export layer for backward compatibility"]

key-files:
  created:
    - "src/offset/mod.rs"
    - "src/shutdown/mod.rs"
  modified:
    - "src/offset/offset_tracker.rs"
    - "src/offset/offset_coordinator.rs"
    - "src/offset/commit_task.rs"
    - "src/shutdown/shutdown.rs"
    - "src/retry/mod.rs"
    - "src/retry/retry_coordinator.rs"
    - "src/coordinator/mod.rs"
    - "src/lib.rs"
    - "src/worker_pool/mod.rs"
    - "src/worker_pool/batch_loop.rs"
    - "src/worker_pool/pool.rs"
    - "src/worker_pool/worker.rs"
    - "src/consumer/runner.rs"
    - "src/runtime/builder.rs"

key-decisions:
  - "Simplified coordinator/mod.rs re-exports (removed nested pub mod submodules that caused path resolution issues)"
  - "Added offset/ and shutdown/ as pub(crate) modules in lib.rs alongside existing retry/"
  - "Preserved all backward-compatible import paths via coordinator/ re-exports"

patterns-established:
  - "Responsibility-split modules: offset/, shutdown/, retry/ with clear boundaries"
  - "coordinator/ as pure forwarding layer with no internal logic"

requirements-completed: [SPLIT-C-01, SPLIT-C-02, SPLIT-C-03, SPLIT-C-04, SPLIT-C-05]

duration: 12min
completed: 2026-04-20
---

# Phase 5: Split coordinator/ into offset/, shutdown/, retry/ modules

**Split coordinator/ into 3 responsibility-bound modules with backward-compatible re-exports**

## Performance

- **Duration:** 12 min
- **Tasks:** 6 completed, 1 checkpoint (human-verify approved)
- **Files modified:** 16 (14 source files + 2 planning)

## Accomplishments

- Created `offset/` module with `OffsetTracker`, `OffsetCoordinator`, `OffsetCommitter`
- Created `shutdown/` module with `ShutdownCoordinator`, `ShutdownPhase`
- Moved `RetryCoordinator` into existing `retry/` module
- Converted `coordinator/` into thin re-export layer preserving all backward-compatible paths
- All offset commit semantics preserved (highest-contiguous-offset algorithm, has_terminal gating)

## Task Commits

1. **Task 1-6 (all tasks):** `d257882` (refactor)
   - Split coordinator/ into offset/, shutdown/, retry/
   - All backward-compatible re-exports working

**Plan metadata commit:** `d257882` (refactor: complete coordinator split plan)

## Files Created/Modified

- `src/offset/mod.rs` - Offset module entry point
- `src/shutdown/mod.rs` - Shutdown module entry point
- `src/offset/offset_tracker.rs` - Per-TP offset tracking with BTreeSet buffering
- `src/offset/offset_coordinator.rs` - OffsetCoordinator trait
- `src/offset/commit_task.rs` - Background throttled offset committer
- `src/shutdown/shutdown.rs` - 4-phase shutdown lifecycle coordinator
- `src/retry/retry_coordinator.rs` - RetryCoordinator moved from coordinator/
- `src/retry/mod.rs` - Now exports RetryCoordinator alongside RetryPolicy
- `src/coordinator/mod.rs` - Thin re-export layer (no internal logic)
- `src/lib.rs` - Added `pub(crate) mod offset` and `pub(crate) mod shutdown`
- `src/worker_pool/mod.rs` - Updated imports to use coordinator:: re-exports
- `src/worker_pool/batch_loop.rs` - Updated imports
- `src/worker_pool/pool.rs` - Updated imports
- `src/worker_pool/worker.rs` - Updated imports
- `src/consumer/runner.rs` - Updated ShutdownCoordinator import
- `src/runtime/builder.rs` - Consolidated coordinator imports

## Decisions Made

- **Simplified coordinator/mod.rs re-exports:** Instead of nested `pub mod offset { ... }` submodules that caused `unresolved import crate::offset` errors (because `coordinator` is `pub(crate)`), used direct `pub use crate::offset::...` statements
- **Added offset/ and shutdown/ to lib.rs:** These were moved from `coordinator/` so needed to be declared at crate root alongside `retry/`
- **Preserved coordinator:: re-exports:** All existing import paths continue to work via the re-export layer

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

- **Nested submodule path issue:** `pub mod offset { pub use crate::offset::... }` inside `pub(crate) mod coordinator` caused `crate::offset` to be unresolved because the nested submodule context couldn't reach the crate root. Fixed by using direct `pub use` statements in `coordinator/mod.rs` instead of nested submodules.

## Threat Flags

None - pure internal refactor, no new trust boundaries introduced.

## Next Phase Readiness

- All coordinator/ types now cleanly split into offset/, shutdown/, retry/
- Backward compatibility preserved - existing imports continue to work
- Ready for Phase 6 (no new dependencies required)

---
*Phase: 05-split-coordinator*
*Completed: 2026-04-20*
