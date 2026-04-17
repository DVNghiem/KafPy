# Phase 19: DLQ Routing - Research

**Researched:** 2026-04-17
**Domain:** Dead-letter queue routing for Kafka message processing failures
**Confidence:** HIGH

## Summary

Phase 19 implements DLQ routing for exhausted and non-retryable message failures. When a message fails with `max_attempts` exceeded (retryable) or a terminal/non-retryable failure, it should be produced to a DLQ topic with full metadata envelope. The architecture uses a `DlqRouter` trait for routing decisions, a `DlqMetadata` struct for envelope data, and fire-and-forget produce via `FutureProducer` with bounded delivery channel.

**Primary recommendation:** Implement `DlqMetadata` + `DlqRouter` trait first, then wire into `worker_loop` at the existing DLQ stub, then add fire-and-forget produce pipeline.

## User Constraints (from CONTEXT.md)

### Locked Decisions

- **D-01:** DLQ topic naming = `{dlq_topic_prefix}{original_topic}` (default `"dlq."`)
- **D-02:** `DlqMetadata` struct with fields: `original_topic`, `original_partition`, `original_offset`, `failure_reason`, `attempt_count`, `first_failure_timestamp`, `last_failure_timestamp`
- **D-03:** `RetryCoordinator::record_failure()` returns `(should_retry, should_dlq, delay)` — when `should_dlq=true`, worker calls `DlqRouter::route()`
- **D-04:** Fire-and-forget via `FutureProducer` with delivery callback, bounded queue (~100), drop on full
- **D-05:** `DlqRouter` trait with `route(&self, metadata: &DlqMetadata) -> TopicPartition`
- **D-06:** Extensible design via trait

### Deferred Ideas

- Per-handler DLQ topic routing — trait-based design allows this, not implemented in Phase 19
- DLQ replay mechanism — deferred to future

## Architectural Responsibility Map

| Capability | Primary Tier | Rationale |
|------------|--------------|-----------|
| DLQ metadata envelope construction | Rust / worker_pool | Worker loop has all context (topic/partition/offset/reason/timestamps) |
| DLQ routing decision | Rust / RetryCoordinator | Already tracks state, decides when retries exhausted |
| DLQ produce (fire-and-forget) | Rust / produce module | Async FutureProducer, non-blocking |
| DLQ topic name resolution | Rust / DlqRouter trait | Config-based routing logic |
| ConsumerConfig extension | Rust / config.rs | Add `dlq_topic_prefix` field |

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| rdkafka | current | `FutureProducer`, `FutureRecord`, headers | Already in use for PyProducer |
| tokio | current | async/await, mpsc bounded channel | Already in worker_pool |
| chrono | current | `DateTime<Utc>` for timestamps | Already used in timestamp handling |
| parking_lot | current | Mutex for state | Already in RetryCoordinator |

**No new dependencies required** — all needed types (`FutureProducer`, `OwnedHeaders`, `FutureRecord`) already in use via `src/produce.rs`.

## Architecture Patterns

### System Architecture Diagram

```
Message Processing Flow (failure path):
┌─────────────────────────────────────────────────────────────┐
│  worker_loop: handler.invoke() returns ExecutionResult::Error/Rejected
│                           │
│                           ▼
│              RetryCoordinator.record_failure(topic, partition, offset, reason)
│                           │
                    │         │ (should_retry, should_dlq, delay)
                    ▼         ▼
              [retryable]    [non-retryable or max_attempts exceeded]
                │                        │
                ▼                        ▼
         sleep + retry            should_dlq=true
                                       │
                                       ▼
                              DlqRouter::route(metadata)
                                       │
                                       ▼
                              ┌─────────────────┐
                              │ BoundedMpscQueue│ (~100 deep)
                              │  (fire-and-forget)│
                              └────────┬────────┘
                                       │
                                       ▼
                              FutureProducer::send_async()
                              (delivery callback logs only)
```

### Recommended Project Structure

