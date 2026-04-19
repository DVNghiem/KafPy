# Feature Research: Graceful Shutdown and Rebalance Handling

**Domain:** Kafka Consumer Framework — Graceful Shutdown & Rebalance Lifecycle
**Researched:** 2026-04-19
**Confidence:** MEDIUM-HIGH

## Feature Landscape

### Table Stakes (Users Expect These)

Features users assume exist in any production-grade Kafka framework. Missing these = data loss risk, duplicate processing, or incorrect offset commits during planned and unplanned interruptions.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| **Graceful shutdown sequence** | Stop intake → drain queues → commit offsets → stop workers. Missing this = uncommitted offsets, duplicate messages on restart. | MEDIUM | KafPy currently has `stop()` + `WorkerPool::shutdown()` but missing `rd_kafka_consumer_close()` call to leave group properly. |
| **Rebalance event handling** | Consumer group membership changes constantly in production. Without callbacks, partition revocation causes data loss or duplication. | MEDIUM | librdkafka provides `rebalance_cb` callback. KafPy does not implement this. |
| **Partition ownership state machine** | Partitions transition through states: assigned → processing → draining → revoked. Each state requires different behavior. | MEDIUM | No explicit state machine in current implementation. |
| **Safe offset progression** | Commit only highest-contiguous offsets. Gaps (failed/retryable messages) must not be committed. | LOW | KafPy already implements `OffsetTracker` with BTreeSet highest-contiguous algorithm. |
| **Final offset commit on shutdown/rebalance** | Before leaving group or stopping, must commit current position. | MEDIUM | `graceful_shutdown()` exists in `OffsetCoordinator` but is only called from `WorkerPool::shutdown()`, not on rebalance. |
| **DLQ flush before offset commit** | Failed messages should reach DLQ before their offsets are committed. | LOW | KafPy already flushes DLQ before final commit (D-02 pattern). |

### Differentiators (Competitive Advantage)

Features that set KafPy apart. These go beyond basic consumer lifecycle handling.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **Partition-state-aware backpressure** | During rebalance, backpressure signals can pause partitions mid-revocation, causing gaps. State machine prevents this. | HIGH | KafPy already has `pause_partition` / `resume_partition` via `ConsumerDispatcher` but no rebalance integration. |
| **Graceful drain timeout** | Hard timeout on drain prevents infinite wait on slow handlers. Configurable per-partition deadline. | MEDIUM | No drain timeout currently. `WorkerPool::shutdown()` waits indefinitely for workers. |
| **Incremental rebalance support (KIP-848)** | Next-gen protocol (Kafka 4.0+) uses incremental assign/unassign. Different from classic revoke-all-then-assign-all. | HIGH | KIP-848 is preview in librdkafka 2.4+. Classic protocol is current default. |
| **Static group membership handling** | `group.instance.id` consumers retain partition ownership across restarts. Proper close is critical to avoid fencing. | MEDIUM | Not currently handled — `rd_kafka_consumer_close()` missing. |
| **Rebalance-induced DLQ flush** | When partitions are revoked, any in-flight failed messages should be flushed to DLQ immediately rather than waiting for shutdown. | MEDIUM | Only happens on explicit `WorkerPool::shutdown()`, not on rebalance. |
| **Python-visible rebalance callbacks** | Allow Python code to react to partition assignment changes (log, metrics, custom routing). | MEDIUM | No callback mechanism exposed to Python layer. |

### Anti-Features (Commonly Requested, Often Problematic)

| Anti-Feature | Why Problematic | Alternative |
|--------------|-----------------|-------------|
| **Auto-commit during shutdown** | Can commit offsets before processing completes, causing data loss. | Manual offset management with explicit `store_offset` + `commit` — KafPy already uses this. |
| **Blocking shutdown with no timeout** | Slow Python handlers can cause shutdown to hang indefinitely. | Configurable drain timeout; graceful degradation after timeout. |
| **Revoke-all then assign-all pattern** | Classic rebalance protocol revokes all partitions before assigning new ones. Forces complete drain cycle. | Incremental rebalance (KIP-848) minimizes churn; not yet implemented. |
| **Global consumer close without per-partition state** | Closing consumer without tracking which partitions were owned causes commit to happen on wrong offsets. | Partition ownership state machine tracks which offsets belong to which partition ownership epoch. |

## Feature Dependencies

```
[Partition Ownership State Machine]
    ├──requires──> [Rebalance Callback Registration]
    └──requires──> [Partition Handle Registry]

[Graceful Shutdown Sequence]
    ├──requires──> [Stop Consumer Intake (close receiver)]
    ├──requires──> [Drain In-Flight Messages]
    ├──requires──> [Flush Failed to DLQ]
    ├──requires──> [Commit Final Offsets]
    └──requires──> [rd_kafka_consumer_close()]

[Rebalance Event Handling]
    ├──requires──> [librdkafka rebalance_cb]
    ├──triggers──> [Partition State Transitions]
    └──triggers──> [DLQ Flush on Revoke]

[Safety Guarantees]
    ├──requires──> [Highest-Contiguous Offset Commit]
    └──ensures──> [No Gaps Committed]
```

