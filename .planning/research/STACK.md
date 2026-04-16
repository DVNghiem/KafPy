# Stack Research: Offset Commit Coordinator

**Domain:** PyO3 Rust Kafka client — per-topic-partition highest-contiguous-offset commit coordination
**Researched:** 2026-04-16
**Confidence:** MEDIUM (rdkafka 0.38 API verified via Context7, but offset-store async patterns lack published code examples)

## Executive Summary

The v1.3 offset commit coordinator requires a new `OffsetTracker` data structure that tracks per-topic-partition acknowledged offsets and computes the highest contiguous offset for commit. The key rdkafka APIs are `store_offset()` (per-message, async-safe) and `commit_consumer_state()` (batch commit). The existing `ConsumerRunner::commit()` method already wraps the latter — the new component integrates between `ExecutionResult::Ok` (from WorkerPool) and rdkafka's offset storage layer.

No new crate dependencies are needed. The existing stack (rdkafka 0.38, Tokio, parking_lot) is sufficient.

## Recommended Stack

### Core Technologies (Already Present)

| Technology | Version | Purpose | Why |
|------------|---------|---------|-----|
| rdkafka | 0.38 | Kafka protocol, `store_offset`, `commit` | Current version, verified via Context7 |
| tokio | 1.40 | Async runtime, channels | Existing stack |
| parking_lot | 0.12 | Fast mutex for tracker map | Existing stack, no poison semantics |
| thiserror | 2.0 | Error types | Existing stack |

### New Module: OffsetTracker

```
src/offset/
  mod.rs
  tracker.rs       # OffsetTracker struct + TopicPartition tracking
  commit.rs        # CommitCoordinator (store_offset + commit coordination)
```

## rdkafka Offset API Deep Dive

### Two-Phase Offset Model

rdkafka uses a two-phase model for manual offset management:

1. **Store phase** — `consumer.store_offset()` records the offset you have processed in memory. This is fast, local, non-blocking.
2. **Commit phase** — `consumer.commit_consumer_state()` flushes stored offsets to the broker. This is a network call.

```
Message received (offset N)
    → worker.process()
    → ExecutionResult::Ok
    → offset_tracker.ack(topic, partition, N)   # store phase (in-memory)
    → offset_tracker.commit_ready()              # check highest contiguous
    → consumer.store_offset(topic, partition, N) # persist to librdkafka
    → consumer.commit_consumer_state()          # flush to broker
```

### store_offset() vs commit_message()

| API | Behavior | Use Case |
|-----|---------|----------|
| `consumer.store_offset(topic, partition, offset)` | Stores offset in librdkafka's internal map. Does NOT send to broker. Returns `()`, async-safe. | Called on every processed message before ack is returned |
| `consumer.commit_consumer_state()` | Sends all stored offsets to broker in one request. | Periodic batch commit (e.g., every N offsets or T timeout) |
| `consumer.commit_message(msg, CommitMode)` | Stores AND commits a single message's offset immediately. | Synchronous per-message commit (higher latency) |

**Key insight for v1.3:**
- Use `store_offset()` after each `ExecutionResult::Ok` to record processed position
- Use `commit_consumer_state()` periodically (batch) rather than per-message
- This gives at-least-once delivery: if process crashes after store_offset but before commit_consumer_state, librdkafka re-delivers from the stored position on restart

### Configuration Required

```rust
// ConsumerConfig builder must set:
.enable.auto.commit(false)           // Disable automatic commit
.set("enable.auto.offset.store", "false")  // Disable auto-store (we do it manually)
// Note: rdkafka default is enable.auto.offset.store=true (auto-store on consume)
// We need false so our store_offset() is the authoritative trigger
```

**Why disable auto-store:**
- rdkafka's default `enable.auto.offset.store=true` auto-stores offset on every `recv()`
- This means librdkafka already tracks consumed offsets internally
- With manual mode, we control exactly when `store_offset()` is called (after successful processing)
- This allows out-of-order processing: we ack offset 10 before offset 9, but only commit 9 once 8 is also acked

### Async Safety

`store_offset()` is async-safe — it only writes to an in-memory HashMap inside librdkafka. No network I/O, no blocking. Safe to call from any Tokio task.

`commit_consumer_state()` is also async-safe (wraps `rd_kafka_commit` which has an async variant in librdkafka). The existing `ConsumerRunner::commit()` already wraps this.

## OffsetTracker Data Structure

### Design: Per-Topic-Partition Tracking with Highest-Contiguous-Offset Logic