```
src/
├── dlq/
│   ├── mod.rs           # re-exports
│   ├── metadata.rs      # DlqMetadata struct
│   ├── router.rs        # DlqRouter trait + DefaultDlqRouter
│   └── produce.rs       # DLQ produce helper (fire-and-forget)
├── worker_pool/mod.rs   # wire DLQ routing at existing stub
├── consumer/config.rs   # add dlq_topic_prefix field
└── coordinator/retry_coordinator.rs  # add should_dlq to record_failure return
```

### Pattern 1: DlqMetadata Envelope

```rust
// Source: derived from D-02 (CONTEXT.md) + owned_message.rs pattern
use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct DlqMetadata {
    pub original_topic: String,
    pub original_partition: i32,
    pub original_offset: i64,
    pub failure_reason: FailureReason,
    pub attempt_count: u32,
    pub first_failure_timestamp: DateTime<Utc>,
    pub last_failure_timestamp: DateTime<Utc>,
}
```

### Pattern 2: DlqRouter Trait

```rust
// Source: D-05 (CONTEXT.md)
use rdkafka::TopicPartition;

pub trait DlqRouter: Send + Sync {
    fn route(&self, metadata: &DlqMetadata) -> TopicPartition;
}

pub struct DefaultDlqRouter {
    dlq_topic_prefix: String,
}

impl DefaultDlqRouter {
    pub fn new(dlq_topic_prefix: String) -> Self {
        Self { dlq_topic_prefix }
    }
}

impl DlqRouter for DefaultDlqRouter {
    fn route(&self, metadata: &DlqMetadata) -> TopicPartition {
        let topic = format!("{}{}", self.dlq_topic_prefix, metadata.original_topic);
        TopicPartition::new(topic, metadata.original_partition)
    }
}
```

### Pattern 3: Fire-and-Forget Produce via Bounded Channel

```rust
// Source: produce.rs pattern + D-04 (CONTEXT.md)
// SharedDlqProducer wraps FutureProducer with bounded delivery channel
use tokio::sync::mpsc;

struct SharedDlqProducer {
    producer: Arc<FutureProducer>,
    delivery_tx: mpsc::Sender<DeliveryResult>,  // ~100 bounded
}

impl SharedDlqProducer {
    fn produce(&self, topic: &str, partition: i32, msg: &OwnedMessage, metadata: &DlqMetadata) {
        // Build headers from metadata
        let headers = build_dlq_headers(metadata);
        let record = FutureRecord::to(topic)
            .payload(msg.payload.as_deref().unwrap_or(&[]))
            .key(msg.key.as_deref())
            .partition(partition)
            .headers(headers);

        // Fire-and-forget: spawn send, don't await
        let producer = Arc::clone(&self.producer);
        tokio::spawn(async move {
            let _ = producer.send(record, Timeout::After(Duration::from_secs(5))).await;
            // delivery callback logs only — don't block
        });
    }
}
```

### Pattern 4: Headers from DlqMetadata

```rust
// Source: produce.rs header pattern
fn build_dlq_headers(metadata: &DlqMetadata) -> OwnedHeaders {
    let mut headers = OwnedHeaders::new();
    headers = headers.insert(Header {
        key: "dlq.original_topic",
        value: Some(metadata.original_topic.as_bytes()),
    });
    headers = headers.insert(Header {
        key: "dlq.partition",
        value: Some(metadata.original_partition.to_string().as_bytes()),
    });
    headers = headers.insert(Header {
        key: "dlq.offset",
        value: Some(metadata.original_offset.to_string().as_bytes()),
    });
    headers = headers.insert(Header {
        key: "dlq.reason",
        value: Some(metadata.failure_reason.to_string().as_bytes()),
    });
    headers = headers.insert(Header {
        key: "dlq.attempts",
        value: Some(metadata.attempt_count.to_string().as_bytes()),
    });
    // ISO8601 timestamps
    headers = headers.insert(Header {
        key: "dlq.first_failure",
        value: Some(metadata.first_failure_timestamp.to_rfc3339().as_bytes()),
    });
    headers = headers.insert(Header {
        key: "dlq.last_failure",
        value: Some(metadata.last_failure_timestamp.to_rfc3339().as_bytes()),
    });
    headers
}
```

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Kafka produce | raw sockets | `FutureProducer` from rdkafka | Already in use, thread-safe |
| Header encoding | manual byte packing | `rdkafka::message::OwnedHeaders` | Already in produce.rs |
| Async DLQ channel | custom queue | `tokio::sync::mpsc::bounded(100)` | Standard pattern |
| Timestamp formatting | manual strings | `chrono::DateTime::to_rfc3339()` | Standard Rust approach |