## Feature Analysis

### 1. Graceful Shutdown Flow

The canonical shutdown sequence for a Kafka consumer (from librdkafka INTRODUCTION.md):

```c
// 1) Leave consumer group, commit final offsets
rd_kafka_consumer_close(rk);

// 2) Destroy handle object
rd_kafka_destroy(rk);
```

**KafPy's current flow:**
1. `Consumer::stop()` → cancels `shutdown_token`
2. `WorkerPool::shutdown()` → flushes DLQ → calls `graceful_shutdown()` → awaits worker completion
3. `ConsumerDispatcher::run()` → exits when stream returns `None` (sender dropped)
4. **MISSING**: No `rd_kafka_consumer_close()` call — consumer stays in group until `session.timeout.ms` expires

**What KafPy is missing for proper shutdown:**
- `ConsumerRunner::close()` method that calls `rd_kafka_consumer_close()`
- Coordination between `WorkerPool::shutdown()` and consumer close
- The `OffsetCommitter` background task is not signalled to drain

### 2. Rebalance Event Handling

librdkafka provides a `rebalance_cb` callback that receives three event types:

| Event | Meaning | Required Action |
|-------|---------|-----------------|
| `RD_KAFKA_EVENT_REBALANCE_ASSIGN` | Partitions assigned to this consumer | Register partitions, resume consumption |
| `RD_KAFKA_EVENT_REBALANCE_REVOKE` | Partitions revoked from this consumer | Pause consumption, drain in-flight, commit offsets, unregister |
| `RD_KAFKA_EVENT_REBALANCE_ERROR` | Rebalance failed | Log error, possibly wait for retry |

**Current KafPy implementation:** None. `ConsumerRunner` does not register a rebalance callback.

**What needs to be added:**
1. `ConsumerRebalanceHandler` trait or struct to receive callbacks
2. `ConsumerRunner::set_rebalance_callback()` to register with librdkafka
3. On `ASSIGN`: call `populate_partitions()`, register with `OffsetTracker`
4. On `REVOKE`: pause partition, flush in-flight to DLQ, commit offsets, unregister from `OffsetTracker`
5. On `ERROR`: log, possibly trigger retry with backoff

### 3. Partition Ownership State Machine

A partition owned by this consumer should go through these states:

```
[Assigned] ──> [Processing] ──> [Draining] ──> [Revoked]
                │                   │
                │                   └──> [Paused] (backpressure)
                │
                └──> [Paused] (backpressure) ──> [Processing]
```

**State definitions:**
- **Assigned**: Partition is in our assignment. We are responsible for committing offsets.
- **Processing**: Actively fetching and processing messages from this partition.
- **Draining**: Shutdown or rebalance signal received. No new messages fetched. In-flight being processed.
- **Paused**: Consumption paused via `rd_kafka_pause()`. Offset not advancing.
- **Revoked**: No longer in our assignment. Partition state should be cleared.

**KafPy's current state tracking:**
- `partition_handles` in `ConsumerDispatcher` tracks partition lists for pause/resume
- `paused_topics` tracks which topics are paused
- **Missing**: Per-partition state with explicit transitions, no rebalance integration

### 4. Safe Offset Progression During Shutdown/Rebalance

The key invariant: **never commit a gap**.

librdkafka's `consumer_lag_stored` field in statistics shows the difference between `hi_offset` (latest offset in Kafka) and `stored_offset` (offset to be committed). This is the safe zone.

**KafPy's existing protection:**
- `OffsetTracker` uses BTreeSet to track pending offsets
- `should_commit()` only returns true when `committed_offset + 1` is in `pending_offsets`
- `graceful_shutdown()` only commits partitions where `should_commit()` is true
- `has_terminal` flag blocks commit for a partition when terminal failure seen (D-01)

**Additional needed protection during rebalance:**
- When `REVOKE` event fires, must wait for in-flight to complete before committing
- Must flush all retryable failures to DLQ before revoking (same as shutdown)
- Partition that is draining should not accept new messages from the consumer

### 5. Common Patterns in Other Libraries

**Java Spring Kafka:**
```java
// Listener container lifecycle
container.stop(() -> {
    // Called on each listener thread — drain logic
    kafkaTemplate.sendToDLQ(records);
    committedOffsets.setOffset(topic, partition, offset);
});

// Rebalance listener
container.getContainerProperties().setConsumerRebalanceListener(
    new ConsumerRebalanceListener() {
        public void onPartitionsRevoked(Collection<TopicPartition> partitions) {
            // Commit offsets before revoke
        }
        public void onPartitionsAssigned(Collection<TopicPartition> partitions) {
            // Seek to committed offset
        }
    }
);
```

