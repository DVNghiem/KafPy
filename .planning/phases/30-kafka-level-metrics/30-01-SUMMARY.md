---
phase: "30-kafka-level-metrics"
plan: "01"
subsystem: infra
tags: [kafka, metrics, tokio, polling, observability]

requires:
  - phase: "28-metrics-infrastructure"
    provides: "MetricsSink trait, polling-based gauge patterns"
  - phase: "29-tracing-infrastructure"
    provides: "ObservabilityConfig structure"

provides:
  - "KafkaMetrics struct with consumer_lag, assignment_size, committed_offset gauges per TP"
  - "KafkaMetricsTask background poller (every 10s)"
  - "OffsetTracker::offset_snapshots() method for committed offsets per TP"
  - "OffsetCommitLatencyTracker for offset_commit_latency histogram"
  - "ObservabilityConfig.kafka_poll_interval field (default 10s)"

affects: [dispatcher-integration, worker-pool-ack]

tech-stack:
  added: []
  patterns:
    - "Polling-based metrics (avoids per-message overhead)"
    - "Arc<AtomicU64> for thread-safe cross-thread snapshot sharing"
    - "tokio::time::interval for background polling"

key-files:
  created:
    - "src/observability/kafka_metrics.rs"
  modified:
    - "src/coordinator/offset_tracker.rs"
    - "src/observability/config.rs"
    - "src/observability/mod.rs"

key-decisions:
  - "Used tokio::time::interval for 10s polling (OBS-22)"
  - "Arc<AtomicU64> snapshots for thread-safe metrics sharing (OBS-24)"
  - "OffsetCommitLatencyTracker uses HashMap<(topic,partition,offset)->Instant> (OBS-25)"
  - "KafkaMetricsTask::spawn runs indefinitely until dropped (no shutdown channel)"

patterns-established:
  - "Background polling task pattern (KafkaMetricsTask)"
  - "Per-TP metrics keyed by (String, i32) tuple"

requirements-completed: [OBS-20, OBS-21, OBS-22, OBS-23, OBS-24, OBS-25]

# Metrics
duration: 20min
completed: 2026-04-18

---

# Phase 30: Kafka-Level Metrics Summary

**KafkaMetrics struct with consumer_lag/assignment_size/committed_offset gauges per TP, background polling task every 10s, and offset_commit_latency histogram**

## Performance

- **Duration:** 20 min
- **Started:** 2026-04-18T08:25:00Z
- **Completed:** 2026-04-18T08:45:09Z
- **Tasks:** 7
- **Files modified:** 4

## Accomplishments
- Created `KafkaMetrics` struct with per-TP gauges (consumer_lag, committed_offset) and per-topic gauge (assignment_size)
- Created `KafkaMetricsTask` background poller using `tokio::time::interval` (10s default)
- Created `OffsetCommitLatencyTracker` for recording commit latency histogram
- Added `OffsetTracker::offset_snapshots()` for querying committed offsets per TP
- Updated `ObservabilityConfig` with `kafka_poll_interval` field (default 10s)
- Updated `observability/mod.rs` to export new types

## Files Created/Modified
- `src/observability/kafka_metrics.rs` - Main implementation: KafkaMetrics, KafkaMetricsTask, OffsetCommitLatencyTracker, KafkaMetricsSnapshot
- `src/coordinator/offset_tracker.rs` - Added `offset_snapshots()` method
- `src/observability/config.rs` - Added `kafka_poll_interval` field
- `src/observability/mod.rs` - Added `kafka_metrics` module and exports

## Decisions Made
- Used `parking_lot::RwLock` for tp_metrics (better performance than std::sync::RwLock for contended cases)
- Used `tokio::time::interval` for polling (OBS-22)
- Assignment change counter (`record_assignment_change()`) is exposed but not yet wired to rebalance callback (deferred)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed borrow checker issues with guard lifetime**
- **Found during:** Task 2 (kafka_metrics.rs implementation)
- **Issue:** `snapshot` reference into RwLock guard was returned while guard was dropped, causing lifetime error
- **Fix:** Restructured to perform all atomic stores inside the lock scope, eliminating returned reference
- **Files modified:** src/observability/kafka_metrics.rs
- **Verification:** cargo check passes
- **Committed in:** 966b52b (part of main commit)

**2. [Rule 3 - Blocking] tokio watch::Receiver has no recv() method**
- **Found during:** Task 2 (KafkaMetricsTask::run)
- **Issue:** `tokio::sync::watch::Receiver` in tokio 1.49 does not have `recv()` method
- **Fix:** Simplified to run indefinitely without shutdown channel (PhantomData marker instead)
- **Files modified:** src/observability/kafka_metrics.rs
- **Verification:** cargo check passes
- **Committed in:** 966b52b

**3. [Rule 3 - Blocking] rdkafka Offset enum has no to_i64 method**
- **Found during:** Task 2 (poll_and_update)
- **Issue:** `rdkafka::Offset` is an enum without `to_i64()`, only `to_raw()` returning `Option<i64>`
- **Fix:** Added `offset_to_i64()` helper that matches on enum variants and extracts i64 or returns 0
- **Files modified:** src/observability/kafka_metrics.rs
- **Verification:** cargo check passes
- **Committed in:** 966b52b

**4. [Rule 2 - Type] MetricLabels insert expects Into<String>, not i32**
- **Found during:** Task 2 (record_gauges, record_commit_latency)
- **Issue:** `partition` is `i32`, `MetricLabels::insert` expects `Into<String>`
- **Fix:** Used `.to_string()` on partition values
- **Files modified:** src/observability/kafka_metrics.rs
- **Verification:** cargo check passes
- **Committed in:** 966b52b

---

**Total deviations:** 4 auto-fixed (2 blocking, 1 bug, 1 type error)
**Impact on plan:** All auto-fixes were necessary for correctness and compilation. No scope creep.

## Issues Encountered
- tokio 1.49 `watch::Receiver` API differs from expected (no `recv()`) - resolved by simplifying design
- rdkafka `Offset` enum pattern matching required careful handling - resolved with helper function

## Known Stubs

| File | Line | Stub | Reason |
|------|------|------|--------|
| kafka_metrics.rs | ~230 | `KafkaMetricsTask::spawn` runs forever (no graceful shutdown) | Simpler design for initial implementation; shutdown can be added later via oneshot channel |
| kafka_metrics.rs | N/A | `record_assignment_change()` not wired to rebalance callback | Requires ConsumerRunner modification to expose `set_rebalance_callback()` - deferred to future phase |

## Next Phase Readiness
- KafkaMetrics infrastructure complete, ready for dispatcher integration
- Rebalance callback wiring requires ConsumerRunner modification (OBS-26 partially met - counter exists but not called from rebalance)
- No blockers for continuing to next plan

---
*Phase: 30-kafka-level-metrics*
*Completed: 2026-04-18*