## Runtime State Inventory

> Skip — Phase 19 is new feature implementation, not rename/refactor/migration.

## Common Pitfalls

### Pitfall 1: Blocking on DLQ Produce in Worker Loop
**What goes wrong:** If `FutureProducer::send` is awaited directly, slow broker causes worker starvation.
**Why it happens:** DLQ path is in the hot path of `worker_loop`.
**How to avoid:** Always `tokio::spawn` the produce future without awaiting, use bounded channel for backpressure.

### Pitfall 2: Losing Original Message Payload on DLQ
**What goes wrong:** `OwnedMessage` may not own payload if it was sliced.
**Why it happens:** rdkafka `BorrowedMessage` can slice payloads.
**How to avoid:** `OwnedMessage::from_borrowed` already copies payload/key — verified in `src/consumer/message.rs:62-85`.

### Pitfall 3: Timestamp Precision Loss
**What goes wrong:** `Instant` vs `DateTime<Utc>` mismatch when constructing `DlqMetadata`.
**Why it happens:** `RetryCoordinator` uses `std::time::Instant` internally, not `DateTime<Utc>`.
**How to avoid:** Store `DateTime<Utc>` from `chrono::Utc::now()` at first failure, convert `Instant` to `SystemTime` for final timestamp.

### Pitfall 4: Missing `Send + Sync` on DlqRouter
**What goes wrong:** `DlqRouter` used across multiple tokio tasks.
**Why it happens:** Trait objects require explicit bounds.
**How to avoid:** `trait DlqRouter: Send + Sync` — already specified in D-05.

## Code Examples

### Worker Loop DLQ Stub (current — to be replaced)

From `src/worker_pool/mod.rs:106-116`:
```rust
// Max attempts exceeded or non-retryable → route to DLQ
tracing::error!(
    worker_id = worker_id,
    topic = %ctx.topic,
    partition = ctx.partition,
    offset = ctx.offset,
    reason = %reason,
    "max attempts exceeded — DLQ routing not implemented in this phase"
);
// DLQ routing happens in Phase 19 — for now just log
queue_manager.ack(&msg.topic, 1);
```

### RetryCoordinator.record_failure Return Type (current)

From `src/coordinator/retry_coordinator.rs:64-106`:
```rust
pub fn record_failure(...) -> (bool, Option<Duration>) {
    // Returns (should_retry, delay)
    // Currently: non-retryable category → (false, None) — caller must infer DLQ
    // Phase 19: change to (should_retry, should_dlq, delay)
}
```

### OwnedHeaders Pattern