```rust
use std::collections::HashMap;
use parking_lot::Mutex;
use std::sync::Arc;

/// Tracks acknowledged offsets for a single topic-partition.
/// Computes highest contiguous offset — the position to commit.
pub struct TopicPartitionOffset {
    /// All acknowledged offsets (may have gaps due to out-of-order ack).
    /// Using i64 offset as key for O(1) lookup.
    acked: std::collections::HashSet<i64>,
    /// Highest contiguous offset that can be committed.
    /// This equals the lowest offset in a contiguous sequence from 0.
    /// Recalculated on every ack.
    highest_contiguous: i64,
    /// Last committed offset (to avoid duplicate commits).
    last_committed: i64,
}

impl TopicPartitionOffset {
    /// Record an ack for `offset`. Recalculates highest_contiguous.
    pub fn ack(&mut self, offset: i64) {
        self.acked.insert(offset);
        self.recalculate_highest_contiguous();
    }

    /// Returns the highest contiguous offset to commit, or None if no advance.
    pub fn commit_ready(&self) -> Option<i64> {
        let candidate = self.highest_contiguous;
        if candidate > self.last_committed {
            Some(candidate)
        } else {
            None
        }
    }

    /// Mark `offset` as committed to avoid duplicate commits.
    pub fn committed(&mut self, offset: i64) {
        self.last_committed = offset;
        // Also clean up acked set below committed offset to prevent unbounded growth
        self.acked.retain(|&o| o > offset);
        self.highest_contiguous = offset + 1;
    }

    fn recalculate_highest_contiguous(&mut self) {
        // Find the lowest offset that has a gap (missing offset)
        // Start from last_committed + 1 and scan forward
        let mut candidate = self.last_committed + 1;
        while self.acked.contains(&candidate) {
            candidate += 1;
        }
        self.highest_contiguous = candidate;
    }
}
```

**Why HashSet for acked:**
- O(1) insert and lookup
- Memory is bounded (only uncommitted offsets are stored)
- Cleanup on commit (`retain` only offsets > committed) prevents unbounded growth

**Why recalculate on every ack:**
- Out-of-order completion means offset 10 can be acked before offset 9
- `highest_contiguous` finds the first gap in the sequence starting from `last_committed + 1`
- If acked = {5, 6, 8, 9, 10} and last_committed = 4, then highest_contiguous = 7 (gap at 7)

### OffsetTracker: Multi-Topic-Partition Manager

```rust
pub struct OffsetTracker {
    partitions: parking_lot::Mutex<HashMap<TopicPartitionKey, TopicPartitionOffset>>,
}

#[derive(Clone, Hash, Eq, PartialEq)]
struct TopicPartitionKey {
    topic: String,
    partition: i32,
}

impl OffsetTracker {
    /// Record ack for (topic, partition, offset).
    pub fn ack(&self, topic: &str, partition: i32, offset: i64) {
        let mut guard = self.partitions.lock();
        let tp = guard.entry(TopicPartitionKey::new(topic, partition))
            .or_insert_with(|| TopicPartitionOffset::new());
        tp.ack(offset);
    }

    /// Returns offsets ready to commit: Map<TopicPartitionKey, i64>
    /// Call this before store_offset + commit_consumer_state.
    pub fn commit_ready(&self) -> HashMap<TopicPartitionKey, i64> {
        let guard = self.partitions.lock();
        guard.iter()
            .filter_map(|(k, v)| v.commit_ready().map(|offset| (k.clone(), offset)))
            .collect()
    }

    /// Mark offsets as committed. Call this after successful commit_consumer_state.
    pub fn committed(&self, topic: &str, partition: i32, offset: i64) {
        let mut guard = self.partitions.lock();
        if let Some(tp) = guard.get_mut(&TopicPartitionKey::new(topic, partition)) {
            tp.committed(offset);
        }
    }
}
```

### Integration with Existing ConsumerRunner

The `OffsetTracker` is owned by a new `CommitCoordinator` that sits between `ExecutionResult::Ok` events and `ConsumerRunner::commit()`:

```rust
pub struct CommitCoordinator {
    tracker: Arc<OffsetTracker>,
    consumer: Arc<StreamConsumer>,  // from ConsumerRunner
    // Config: commit batch size and interval
    commit_batch_size: usize,
    commit_interval: Duration,
}

impl CommitCoordinator {
    /// Called from WorkerPool on ExecutionResult::Ok.
    pub fn on_message_acked(&self, topic: &str, partition: i32, offset: i64) {
        // 1. Ack in tracker
        self.tracker.ack(topic, partition, offset);

        // 2. Check if we should store_offset
        if let Some(commit_offset) = self.tracker.commit_ready(topic, partition) {
            // 3. Store in rdkafka (fast, async-safe)
            self.consumer.store_offset(topic, partition, commit_offset);
        }
    }

    /// Called periodically (timer or batch count).
    /// Returns offsets that were committed.
    pub fn flush_commit(&self) -> Result<Vec<(String, i32, i64)>, ConsumerError> {
        let ready = self.tracker.commit_ready_all();
        if ready.is_empty() {
            return Ok(vec![]);
        }
        // store_offset for all ready (already done in on_message_acked for incremental)
        // Commit all to broker
        self.consumer.commit_consumer_state(rdkafka::consumer::CommitMode::Async)?;
        // Mark as committed
        for (topic, partition, offset) in &ready {
            self.tracker.committed(topic, partition, *offset);
        }
        Ok(ready)
    }
}
```

