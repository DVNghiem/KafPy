---
phase: 13-consumer-runner-store-offset
plan: '01'
subsystem: consumer
tags: [rdkafka, store_offset, consumer-config, offset-management]

# Dependency graph
requires:
  - phase: 11-offsettracker-core
    provides: OffsetTracker with highest_contiguous() and committed_offset() interface
  - phase: 12-offsetcommitter
    provides: OffsetCommitter with commit_partition() method that calls store_offset + commit
provides:
  - ConsumerRunner::store_offset async method via spawn_blocking
  - enable_auto_offset_store field in ConsumerConfig (default false)
  - enable_auto_offset_store builder method
  - enable.auto.offset.store=false in rdkafka config
affects:
  - phase: 14 (OffsetCoordinator trait integration)
  - phase: 15 (WorkerPool offset tracking integration)

# Tech tracking
tech-stack:
  added: []
  patterns:
    - spawn_blocking for blocking rdkafka C library calls
    - Two-phase manual offset management (store_offset then commit)

key-files:
  created: []
  modified:
    - src/consumer/runner.rs
    - src/consumer/config.rs

key-decisions:
  - "ConsumerRunner::store_offset uses spawn_blocking to avoid blocking async executor on rdkafka store_offset"
  - "enable_auto_offset_store defaults to false for manual two-phase offset management"
  - "store_offset errors do not advance stored offset — next cycle retries"

patterns-established:
  - "Async wrapper around blocking rdkafka consumer methods via spawn_blocking"
  - "Config field pattern: struct field + builder field + builder default + builder method + build() + into_rdkafka_config()"

requirements-completed:
  - COMMIT-01
  - COMMIT-02
  - CONFIG-01
  - CONFIG-02

# Metrics
duration: 6 min
completed: 2026-04-17T00:28:51+07:00
---

# Phase 13 Plan 01: ConsumerRunner store_offset + enable_auto_offset_store Config Summary

**ConsumerRunner::store_offset async method via spawn_blocking, ConsumerConfig with enable_auto_offset_store (default false)**

## Performance

- **Duration:** 6 min
- **Started:** 2026-04-17T00:22:20+07:00
- **Completed:** 2026-04-17T00:28:51+07:00
- **Tasks:** 1 (2 requirements addressed in single atomic commit)
- **Files modified:** 2

## Accomplishments
- Added `ConsumerRunner::store_offset(topic, partition, offset)` async method that calls rdkafka's `store_offset` via `tokio::task::spawn_blocking`
- Added `enable_auto_offset_store: bool` to `ConsumerConfig` struct (default `false`)
- Added `enable_auto_offset_store(bool)` builder method to `ConsumerConfigBuilder`
- Set `enable.auto.offset.store=false` in rdkafka config via `into_rdkafka_config()`

## Task Commits

Each task was committed atomically:

1. **Task 1 + 2: ConsumerRunner store_offset + enable_auto_offset_store config** - `9af3722` (feat)

**Plan metadata:** `9af3722` (docs: complete plan)

## Files Created/Modified
- `src/consumer/runner.rs` - Added `store_offset` async method (lines 131-146)
- `src/consumer/config.rs` - Added `enable_auto_offset_store` field, builder method, and rdkafka config (lines 24, 79, 99, 141-144, 212, 239-240)

## Decisions Made
- Used `spawn_blocking` pattern for rdkafka's blocking `store_offset` call — same pattern used elsewhere in the codebase for Python GIL calls
- `enable_auto_offset_store` defaults to `false` to ensure manual two-phase offset management is active by default
- `store_offset` errors are mapped to `ConsumerError::Receive` to maintain consistent error handling

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- Phase 14 (OffsetCoordinator trait) is ready to proceed — `ConsumerRunner::store_offset` and `commit` are both available
- No blockers or concerns

---
*Phase: 13-consumer-runner-store-offset*
*Completed: 2026-04-17*