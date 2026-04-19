# Phase 34: Rebalance Handling - Context

**Gathered:** 2026-04-19
**Status:** Ready for planning

## Phase Boundary

Handle Kafka consumer rebalance events: partition assignment changes (Assign/Revoke/Error), track ownership per topic-partition, and prevent offset advance on revoked partitions.

## Implementation Decisions

### RB-01: PartitionOwnership State Machine
- **D-01:** States: `Owned`, `Revoked`, `PendingAssign`
  - `Owned` — partition is currently assigned and processing
  - `Revoked` — partition was revoked but may have in-flight work
  - `PendingAssign` — partition assignment received but not yet fully owned
- **D-02:** Owned/Revoked transitions driven by `RebalanceHandler` calling `update_assignment()`
- **D-03:** `is_owned(topic, partition) -> bool` used by `record_ack()` to guard offset recording

### RB-02: RebalanceEvent Channel
- **D-04:** `RebalanceEvent` enum: `Assign(Vec<TopicPartition>)`, `Revoke(Vec<TopicPartition>)`, `Error(String)`
- **D-05:** Channel: `broadcast::Sender<RebalanceEvent>` — single producer (ConsumerRunner), multiple consumers (Dispatcher, WorkerPool, OffsetCommitter)
- **D-06:** Rebalance detection via assignment comparison in `ConsumerRunner::run()` loop — compare `consumer.assignment()` snapshot on each recv iteration

### RB-03: ConsumerDispatcher Pause/Resume
- **D-07:** On `Revoke`: pause via `runner.pause(tpl)` for revoked TPs immediately
- **D-08:** On `Assign`: resume via `runner.resume(tpl)` for newly assigned TPs
- **D-09:** Pause is HARD (rdkafka-level) — prevents messages from being consumed for revoked partitions

### RB-04: record_ack Ownership Guard
- **D-10:** `record_ack(topic, partition, offset)` checks `PartitionOwnership::is_owned()` before calling `ack()`
- **D-11:** If partition is not owned (revoked): log warning and skip (do NOT advance offset)
- **D-12:** Check is per-message (every ack) — not per-partition boolean caching

### RB-05: flush_pending_retries
- **D-13:** `flush_pending_retries()` is a new method on `RetryCoordinator` (separate from `flush_failed_to_dlq`)
- **D-14:** Sends ALL pending (not-yet-processed) retries to DLQ immediately on rebalance revocation
- **D-15:** `flush_failed_to_dlq` on `OffsetCoordinator` handles already-committed-but-failed offsets

### Integration with ShutdownCoordinator
- **D-16:** During `Draining`/`Finalizing` phases, rebalance events still flow but components handle gracefully
- **D-17:** `ShutdownCoordinator` does NOT transition on rebalance — rebalance is separate from shutdown lifecycle

## Existing Code Insights

### Integration Points
- `ConsumerRunner::run()` — add assignment comparison loop for rebalance detection
- `OffsetCommitter::run()` — subscribe to RebalanceEvent channel, commit pending on Revoke
- `WorkerPool::shutdown()` — already calls `flush_failed_to_dlq`, needs `flush_pending_retries` call

### Key files (from Phase 33 research)
- `src/coordinator/shutdown.rs` — ShutdownCoordinator already in place
- `src/consumer/runner.rs` — needs rebalance detection added to run() loop
- `src/coordinator/offset_tracker.rs:247` — `record_ack` currently calls `ack()` directly, needs ownership guard

## Deferred Ideas
- Python-visible rebalance callbacks (v1.9 opt-in feature)
- Incremental rebalance (KIP-848, Kafka 4.0+)

---
*Phase: 34-rebalance-handling*
*Context gathered: 2026-04-19*