**Go segmentio/kafka-go:**
```go
// ConsumerGroup handling
type ConsumerGroupHandler interface {
    Setup(Session) error
    Cleanup(Session) error
    Consume(Session, Message) error
}

// OnRevoke: drain current assignments
func (h *handler) Cleanup(session kafka.GroupSession) error {
    for _, assignment := range session.Assignments() {
        // Commit offsets for each topic-partition
    }
    return nil
}
```

**Python confluent-kafka-python:**
```python
# Rebalance callback
def rebalance_callback(consumer, partitions, event):
    if event == KafkaRebalanceEvent.REVOKE:
        # Commit offsets before partitions revoked
        consumer.commit(partitions, async=False)
    elif event == KafkaRebalanceEvent.ASSIGN:
        # Seek to committed offset
        for p in partitions:
            consumer.seek(p)

# Configuration
conf['rebalance_cb'] = rebalance_callback
```

### 6. rdkafka Rebalance Callback Registration

From rdkafka `rdkafka.h`, the rebalance callback is configured via `rd_kafka_conf_set_rebalance_cb()` or in C++ via `RdKafka::Conf::set_rebalance_cb()`.

The callback signature:
```c
void rebalance_callback(rd_kafka_t *rk,
                        rd_kafka_resp_err_t err,
                        rd_kafka_topic_partition_list_t *partitions,
                        void *opaque);
```

The librdkafka `INTRODUCTION.md` notes on rebalance (section "Consumer groups"):
- Consumer group state flow is visualized in `src/librdkafka_cgrp_synch.png`
- Static group membership (KIP-345) retains partition ownership across restarts
- KIP-848 "next generation" protocol is incremental assign/unassign (preview in librdkafka 2.4+)

## MVP Recommendation

### Launch With (v1.8 — Shutdown & Rebalance)

**Must have for production safety:**

1. **`ConsumerRunner::close()` method** — calls `rd_kafka_consumer_close()` to leave group properly. Without this, consumer appears in group until `session.timeout.ms` expires.

2. **Coordinated shutdown sequence:**
   - `WorkerPool::shutdown()` signals drain
   - Wait for in-flight completion (with timeout)
   - Flush failed to DLQ
   - Commit final offsets
   - Call `ConsumerRunner::close()`
   - Cancel all background tasks

3. **Basic rebalance callback:**
   - `RD_KAFKA_EVENT_REBALANCE_REVOKE`: pause partition, drain in-flight, commit offsets, unregister partition
   - `RD_KAFKA_EVENT_REBALANCE_ASSIGN`: register partitions, populate partition handles

4. **Partition ownership state enum:**
   ```rust
   enum PartitionOwnership {
       Assigned,    // In our assignment
       Processing, // Actively consuming
       Paused,     // rd_kafka_pause() called
       Draining,   // Revoke/shutdown in progress
       Revoked,    // No longer ours
   }
   ```

### Add After Validation (v1.9 — Production Hardening)

- Drain timeout with graceful degradation
- Python-visible rebalance callbacks (opt-in)
- Incremental rebalance support (when Kafka 4.0+ widely deployed)
- Rebalance-induced DLQ flush

### Future Consideration (v2+)

- Static group membership fencing detection
- KIP-848 incremental rebalance protocol support
- Partition-level drain deadlines (per-partition timeout vs global)

## Sources

### Primary (HIGH confidence)
- [librdkafka INTRODUCTION.md](target/debug/build/rdkafka-sys-125b88b4db967997/out/INTRODUCTION.md) — termination sequence, consumer groups, static membership, KIP-848
- [librdkafka STATISTICS.md](target/debug/build/rdkafka-sys-125b88b4db967997/out/STATISTICS.md) — `consumer_lag_stored`, `fetch_state`, `app_offset`, `committed_offset`
- KafPy existing implementation: `src/consumer/runner.rs`, `src/worker_pool/mod.rs`, `src/coordinator/offset_tracker.rs`, `src/coordinator/offset_coordinator.rs`

### Secondary (MEDIUM confidence)
- [Spring Kafka ListenerContainer lifecycle](https://docs.spring.io/spring-kafka/reference/html/) — shutdown and rebalance patterns
- [confluent-kafka-python rebalance_cb](https://github.com/confluent/confluent-kafka-python) — Python binding patterns
- [segmentio/kafka-go ConsumerGroup](https://github.com/segmentio/kafka-go) — Go patterns

### Tertiary (LOW confidence)
- KIP-848 protocol details — still preview in librdkafka 2.4.0, may change

---

*Research for: graceful shutdown and rebalance handling (v1.8 candidate)*
*Researched: 2026-04-19*
