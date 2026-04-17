# Phase 19: DLQ Routing - Context

**Gathered:** 2026-04-17
**Status:** Ready for planning

<domain>
## Phase Boundary

Routing exhausted/non-retryable messages to a dead-letter queue with metadata envelope. After max_attempts exceeded OR non-retryable failure → produce to DLQ topic with metadata headers.
</domain>

<decisions>
## Implementation Decisions

### D-01: DLQ Topic Naming — Configurable
- `ConsumerConfig` has `dlq_topic_prefix: String` field
- DLQ topic = `{dlq_topic_prefix}{original_topic}` (default: `"dlq."`)
- Example: `"dlq.my-topic"` for topic `my-topic`
- **Rationale**: Companies often restrict Kafka topic creation — admins pre-provision DLQ topics, so naming must be configurable

### D-02: DLQ Metadata Envelope
- `DlqMetadata` struct fields: `original_topic`, `original_partition`, `original_offset`, `failure_reason`, `attempt_count`, `first_failure_timestamp`, `last_failure_timestamp`
- Stored as Kafka message headers: `dlq.original_topic`, `dlq.partition`, `dlq.offset`, `dlq.reason`, `dlq.attempts`, `dlq.first_failure`, `dlq.last_failure`

### D-03: Routing Integration
- `RetryCoordinator::record_failure()` returns `(should_retry: bool, should_dlq: bool, delay: Option<Duration>)`
- When `should_dlq=true`, worker_loop calls `DlqRouter::route()` to get target TopicPartition
- `DlqRouter` is a trait: `trait DlqRouter { fn route(&self, metadata: &DlqMetadata) -> TopicPartition; }`
- Default implementation: uses `dlq_topic_prefix` from ConsumerConfig
- **Rationale**: RetryCoordinator already tracks state — it makes the DLQ routing decision. WorkerPool orchestrates the action.

### D-04: Produce Behavior — Fire-and-forget
- Use async `FutureProducer` with delivery callback (non-blocking)
- Worker loop does NOT await DLQ produce result
- Bounded queue (~100 messages) between worker and producer
- If queue full: log warning, drop message (don't block main consumer)
- Delivery callback logs success/failure for observability only
- **Rationale**: DLQ messages are already "dead" — failing to produce to DLQ shouldn't block the main pipeline

### D-05: DlqRouter Trait
```rust
pub trait DlqRouter: Send + Sync {
    fn route(&self, metadata: &DlqMetadata) -> TopicPartition;
}
```
- `DefaultDlqRouter`: uses `ConsumerConfig::dlq_topic_prefix`, preserves partition
- `CustomDlqRouter` (future): per-handler DLQ topic routing

### D-06: Extensible Design
- `DlqRouter` trait allows custom routing (per-handler DLQ topics, custom metadata)
- Future: dead-letter topic per handler type

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase 18 Artifacts
- `src/coordinator/retry_coordinator.rs` — RetryCoordinator pattern (what this phase extends)
- `src/worker_pool/mod.rs` — worker_loop stubs with "DLQ routing not implemented"
- `src/consumer/config.rs` — ConsumerConfig pattern (add dlq_topic_prefix field)

### Phase 17 Artifacts
- `src/failure/reason.rs` — FailureReason enum and FailureCategory
- `src/python/handler.rs` — PythonHandler pattern (per-handler override support)

### Related
- `src/retry/policy.rs` — RetryPolicy (used to get max_attempts)
</canonical_refs>

<specifics>
## Specific Ideas

- DLQ produce uses `rdkafka::producer::FutureProducer` with async callback
- Headers format: string values, ISO8601 timestamps
- Partition: preserved from original message
</specifics>

<deferred>
## Deferred Ideas

- Per-handler DLQ topic routing — trait-based design allows this, not implemented in Phase 19
- DLQ replay mechanism — re-consume from DLQ topic with original handler
</deferred>

---

*Phase: 19-dlq-routing*
*Context gathered: 2026-04-17*
