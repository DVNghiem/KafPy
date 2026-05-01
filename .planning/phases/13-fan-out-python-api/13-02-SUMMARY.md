---
phase: 13-fan-out-python-api
plan: "02"
subsystem: observability
tags: [fan-out, prometheus, metrics, rust]

# Dependency graph
requires:
  - phase: "13-01"
    provides: "FanOutTracker, BranchResult enum, SinkConfig, register_fanout API"
provides:
  - "FanOutMetrics struct with record_branch_duration() and record_branch_completion()"
  - "kafpy.fanout.branch_duration_seconds histogram (pre-registered)"
  - "kafpy.fanout.branch_total counter (pre-registered)"
  - "OBSV-01 fan-out metrics emitted from wait_all() completion callback"
affects:
  - "Phase 13 (OBSV-02 trace context branching)"

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Fan-out metrics on branch completion only (not primary path)"
    - "BranchResult-to-outcome label conversion (ok/error/timeout)"

key-files:
  created: []
  modified:
    - "src/observability/metrics.rs"
    - "src/worker_pool/worker.rs"

key-decisions:
  - "D-05: fan_out_branch_duration_seconds histogram, fan_out_branch_total counter with fan_out_id/branch_name/branch_outcome labels"
  - "D-06: branch_outcome = ok | error | timeout"
  - "Primary throughput NOT double-counted — no fan-out metric on primary dispatch path"
  - "Metrics emitted in wait_all() completion task after all branches finish"

patterns-established:
  - "Fan-out metrics recorder struct (FanOutMetrics) mirroring HandlerMetrics pattern"

requirements-completed:
  - "OBSV-01"

# Metrics
duration: 14min
completed: 2026-05-01
---

# Phase 13 Plan 02: Fan-Out Metrics Summary

**Fan-out branch metrics (histogram + counter) with fan_out_id, branch_name, branch_outcome labels. Primary throughput NOT double-counted.**

## Performance

- **Duration:** 14 min
- **Started:** 2026-05-01T02:51:00Z
- **Completed:** 2026-05-01T02:55:00Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments

- Added `FanOutMetrics` struct with `record_branch_duration()` and `record_branch_completion()` methods
- Pre-registered `kafpy.fanout.branch_duration_seconds` histogram and `kafpy.fanout.branch_total` counter in `SharedPrometheusSink::new()`
- Emitted `fan_out_branch_total` counter per branch completion via `FanOutMetrics::outcome_from_result()` conversion
- Primary throughput path has no fan-out metrics (OBSV-01 truth satisfied)

## Task Commits

Each task was committed atomically:

1. **Task 1: Add FanOutMetrics to metrics.rs** - `6b23e40` (feat)

**Plan metadata:** `85530f8` (docs: complete plan 13-01)

## Files Created/Modified

- `src/observability/metrics.rs` - Added FanOutMetrics struct, outcome constants, and metric registrations
- `src/worker_pool/worker.rs` - Added FanOutMetrics import and metrics emission in wait_all() completion task

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Removed spurious `branch_start` Instant capture inside spawn**
- **Found during:** Task 2 implementation
- **Issue:** `branch_start` was added but `branch_elapsed` was never computed or used, making the Instant pointless
- **Fix:** Removed unused Instant since the metrics are emitted after `wait_all()` which already gates on all branches completing
- **Files modified:** `src/worker_pool/worker.rs`
- **Commit:** `6b23e40`

**2. [Rule 3 - Blocking] Fixed moved value errors for trace context in loop**
- **Found during:** `cargo check --lib`
- **Issue:** `ctx_trace_id`/`ctx_span_id` were captured in `async move` inside a `for` loop, causing moved-value errors across iterations
- **Fix:** Moved clone of trace context outside the loop into `parent_trace_id_opt`/`parent_span_id_opt` before the loop
- **Files modified:** `src/worker_pool/worker.rs`
- **Commit:** `6b23e40`

## Verification

- `cargo check --lib` passes with warnings only (no errors)
- Fan-out metrics registrations present in `SharedPrometheusSink::new()`
- `FanOutMetrics` exported from `observability::metrics` module

## Known Stubs

None.

## Threat Flags

None — fan-out metrics are internal, labels are bounded by active dispatch count.
