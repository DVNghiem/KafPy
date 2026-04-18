# Phase 30: Kafka-Level Metrics - Context

**Gathered:** 2026-04-18
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 30 instruments the rdkafka layer with Kafka-specific metrics: consumer_lag gauge, assignment_size gauge, committed_offset gauge per topic-partition, offset_commit_latency histogram, and assignment change counter. Polling-based (every 10s) to avoid hot-path overhead. Thread-safe via Arc<AtomicU64> snapshots from rdkafka statistics callback.

</domain>

<decisions>
## Implementation Decisions

### KafkaMetrics Struct — OBS-20
- **OBS-20:** `KafkaMetrics` struct exposes consumer_lag gauge, assignment_size gauge, and committed_offset gauge per topic-partition
- Per-TP gauges keyed by (topic, partition)

### Consumer Lag Calculation — OBS-21
- **OBS-21:** Consumer lag = `highwater - position` per partition using rdkafka `TopicPartitionList` API (no extra Kafka API calls)

### Polling Interval — OBS-22
- **OBS-22:** Background polling task updates Kafka gauges every 10s (default), configurable via ObservabilityConfig
- Configurable: allows user to set polling interval (default 10s, can be overridden)

### OffsetTracker Snapshots — OBS-23
- **OBS-23:** `OffsetTracker::offset_snapshots()` returns current committed offset per topic-partition for gauge reporting

### Thread Safety — OBS-24
- **OBS-24:** rdkafka statistics callback thread-safe — stats shared via `Arc<AtomicU64>` snapshots between rdkafka internal thread and Tokio runtime

### Offset Commit Latency — OBS-25
- **OBS-25:** `offset_commit_latency` histogram records time from offset advancement to commit acknowledgment

### Assignment Change Counter — OBS-26
- **OBS-26:** Assignment change counter emits event when topic-partition assignment changes via rebalance callback

</decisions>

<canonical_refs>
## Canonical References

### Codebase References
- `src/coordinator/offset_tracker.rs` — OffsetTracker for offset_snapshots()
- `src/dispatcher/mod.rs` — ConsumerDispatcher with queue_depth/inflight
- `src/consumer/runner.rs` — rdkafka consumer integration

### Prior Phase Context
- `.planning/phases/28-metrics-infrastructure/28-CONTEXT.md` — metrics patterns, polling-based gauge updates
- `.planning/phases/29-tracing-infrastructure/29-CONTEXT.md` — ObservabilityConfig structure

### Requirements
- `.planning/REQUIREMENTS.md` — OBS-20 through OBS-26 (Phase 30 requirements)

</canonical_refs>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---
*Phase: 30-kafka-level-metrics*
*Context gathered: 2026-04-18*