## Integration with Existing Modules

### Integration Point 1: WorkerPool worker_loop

In `worker_loop` (currently in `src/worker_pool/mod.rs`), replace:

```rust
// Current:
ExecutionResult::Ok => {
    queue_manager.ack(&msg.topic, 1);
}
```

With:

```rust
// New:
ExecutionResult::Ok => {
    queue_manager.ack(&msg.topic, 1);
    // Wire execution completion to offset tracker
    if let Some(coordinator) = self.commit_coordinator.as_ref() {
        coordinator.on_message_acked(&msg.topic, msg.partition, msg.offset);
    }
}
```

### Integration Point 2: ConsumerRunner

`ConsumerRunner::commit()` already exists. The new `CommitCoordinator` wraps the same consumer and calls `commit_consumer_state()` on a timer/batch schedule.

### Integration Point 3: ConsumerConfig

Add `enable_auto_commit(false)` and `enable_auto_offset_store(false)` to `ConsumerConfig::build_rdkafka_config()`.

## Async-Safe Patterns

### store_offset: Always Safe

`StreamConsumer::store_offset()` in rdkafka writes to an internal `HashMap<TopicPartition, i64>` protected by librdkafka's own mutex. This is thread-safe and async-safe.

**No additional synchronization needed** between WorkerPool workers calling `store_offset()` concurrently — librdkafka handles it internally.

### commit_consumer_state: Async-Safe

`commit_consumer_state(CommitMode::Async)` is also async-safe. It spawns a background task in librdkafka's internal thread pool.

### Race Condition Prevention

**Scenario:** Worker A acks offset 10, Worker B acks offset 11, both call `store_offset()` concurrently.

**Prevents double-commit via `last_committed` guard:**
```rust
pub fn commit_ready(&self) -> Option<i64> {
    let candidate = self.highest_contiguous;
    if candidate > self.last_committed {  // Only advance if new
        Some(candidate)
    } else {
        None
    }
}
```

**Scenario:** `flush_commit()` calls `commit_consumer_state()` while WorkerPool is calling `store_offset()`.

**No race:** `store_offset()` only updates the in-memory map. `commit_consumer_state()` reads from the same map and sends to broker. librdkafka's internal map is the source of truth and is protected by its own locking.

## Alternatives Considered

| Approach | Why Not | When Better |
|----------|---------|-------------|
| Per-message `commit_message()` | Synchronous, higher latency, blocks worker | When strict ordering required and latency tolerance is low |
| rdkafka's built-in `enable.auto.commit=true` | No control over highest-contiguous, commits all acked | When at-least-once is acceptable and simplicity preferred |
| Async commit via `commit_consumer_state(Async)` vs `Sync` | Already using Async in `ConsumerRunner::commit()` | N/A — already chosen |
| Use `parking_lot::RwLock` for tracker | Unnecessary — all operations are fast, no read-heavy/read-write distinction | When reader contention becomes an issue at high throughput |

## What NOT to Use

| Pattern | Avoid Because |
|---------|----------------|
| `enable.auto.commit=true` | Commits automatically on consume, bypassing our ack tracking — defeats highest-contiguous logic |
| `enable.auto.offset.store=true` | rdkafka auto-stores on every `recv()`, bypassing our store_offset control |
| Per-message `commit_message` | Synchronous, network round-trip per message, high latency |
| `CommitMode::Sync` for batch commit | Blocks Tokio thread — use `Async` always |
| `std::sync::Mutex` over `parking_lot::Mutex` | Poison semantics complicate error handling, slower uncontended acquisition |

## Version Compatibility

| Package | Version | Compatible With | Notes |
|---------|---------|-----------------|-------|
| rdkafka | 0.38 | Tokio 1.40, pyo3 0.27.2 | Current version, verified via `cargo search` |
| rdkafka-sys | (bundled) | librdkafka 2.6.x | Bundled with rdkafka 0.38 |
| parking_lot | 0.12 | rdkafka 0.38 | No conflicts |

## Sources

- Context7: `/fede1024/rust-rdkafka` — `store_offset` and `commit_consumer_state` API verified
- Context7: `/fede1024/rust-rdkafka` — `StreamConsumer` API verified (already in use in `src/consumer/runner.rs`)
- `src/consumer/runner.rs:116-121` — existing `commit()` implementation using `CommitMode::Async`
- `src/consumer/config.rs:227` — existing `enable_auto_commit` config
- `Cargo.toml:29` — `rdkafka = { version = "0.38" }` confirmed as current version
- `src/worker_pool/mod.rs:64` — `queue_manager.ack()` integration pattern
- `src/python/executor.rs:72` — `OffsetAck` placeholder trait marker

---
*Stack research for: Offset Commit Coordinator v1.3*
*Researched: 2026-04-16*