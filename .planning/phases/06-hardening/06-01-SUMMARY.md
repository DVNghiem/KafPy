---
phase: 06-hardening
plan: '01'
subsystem: error-handling
tags: [rust, thiserror, error-handling, structured-fields]

# Dependency graph
requires:
  - phase: 03-python-handler-api
    provides: ConsumerDispatcher wiring to Dispatcher
provides:
  - ConsumerError with structured fields (broker, topic, partition, offset, bytes_preview)
  - DispatchError with structured fields (queue_name, capacity, reason, broker)
affects:
  - phase: 06-hardening (plan 02)

# Tech tracking
tech-stack:
  added: []
  patterns:
    - Structured error fields with thiserror #[source] annotations
    - Box<dyn Error + Send + Sync> for flexible error sources

key-files:
  created: []
  modified:
    - src/consumer/error.rs - ConsumerError with structured variants
    - src/dispatcher/error.rs - DispatchError with structured variants
    - src/consumer/runner.rs - Updated Subscription/Receive call sites
    - src/dispatcher/consumer_dispatcher.rs - Updated call sites and pattern matching
    - src/dispatcher/mod.rs - Updated Dispatcher send methods
    - src/dispatcher/queue_manager.rs - Updated send_to_handler_by_id

key-decisions:
  - "Used Box<dyn std::error::Error + Send + Sync> for Serialization source to support multiple deserializers"
  - "Used 'unknown' broker placeholder for pause/resume partition where broker context is unavailable"

patterns-established:
  - "Structured error variants with actionable context fields (topic, partition, offset, broker)"
  - "#[source] annotation for wrapped errors to preserve error chain"

requirements-completed: [LH-05]

# Metrics
duration: 18min
completed: 2026-04-29
---

# Phase 06-hardening Plan 01: Error Structured Fields Summary

**ConsumerError and DispatchError converted from stringly-typed variants to structured fields with actionable context (topic, partition, offset, broker, bytes_preview)**

## Performance

- **Duration:** 18 min
- **Started:** 2026-04-29T06:34:08Z
- **Completed:** 2026-04-29T06:52:00Z
- **Tasks:** 3
- **Files modified:** 6

## Accomplishments
- ConsumerError::Subscription { broker, message } - includes broker address for connection failures
- ConsumerError::Receive { handler, timeout_ms, message } - includes handler name and timeout for debugging
- ConsumerError::Serialization { topic, partition, offset, bytes_preview, source } - full message context with flexible error source
- ConsumerError::Processing { topic, partition, offset, message } - processing failure context
- DispatchError::QueueFull { queue_name, capacity } - includes capacity for queue sizing
- DispatchError::Backpressure { queue_name, reason } - includes reason for backpressure classification
- DispatchError::UnknownTopic { topic, broker } - broker context for topic resolution failures
- DispatchError::HandlerNotRegistered { topic } - consistent single-field naming
- DispatchError::QueueClosed { topic } - consistent single-field naming
- All call sites updated across runner.rs, consumer_dispatcher.rs, mod.rs, queue_manager.rs

## Task Commits

Each task was committed atomically:

1. **Task 1: Improve ConsumerError with structured fields** - `18e1961` (feat)
2. **Task 2: Improve DispatchError with structured fields** - `18e1961` (feat, same commit)
3. **Task 3: Update error.rs re-exports and verify module compiles** - `18e1961` (feat, same commit)

**Plan metadata:** `18e1961` (feat: complete plan 06-01)

## Files Created/Modified
- `src/consumer/error.rs` - ConsumerError with Subscription { broker, message }, Receive { handler, timeout_ms, message }, Serialization { topic, partition, offset, bytes_preview, source }, Processing { topic, partition, offset, message }
- `src/dispatcher/error.rs` - DispatchError with QueueFull { queue_name, capacity }, Backpressure { queue_name, reason }, UnknownTopic { topic, broker }, HandlerNotRegistered { topic }, QueueClosed { topic }
- `src/consumer/runner.rs` - Updated all ConsumerError::Subscription and ConsumerError::Receive call sites
- `src/dispatcher/consumer_dispatcher.rs` - Updated ConsumerError and DispatchError call sites, pattern matching for Backpressure { .. } and HandlerNotRegistered { .. }
- `src/dispatcher/mod.rs` - Updated Dispatcher::send and send_with_policy_and_signal with new DispatchError variants
- `src/dispatcher/queue_manager.rs` - Updated send_to_handler_by_id with new DispatchError variants

## Decisions Made
- Used `Box<dyn std::error::Error + Send + Sync>` for Serialization source type to support JSON, msgpack, and other deserializers flexibly
- Used `#[source]` annotation on wrapped errors to preserve error chain for `std::error::Error::source()` compatibility
- Used "unknown" placeholder for broker field in pause_partition/resume_partition where broker info is not readily available

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
- Pattern matching limitation: Rust doesn't allow arbitrary expressions in match patterns - fixed by using simple field bindings `queue_name` and `reason` instead of trying to match specific values
- Test assertion `matches!(result, Err(DispatchError::Backpressure(_)))` failed after variant change - updated to `matches!(result, Err(DispatchError::Backpressure { .. }))`

## Next Phase Readiness
- Error struct changes complete and verified with `cargo check --lib`
- Ready for plan 06-02 (remaining hardening tasks)
- LH-05 requirement satisfied

---
*Phase: 06-hardening*
*Completed: 2026-04-29*
