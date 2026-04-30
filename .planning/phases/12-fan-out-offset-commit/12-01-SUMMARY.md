---
phase: 12
plan: 01
subsystem: worker_pool
tags: [fan-out, offset-commit, dlq, concurrency]
dependency_graph:
  requires:
    - phase-11-fan-out-core
  provides:
    - FANOUT-04
    - FANOUT-05
  affects:
    - src/worker_pool/fan_out.rs
    - src/worker_pool/worker.rs
    - src/python/context.rs
    - src/python/handler.rs
    - src/dlq/metadata.rs
tech_stack:
  added:
    - tokio::sync::Notify (branch completion signaling)
    - AtomicU8 counters (completed branch tracking)
    - Mutex<Vec<(u64, BranchResult)>> (result collection)
  patterns:
    - wait_all() barrier synchronization
    - per-sink timeout override
    - branch metadata DLQ routing
key_files:
  created: []
  modified:
    - src/worker_pool/fan_out.rs
    - src/python/context.rs
    - src/dlq/metadata.rs
    - src/python/handler.rs
    - src/worker_pool/worker.rs
    - src/worker_pool/mod.rs
    - src/worker_pool/batch_loop.rs
    - src/offset/offset_tracker.rs
decisions:
  - D-01: Offset commits when all fan-out branches finish, failures logged and routed to DLQ (not blocking)
  - D-02: Per-sink timeout via invoke_mode_with_timeout_override()
  - D-03: BranchResult enum covers 3-category classification (Ok, Error, Timeout)
  - D-04: FanOutTracker::wait_all() blocks until all registered branches reach terminal state
metrics:
  duration: "~5 minutes"
  completed: "2026-04-29"
  tasks_completed: 6
  files_modified: 8
  commits: 4
---

# Phase 12 Plan 01: Fan-Out Offset Commit Summary

## One-Liner

Fan-out offset commit gating via `wait_all()` — all branches complete before worker idles; per-sink errors routed to DLQ with `branch_id`/`fan_out_id` metadata.

## Commits

| # | Hash | Message |
|---|------|---------|
| 1 | `9f3addc` | feat(phase-12): add wait_all() to FanOutTracker and timeout to SinkConfig |
| 2 | `d28c716` | feat(phase-12): add branch_id/fan_out_id to ExecutionContext and DlqMetadata |
| 3 | `a719fb2` | feat(phase-12): add invoke_mode_with_timeout_override to PythonHandler |
| 4 | `90688c3` | feat(phase-12): wire wait_all() into worker_loop for offset commit gating |

## Completed Tasks

| Task | Name | Commit | Files |
|------|------|--------|-------|
| 1 | Add wait_all() to FanOutTracker | `9f3addc` | src/worker_pool/fan_out.rs |
| 2 | Add timeout field to SinkConfig | `9f3addc` | src/worker_pool/fan_out.rs |
| 3 | Add branch_id/fan_out_id to ExecutionContext | `d28c716` | src/python/context.rs |
| 4 | Add branch_id/fan_out_id to DlqMetadata | `d28c716` | src/dlq/metadata.rs |
| 5 | Wire wait_all() into worker_loop | `90688c3` | src/worker_pool/worker.rs |
| 6 | Add invoke_mode_with_timeout_override | `a719fb2` | src/python/handler.rs |

## Implementation Details

### FanOutTracker Changes (Task 1)
- Added `BranchResults` struct with `results: Vec<(u64, BranchResult)>`
- Added `wait_all()` async method using `tokio::sync::Notify` for efficient waiting
- Added `register_branch()` returning `u64` branch_id
- Added `record_branch_result()` storing results and notifying on completion
- Added `completed: AtomicU8` counter and `results: Mutex<Vec<(u64, BranchResult)>>`

### SinkConfig Changes (Task 2)
- Added `timeout: Option<std::time::Duration>` field for per-sink timeout override
- Updated `register_sink()` to accept timeout parameter

### ExecutionContext Changes (Task 3)
- Added `branch_id: Option<u64>` and `fan_out_id: Option<u64>` fields
- Updated `with_trace()` to accept and pass through branch metadata
- Default values are `None` for non-fan-out paths

### DlqMetadata Changes (Task 4)
- Added `branch_id: Option<u64>` and `fan_out_id: Option<u64>` fields
- Updated `DlqMetadata::new()` to accept these new fields
- Updated all call sites (worker.rs, worker_pool/mod.rs, batch_loop.rs, offset_tracker.rs)

### Worker Loop Wiring (Task 5)
- Generate `fan_out_id` via monotonic counter before spawning branches
- Each branch registered via `register_branch()` before spawning
- `ExecutionContext` created with `branch_id` and `fan_out_id`
- `wait_all()` awaited in spawned task before worker goes to Idle
- Error/Timeout branches routed to DLQ via `dlq_producer.produce_async()` with `dlq_router.route()`
- DLQ metadata includes `branch_id` and `fan_out_id` for replay routing

### Per-Sink Timeout (Task 6)
- Added `invoke_mode_with_timeout_override()` to PythonHandler
- When `timeout` parameter is `Some(d)`, uses `d` regardless of handler_timeout
- When `timeout` parameter is `None`, falls back to handler_timeout

## Deviations from Plan

None — plan executed exactly as written.

## Self-Check

- [x] All tasks committed individually
- [x] All compilation checks pass (`cargo check`)
- [x] FanOutTracker has 16 lines matching wait_all/register_branch/record_branch_result/completed/notify/BranchResults
- [x] SinkConfig has timeout: Option<Duration> field
- [x] ExecutionContext has branch_id and fan_out_id fields
- [x] DlqMetadata has branch_id and fan_out_id fields
- [x] invoke_mode_with_timeout_override method present
