---
phase: 15-workerpool-integration
plan: 01
status: complete
started: 2026-04-17
completed: 2026-04-17
summary: "WorkerPool now wires OffsetCoordinator into its worker loop — record_ack on ExecutionResult::Ok, mark_failed on Error/Rejected, graceful_shutdown commits all partitions"
key_files_created:
  - src/worker_pool/mod.rs (offset coordinator wiring)
  - src/coordinator/offset_tracker.rs (set_runner, all_partitions, graceful_shutdown)
key_files_modified:
  - src/worker_pool/mod.rs
  - src/coordinator/offset_tracker.rs
verification:
  cargo_build: passed
  cargo_test: passed
  manual_verify: |
    Verified via grep that:
    - record_ack called after queue.ack() in ExecutionResult::Ok arm
    - mark_failed called after error/rejected tracing
    - graceful_shutdown commits all partitions on worker pool shutdown
---
## Phase 15 Summary: WorkerPool Integration

**Objective:** Wire OffsetCoordinator into WorkerPool worker loop

**Tasks Completed:**
1. Added `OffsetCoordinator::record_ack()` call after `queue_manager.ack()` in `ExecutionResult::Ok` arm
2. Added `OffsetCoordinator::mark_failed()` calls in `Error` and `Rejected` arms after error tracing
3. Added `set_runner()`, `all_partitions()` methods to OffsetTracker for shutdown coordination
4. Implemented `graceful_shutdown()` to commit highest contiguous offset per topic-partition
5. Updated `WorkerPool::shutdown()` to call `offset_coordinator.graceful_shutdown()` before join_set shutdown

**Decisions Captured:**
- D-03: WorkerPool::shutdown() calls graceful_shutdown() on the coordinator
- D-04: record_ack after queue.ack(), fire-and-forget
- D-05: mark_failed after error trace, fire-and-forget

**Files Modified:**
- `src/worker_pool/mod.rs` — Added offset coordinator record_ack/mark_failed calls
- `src/coordinator/offset_tracker.rs` — Added set_runner(), all_partitions(), graceful_shutdown()

**Verification:**
- Build: passed
- Tests: passed