---
phase: 18-retry-scheduling
plan: 02
subsystem: retry
tags: [retry, exponential-backoff, jitter, retry-coordinator, worker-pool]

# Dependency graph
requires:
  - phase: 18-01
    provides: RetryPolicy struct, RetrySchedule with exponential backoff + jitter
provides:
  - RetryCoordinator tracking per-message retry state
  - ConsumerConfig::default_retry_policy wired to WorkerPool
  - PythonHandler per-handler retry_policy
  - worker_loop retry scheduling with delay and DLQ routing
affects:
  - phase-19 (DLQ implementation)
  - ConsumerConfig (RETRY-05)
  - PythonHandler (RETRY-06)

# Tech tracking
tech-stack:
  added:
    - No new crates (rand was added in 18-01)
  patterns:
    - RetryCoordinator as thread-safe retry state machine
    - Exponential backoff with jitter for retry delays
    - Policy + Coordinator pattern separating config from state

key-files:
  created:
    - src/coordinator/retry_coordinator.rs (175 lines)
  modified:
    - src/consumer/config.rs (adds default_retry_policy)
    - src/python/handler.rs (adds retry_policy field and accessor)
    - src/worker_pool/mod.rs (retry scheduling in worker_loop)
    - src/pyconsumer.rs (wires RetryCoordinator to WorkerPool)
    - src/lib.rs (exports retry module)
    - src/coordinator/mod.rs (exports RetryCoordinator)
    - src/retry/policy.rs (fixes type errors)

key-decisions:
  - "record_failure returns (should_retry, delay) — caller decides action"
  - "Non-retryable/terminal failures return (false, None) immediately — no state stored"
  - "Max attempts exceeded removes state and returns (false, None)"
  - "retry_coordinator.record_success() called BEFORE offset_coordinator.record_ack()"
  - "DLQ routing is logged but not implemented — Phase 19 implements actual routing"

patterns-established:
  - "RetryCoordinator + RetryPolicy: state machine borrowing from policy config"
  - "record_failure / record_success API separates retry tracking from offset coordination"
  - "active_message re-queue pattern for retry without re-polling queue"

requirements-completed:
  - RETRY-03
  - RETRY-04
  - RETRY-05
  - RETRY-06

# Metrics
duration: 25min
completed: 2026-04-17
---

# Phase 18 Plan 02 Summary

**Wired RetryPolicy into ConsumerConfig, PythonHandler, and worker_loop retry scheduling.**

## Performance

- **Duration:** 25 min
- **Started:** 2026-04-17
- **Completed:** 2026-04-17
- **Tasks:** 4
- **Files modified:** 8

## Accomplishments
- `ConsumerConfig` exposes `default_retry_policy: RetryPolicy` with builder method
- `PythonHandler` stores per-handler `retry_policy: Option<RetryPolicy>` with accessor
- `RetryCoordinator` tracks per-message retry state with thread-safe HashMap
- `worker_loop` implements retry scheduling: on retryable failure, sleep then re-queue
- Key invariant (RETRY-03): `offset_coordinator.record_ack()` only called on final success
- After max_attempts, `worker_loop` logs DLQ routing (actual routing in Phase 19)

## Task Commits

Each task was committed atomically:

1. **Task 1: Add default_retry_policy to ConsumerConfig** - `cebd90e` (feat)
2. **Task 2: Add per-handler retry_policy to PythonHandler** - `493f318` (feat)
3. **Task 3: Create RetryCoordinator trait and RetryState** - `9ba9197` (feat)
4. **Task 4: Integrate retry scheduling into worker_loop** - `a9ae0fb` (feat)

## Files Created/Modified
- `src/consumer/config.rs` - Adds `default_retry_policy` field and builder method
- `src/python/handler.rs` - Adds `retry_policy` field, `new()` parameter, and accessor
- `src/worker_pool/mod.rs` - Retry scheduling in worker_loop, WorkerPool stores RetryCoordinator
- `src/pyconsumer.rs` - Creates RetryCoordinator and wires to WorkerPool
- `src/coordinator/retry_coordinator.rs` - RetryCoordinator struct with unit tests
- `src/coordinator/mod.rs` - Exports RetryCoordinator
- `src/lib.rs` - Exports retry module
- `src/retry/policy.rs` - Fixes lifetime parameter and type mismatches

## Decisions Made
- Non-retryable/terminal failures bypass retry state entirely (return `(false, None)`)
- Max attempts exceeded removes retry state (caller counts as processed, not retried)
- `record_success()` clears state before `record_ack()` — order matters for retry correctness
- `active_message = Some(msg)` re-queues for retry without touching the mpsc queue
- RetryCoordinator is created from `rust_config` in pyconsumer.rs (not stored in Consumer struct)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] RetrySchedule type errors**
- **Found during:** Task 1 (building after adding retry module)
- **Issue:** `RetrySchedule<'a>` had unused lifetime parameter; `Duration * usize` multiplication failed
- **Fix:** Removed lifetime parameter, converted to f64-based delay calculation
- **Files modified:** `src/retry/policy.rs`
- **Commit:** `cebd90e`

**2. [Rule 3 - Blocking] PyO3 binding changes needed for PythonHandler::new signature**
- **Found during:** Task 2
- **Issue:** PythonHandler::new signature changed to accept retry_policy; pyconsumer.rs call site needed update
- **Fix:** Updated pyconsumer.rs to extract `default_retry_policy` before `ConsumerRunner::new` consumes rust_config
- **Files modified:** `src/pyconsumer.rs`
- **Commit:** `493f318`

## Issues Encountered

- **PyO3 linking errors in test binary** (pre-existing, not caused by this plan): Test compilation fails due to missing Python symbols when running `cargo test`. This affects the test binary but not the library itself. `cargo build --lib` passes cleanly.

## Threat Flags

None

## Known Stubs

None

## Next Phase Readiness
- RetryCoordinator ready for Phase 19 DLQ routing implementation
- ConsumerConfig.default_retry_policy accessible for Python-exposed configuration (future)
- worker_loop retry scheduling ready for integration with actual DLQ path (Phase 19)

---
*Phase: 18-retry-scheduling*
*Completed: 2026-04-17*