From `src/produce.rs:86-95`:
```rust
let mut kafka_headers = rdkafka::message::OwnedHeaders::new();
for (key, value) in hdrs {
    kafka_headers = kafka_headers.insert(rdkafka::message::Header {
        key: &key,
        value: Some(&value),
    });
}
record = record.headers(kafka_headers);
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Log-only DLQ stub | Full DLQ produce with metadata | Phase 19 | Messages preserved for replay |
| Blocking produce | Fire-and-forget via channel | Phase 19 | Worker throughput maintained |
| No metadata | Structured envelope headers | Phase 19 | Debugging/replay possible |

**Deprecated/outdated:** None in scope.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `rdkafka::TopicPartition` is constructable via `new(topic, partition)` — [VERIFIED: rdkafka API confirms `TopicPartition::new`] | DlqRouter | Low |
| A2 | `chrono::DateTime<Utc>::now()` is available — [VERIFIED: chrono is current dependency] | DlqMetadata | Low |
| A3 | Bounded channel of 100 is sufficient — [ASSUMED: based on D-04 spec] | DLQ produce | Medium — may need tuning |

## Open Questions

1. **How to handle DLQ produce creation at startup?**
   - What we know: `PyProducer::init()` creates FutureProducer from `ProducerConfig`
   - What's unclear: Should `Consumer` own a `FutureProducer` for DLQ, or create one lazily?
   - Recommendation: Create lazy `Arc<FutureProducer>` in `Consumer` or `WorkerPool`, shared across workers

2. **Should DLQ produce use same `ProducerConfig` as user-provided producer?**
   - What we know: `ConsumerConfig` has broker info, security settings
   - What's unclear: Do DLQ messages need different acks/retries config?
   - Recommendation: Use same config with modified `queue.buffering.max.messages` for fire-and-forget

## Environment Availability

> Step 2.6: SKIPPED — no external dependencies beyond project code.

**All required dependencies already in project:**
- `rdkafka` with `FutureProducer` — via existing `src/produce.rs`
- `chrono` for timestamps — [ASSUMED: verify in Cargo.toml]
- `tokio` with mpsc — via existing `worker_pool/mod.rs`

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | `#[tokio::test]` + `#[test]` (existing) |
| Config file | `Cargo.toml` test profile |
| Quick run command | `cargo test --lib` |
| Full suite command | `cargo test` |

### Phase Requirements -> Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|--------------|
| DLQ-01 | DLQ topic naming with prefix | unit | `cargo test dlq` | New file |
| DLQ-02 | DlqMetadata fields populated correctly | unit | `cargo test dlq` | New file |
| DLQ-03 | DlqRouter trait + DefaultDlqRouter | unit | `cargo test dlq` | New file |
| DLQ-04 | Headers attached to DLQ message | unit | `cargo test dlq` | New file |
| DLQ-05 | Fire-and-forget (non-blocking produce) | unit | `cargo test dlq` | New file |
| DLQ-06 | DlqRouter extensible | unit | `cargo test dlq` | New file |

### Wave 0 Gaps

- [ ] `src/dlq/mod.rs` — module re-exports
- [ ] `src/dlq/metadata.rs` — DlqMetadata struct
- [ ] `src/dlq/router.rs` — DlqRouter trait + DefaultDlqRouter
- [ ] `src/dlq/produce.rs` — DLQ produce helper
- [ ] Tests: `src/dlq/tests.rs` or inline `#[cfg(test)]` modules

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V4 Access Control | no | Not applicable |
| V5 Input Validation | yes | Validate DLQ metadata field lengths (topic names, offsets) |
| V6 Cryptography | no | No secrets in DLQ path |

### Known Threat Patterns for DLQ

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| DLQ topic injection | Tampering | Topic name derived from original, not user input |
| Large header values | Denial of Service | Validate metadata field lengths at envelope creation |

## Sources

### Primary (HIGH confidence)
- `src/worker_pool/mod.rs` — worker_loop DLQ stub location
- `src/coordinator/retry_coordinator.rs` — record_failure pattern
- `src/produce.rs` — FutureProducer + OwnedHeaders pattern
- `src/consumer/message.rs` — OwnedMessage structure
- `src/failure/reason.rs` — FailureReason enum
- `src/consumer/config.rs` — ConsumerConfig pattern

### Secondary (MEDIUM confidence)
- Context7 `/fede1024/rust-rdkafka` — rdkafka API surface

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all libraries already in use
- Architecture: HIGH — well-specified in CONTEXT.md
- Pitfalls: MEDIUM — some assumptions about channel sizing

**Research date:** 2026-04-17
**Valid until:** 2026-05-17 (30 days — stable domain)