# Requirements — Milestone v1.4

**Goal:** Implement failure handling and DLQ routing. Rust owns failure classification, retry orchestration, and DLQ routing. Python holds business logic only.

---

## v1.4 Requirements

### Failure Classification

- [ ] **FAIL-01**: `FailureReason` enum — variants: `Retryable(RetryableKind)`, `Terminal(TerminalKind)`, `NonRetryable(NonRetryableKind)`
  - `RetryableKind`: `NetworkTimeout`, `BrokerUnavailable`, `TransientPartitionError`
  - `TerminalKind`: `PoisonMessage`, `DeserializationFailed`, `HandlerPanic`
  - `NonRetryableKind`: `ValidationError`, `BusinessLogicError`, `ConfigurationError`
- [ ] **FAIL-02**: `ExecutionResult::Error(FailureReason, ...)` and `ExecutionResult::Rejected(FailureReason, ...)` carry failure reason
- [ ] **FAIL-03**: `FailureClassifier` trait — `classify(&self, error: &PyErr, context: &ExecutionContext) -> FailureReason`
  - Default classifier: maps Python exceptions to FailureReason variants
  - Extensible: users can provide custom classifiers per handler
- [ ] **FAIL-04**: Structured logging of failure reason with context (topic, partition, offset, attempt count)

### RetryPolicy & Retry Scheduling

- [ ] **RETRY-01**: `RetryPolicy` struct — `max_attempts: usize`, `base_delay: Duration`, `max_delay: Duration`, `jitter_factor: f64`
  - Default: max_attempts=3, base_delay=100ms, max_delay=30s, jitter_factor=0.1
- [ ] **RETRY-02**: `RetrySchedule::next_delay(attempt: usize) -> Duration` — exponential backoff with jitter
  - Formula: `min(base_delay * 2^attempt, max_delay) * (1 - jitter_factor + rng::<0,1>() * jitter_factor * 2)`
- [ ] **RETRY-03**: Retried messages are NOT acknowledged — `record_ack` NOT called until final success
  - Retry attempts call `mark_failed` on the offset coordinator (tracks attempt count without advancing commit)
- [ ] **RETRY-04**: After `max_attempts` exceeded, message routes to DLQ — not retry queue
- [ ] **RETRY-05**: `ConsumerConfig` exposes `default_retry_policy: RetryPolicy`
- [ ] **RETRY-06**: `PythonHandler` stores per-handler `retry_policy: Option<RetryPolicy>` (overrides global)

### DLQ Routing

- [ ] **DLQ-01**: DLQ topic naming: `{original_topic}.DLQ` — partition preserved from original
- [ ] **DLQ-02**: `DlqMetadata` struct — `original_topic: String`, `original_partition: i32`, `original_offset: i64`, `failure_reason: FailureReason`, `attempt_count: u32`, `first_failure_timestamp: DateTime<Utc>`, `last_failure_timestamp: DateTime<Utc>`
- [ ] **DLQ-03**: `DlqRouter` trait — `route(&self, metadata: &DlqMetadata) -> TopicPartition`
  - Default implementation: `DlqRouter::default()` → `{topic}.DLQ`, partition preserved
- [ ] **DLQ-04**: On max_attempts exceeded OR non-retryable: produce to DLQ topic with metadata as headers
  - Headers: `x-kafpy-original-topic`, `x-kafpy-original-partition`, `x-kafpy-original-offset`, `x-kafpy-failure-reason`, `x-kafpy-attempt-count`
- [ ] **DLQ-05**: DLQ produce is fire-and-forget — does not block worker loop
- [ ] **DLQ-06**: Extensible design: `DlqRouter` can be overridden per handler or globally via `ConsumerConfig`

### Terminal Handling & Commit Gating

- [ ] **TERM-01**: `PartitionState` tracks `has_terminal: bool` — set when any message for that TP is terminal
- [ ] **TERM-02**: `OffsetTracker::should_commit` returns false if `has_terminal == true` for that topic-partition
  - Terminal messages block commit to prevent at-least-once delivery of terminal payloads
- [ ] **TERM-03**: `WorkerPool::shutdown` calls `flush_terminal_to_dlq()` before `graceful_shutdown`
- [ ] **TERM-04**: `graceful_shutdown` produces all pending terminal messages to DLQ before committing offsets

### Out of Scope

- Retry topic / exponential backoff via Kafka retry topics (requires Kafka transaction support) — deferred to v2.0
- DLQ message compaction / retention policy configuration — deferred to future
- DLQ consumption / replay utility — deferred to future

---

## Traceability

| Requirement | Phase |
|-------------|-------|
| FAIL-01 | Phase 17 |
| FAIL-02 | Phase 17 |
| FAIL-03 | Phase 17 |
| FAIL-04 | Phase 17 |
| RETRY-01 | Phase 18 |
| RETRY-02 | Phase 18 |
| RETRY-03 | Phase 18 |
| RETRY-04 | Phase 18 |
| RETRY-05 | Phase 18 |
| RETRY-06 | Phase 18 |
| DLQ-01 | Phase 19 |
| DLQ-02 | Phase 19 |
| DLQ-03 | Phase 19 |
| DLQ-04 | Phase 19 |
| DLQ-05 | Phase 19 |
| DLQ-06 | Phase 19 |
| TERM-01 | Phase 20 |
| TERM-02 | Phase 20 |
| TERM-03 | Phase 20 |
| TERM-04 | Phase 20 |

---

*Last updated: 2026-04-17*