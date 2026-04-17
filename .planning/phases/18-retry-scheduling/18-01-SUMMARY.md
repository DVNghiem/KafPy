---
phase: 18-retry-scheduling
plan: 01
subsystem: retry
tags: [retry, backoff, jitter, exponential-backoff]

# Dependency graph
requires:
  - phase: 17-failure-classification
    provides: FailureReason enum with Retryable/Terminal/NonRetryable categories
provides:
  - RetryPolicy struct with configurable max_attempts, base_delay, max_delay, jitter_factor
  - RetrySchedule for computing exponential backoff delays with jitter
affects:
  - phase-18-02 (retry executor integration)
  - ConsumerConfig (RETRY-05)
  - PythonHandler (RETRY-06)

# Tech tracking
tech-stack:
  added:
    - rand = "0.8" crate for random jitter generation
  patterns:
    - Exponential backoff with jitter formula
    - Builder-style configuration with Default trait

key-files:
  created:
    - src/retry/policy.rs (95 lines) - RetryPolicy and RetrySchedule
    - src/retry/mod.rs (3 lines) - module exports

key-decisions:
  - "0-indexed attempts: attempt 0 = first retry, attempt 1 = second retry"
  - "Jitter formula: multiplier ranges from (1 - jitter_factor) to (1 + jitter_factor)"
  - "Exponential cap: min(base_delay * 2^attempt, max_delay)"

patterns-established:
  - "Policy + Schedule pattern: policy holds config, schedule computes delays"
  - "Default construction with sensible values for quick setup"

requirements-completed:
  - RETRY-01
  - RETRY-02

# Metrics
duration: 5min
completed: 2026-04-17
---

# Phase 18: Retry Scheduling Summary

**RetryPolicy struct with exponential backoff and jitter for retry delay computation**

## Performance

- **Duration:** 5 min
- **Started:** 2026-04-17
- **Completed:** 2026-04-17
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments
- Created `src/retry/` module with `RetryPolicy` and `RetrySchedule` types
- `RetryPolicy` configurable via `Default` and `new()` with max_attempts, base_delay, max_delay, jitter_factor
- `RetrySchedule::next_delay(attempt)` computes delays using exponential backoff capped at max_delay with jitter applied

## Task Commits

Each task was committed atomically:

1. **Task 1: Create retry module with RetryPolicy and RetrySchedule** - `e33e64b` (feat)
2. **Task 2: Add rand dependency to Cargo.toml** - `e11a302` (chore)

**Plan metadata:** (summary commit at phase completion)

## Files Created/Modified
- `src/retry/policy.rs` - RetryPolicy struct and RetrySchedule with exponential backoff + jitter
- `src/retry/mod.rs` - Module exports for RetryPolicy and RetrySchedule
- `Cargo.toml` - Added rand = "0.8" dependency

## Decisions Made
- 0-indexed attempt numbers: attempt 0 = first retry after initial failure
- Jitter multiplier formula: `1 - jitter_factor + rng * jitter_factor * 2` gives range [1-jitter_factor, 1+jitter_factor]
- Exponential growth capped at max_delay to prevent unbounded delays
- Policy stores config; Schedule is a thin compute layer borrowing from Policy

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None

## Next Phase Readiness
- RetryPolicy ready for consumption by ConsumerConfig (plan 18-02)
- RetrySchedule::next_delay ready for worker_loop integration (plan 18-02)

---
*Phase: 18-retry-scheduling*
*Completed: 2026-04-17*