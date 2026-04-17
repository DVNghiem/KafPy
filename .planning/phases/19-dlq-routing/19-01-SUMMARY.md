---
phase: 19-dlq-routing
plan: 01
subsystem: dlq
tags: [dlq, retry, infrastructure]
dependency_graph:
  requires: []
  provides:
    - DlqMetadata struct
    - DlqRouter trait
    - DefaultDlqRouter
    - ConsumerConfig.dlq_topic_prefix
    - RetryCoordinator::record_failure 3-tuple return
  affects:
    - src/worker_pool/mod.rs
    - src/coordinator/retry_coordinator.rs
tech_stack:
  added:
    - chrono 0.4 (serde feature) for DateTime<Utc>
    - serde 1.0 (derive feature) for Serialize/Deserialize
  patterns:
    - Builder pattern for ConsumerConfig
    - Trait-based extensibility (DlqRouter)
    - State machine return tuple
key_files:
  created:
    - src/dlq/mod.rs
    - src/dlq/metadata.rs
    - src/dlq/router.rs
  modified:
    - Cargo.toml
    - src/consumer/config.rs
    - src/coordinator/retry_coordinator.rs
    - src/lib.rs
    - src/worker_pool/mod.rs
decisions:
  - "DlqRouter TopicPartition is a local struct (not rdkafka's) since rdkafka only exports TopicPartitionList"
  - "serde added for DlqMetadata serialization (for header encoding in future phases)"
  - "Instant replaced with DateTime<Utc> for first_failure in MessageRetryState per D-02"
metrics:
  duration: "~5 min"
  completed: 2026-04-17
  tasks_completed: 5
---

# Phase 19 Plan 01: DLQ Infrastructure Summary

## One-liner

Core DLQ infrastructure: `DlqMetadata` envelope, `DlqRouter` trait, `ConsumerConfig` prefix field, and `RetryCoordinator::record_failure` returning 3-tuple `(should_retry, should_dlq, delay)`.

## What Was Built

### DlqMetadata (`src/dlq/metadata.rs`)
Structured envelope with fields from D-02: `original_topic`, `original_partition`, `original_offset`, `failure_reason`, `attempt_count`, `first_failure_timestamp`, `last_failure_timestamp`. Uses `DateTime<Utc>` for timestamps and `serde` for serialization.

### DlqRouter Trait (`src/dlq/router.rs`)
Trait `DlqRouter: Send + Sync` with `route(&self, metadata: &DlqMetadata) -> TopicPartition`. Includes `DefaultDlqRouter` producing `{prefix}{original_topic}` with partition preserved. Custom routers can override routing logic (e.g., per-handler DLQ topics in future).

### ConsumerConfig Update (`src/consumer/config.rs`)
Added `dlq_topic_prefix: String` field with default `"dlq."`. Builder method `dlq_topic_prefix()` allows configuration. DLQ topic for `"events"` with default prefix is `"dlq.events"`.

### RetryCoordinator Return Type (`src/coordinator/retry_coordinator.rs`)
`record_failure` now returns `(bool, bool, Option<Duration>)` = `(should_retry, should_dlq, delay)`:
- Terminal/NonRetryable: `(false, true, None)` — immediate DLQ
- Max attempts exceeded: `(false, true, None)` — DLQ
- Retryable within limit: `(true, false, Some(delay))` — schedule retry

`MessageRetryState.first_failure` changed from `Instant` to `DateTime<Utc>`.

### WorkerPool Integration (`src/worker_pool/mod.rs`)
Updated to destructure 3-tuple from `record_failure`. `should_dlq` variable is captured for future DLQ routing (Phase 19-02).

## Verification

| Check | Result |
|-------|--------|
| `cargo build --lib` | PASS (20 warnings, all pre-existing) |
| `cargo check --lib` | PASS |
| `chrono` in Cargo.toml | Present |
| `serde` in Cargo.toml | Present |
| `DlqMetadata` fields | All 7 fields from D-02 |
| `DlqRouter::route` signature | `-> TopicPartition` |
| `record_failure` return | 3-tuple verified in tests |

## Deviations from Plan

1. **[Rule 3 - Blocking Issue] rdkafka TopicPartition not exported**: rdkafka only exports `TopicPartitionList`, not `TopicPartition`. Created a local `TopicPartition` struct in `src/dlq/router.rs` instead. DLQ routing will use this local type; future phases can convert to rdkafka's `TopicPartitionList` when producing.

2. **[Rule 2 - Auto-add missing dependency] Added serde**: DlqMetadata uses `Serialize/Deserialize` for header encoding in future DLQ produce phase. serde was not in plan but is required for correctness.

3. **[Rule 1 - Bug] Instant -> DateTime<Utc>**: `MessageRetryState.first_failure` was `Instant` but D-02 specifies UTC timestamps. Fixed to `DateTime<Utc>` and updated timestamp capture to `Utc::now()`.

## Known Stubs

| Stub | File | Line | Reason |
|------|------|------|--------|
| `should_dlq` variable unused | src/worker_pool/mod.rs | 81, 132 | Phase 19-02 implements actual DLQ routing |

## Commits

| Hash | Message |
|------|---------|
| 9aadf2f | feat(phase-19): add core DLQ infrastructure |

## TDD Gate Compliance

Not a TDD plan — standard execution.

## Self-Check

- [x] All files created exist at correct paths
- [x] Commit hash 9aadf2f found in git log
- [x] chrono + serde deps added to Cargo.toml
- [x] DlqMetadata has all 7 fields
- [x] DlqRouter trait + DefaultDlqRouter implemented
- [x] ConsumerConfig dlq_topic_prefix field present
- [x] record_failure returns 3-tuple in coordinator and worker_pool
- [x] cargo check passes

**Self-Check: PASSED**
