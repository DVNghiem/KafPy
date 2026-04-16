# Feature Research: Offset Commit Coordinator (v1.3)

**Domain:** Kafka at-least-once delivery via per-topic-partition highest-contiguous-offset tracking
**Researched:** 2026-04-16
**Confidence:** MEDIUM — Kafka offset semantics are well-documented in Kafka protocol docs; rdkafka 0.38 API verified via existing codebase patterns; out-of-order buffering logic is standard ecosystem practice

---

## What This Feature Is

Adding a coordinator that tracks which offsets have been acknowledged per topic-partition, computes the highest contiguous acknowledged offset, and periodically calls `store_offset()` + `commit()` to enable at-least-once delivery semantics.

Currently, `WorkerPool` calls `queue_manager.ack()` after Python execution, but this only decrements the inflight counter. It does not store or commit offsets to Kafka. The offset coordinator closes that gap.

---

## 1. Table Stakes (Users Expect These)

Features users assume exist. Missing these = product feels broken for production use.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| **Per-topic-partition ack tracking** | Multiple partitions and topics can be consumed simultaneously; each needs independent offset state | MEDIUM | `OffsetTracker` keyed by `(topic, partition)` |
| **Highest contiguous offset calculation** | Kafka can only re-deliver if the committed offset represents the last successfully processed message; gaps prevent re-delivery | MEDIUM | Buffer non-sequential acks, advance cursor when gap fills |
| **Out-of-order ack buffering** | WorkerPool processes messages concurrently; ack order is non-deterministic | MEDIUM | Store pending acks in a per-partition `BTreeSet<i64>`, advance cursor |
| **store_offset() before commit()** | `store_offset()` records the offset in rdkafka's internal state; `commit()` flushes it to Kafka | LOW | Already used in `ConsumerRunner::commit()` pattern; this adds per-message granularity |
| **Commit on interval or count threshold** | Committing every single message is expensive; batching amortizes cost | LOW | Configurable `commit_interval_ms` or `commit_max_messages` |
| **No duplicate commit when offset unchanged** | Calling `commit()` repeatedly with the same offset wastes network round-trips | LOW | Guard: only call `commit()` if stored offset has advanced |
| **Safe commit conditions** | Never commit beyond the highest contiguous offset (would cause message loss) | HIGH | This is the core correctness guarantee |

### rdkafka Offset API (Verified from Existing Codebase)

From `src/consumer/runner.rs:113-122`:
```rust
pub fn commit(&self) -> Result<(), ConsumerError> {
    self.consumer
        .commit_consumer_state(rdkafka::consumer::CommitMode::Async)
        .map_err(ConsumerError::from)?;
    debug!("Offset committed");
    Ok(())
}
```

rdkafka 0.38 exposes `store_offset` via the `Consumer` trait. The pattern for per-message offset storage:
```rust
// Store offset for a specific topic-partition (used before commit)
consumer.store_offset(topic, partition, offset)?;
// Then commit
consumer.commit_consumer_state(CommitMode::Async)?;
```

The `StreamConsumer` (used in `ConsumerRunner`) also implements `store_offset` from the `Consumer` trait.

### Key Kafka Semantics

**Offset = "the offset of the NEXT message to consume"**
- If committed offset = 100, Kafka will re-deliver messages starting at offset 101
- After processing offset 100, you commit offset 101 (the next expected offset)
- This means: committed offset = highest contiguous offset successfully processed + 1

**What "highest contiguous" means:**
```
Processed:  [ok, ok, ok, ok, ok]  (offsets 100, 101, 102, 103, 104)
Gap at:     offset 105 not yet processed
Highest contiguous acknowledged: 104
Safe to commit: 105 (104 + 1)
```

If messages 106, 107 arrive before 105, they are buffered but not committed. When 105 arrives and is acked, the contiguous range extends to 107 and commit advances to 108.

---

## 2. Differentiators (Competitive Advantage)

Features that set the product apart. Not required, but valued.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **Automatic offset tracking per ExecutionResult** | User does not manually call store/commit; the WorkerPool/Executor integration handles it transparently | MEDIUM | `ExecutionResult::Ok` triggers ack; `Error`/`Rejected` do not |
| **CommitExecutor plug-in** | A new `Executor` implementation that wraps `DefaultExecutor` and adds offset commit policy | MEDIUM | Satisfies "pluggable executor" requirement from v1.2 |
| **Backpressure-aware commit pacing** | When dispatcher is under backpressure, commit more aggressively to free up queue slots | HIGH | v1.4 territory |
| **Partition-level cursor tracking** | Each topic-partition has its own high-water-mark cursor | LOW | Already implied by `ExecutionContext` having `topic, partition, offset` |
| **Commit on graceful shutdown** | Before worker pool drains, commit the highest contiguous offset so no messages are re-delivered on restart | MEDIUM | `WorkerPool::shutdown()` triggers final commit |

---

## 3. Anti-Features (Commonly Requested, Often Problematic)

Features that seem good but create problems.

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| **Commit every message (per-ack commit)** | "Want every message committed immediately for exactly-once" | Kafka commit is a network round-trip; hammering commit per message kills throughput (10-100x slower) | Batch commits: commit every N messages or every T milliseconds |
| **Auto-commit enabled (rdkafka default)** | "Want zero configuration" | Auto-commit commits last offset periodically and on rebalance; if Python crashes between auto-commits, messages can be double-delivered or skipped | Manual commit via `CommitExecutor` with explicit acks |
| **Commit offset X when message X+1 arrives** | "Want to commit right after processing so the next poll picks up" | Committing offset N+1 before offset N is processed would cause data loss if the commit succeeds but the process dies before N completes | Commit only after confirmed successful processing of offset N |
| **Store offset per message without buffering** | "Want to store offset for every message" | Without buffering, out-of-order acks would cause gaps and premature commits or missed commits | Buffer pending acks, advance contiguous cursor |
| **Commit to all partitions on every partition's ack** | "Simpler: commit everything whenever anything is acknowledged" | Wastes network round-trips; different partitions have different high-water marks and should be committed independently | Per-partition commit decisions |

---

## 4. Feature Dependencies

### On Existing Modules

```
ExecutionResult (v1.2)
    └── Ok variant triggers ack() → OffsetTracker
    └── Error / Rejected variants do NOT trigger ack()

ExecutionContext (v1.2)
    └── Fields: topic, partition, offset — used as key in OffsetTracker
    └── Worker id — useful for logging which worker processed what

WorkerPool (v1.2)
    └── Calls executor.execute(ctx, msg, result) after Python execution
    └── After calling executor, if result is Ok → calls queue_manager.ack(topic, 1)
    └── This is where offset coordinator slots in: ack should also notify OffsetTracker

QueueManager (v1.1)
    └── Already tracks inflight per topic (not per topic-partition)
    └── OffsetTracker is separate from inflight tracking; different concerns

Executor trait (v1.2)
    └── Placeholder: RetryExecutor, OffsetAck
    └── New: CommitExecutor wraps DefaultExecutor and adds offset commit
```

### Module to Add

```
src/python/
    offset_tracker.rs    # OffsetTracker, per-topic-partition high-water cursor
    commit_executor.rs   # CommitExecutor, wraps DefaultExecutor with commit policy
```

### Integration Points

```
WorkerPool worker_loop()
    │
    ├── After handler.invoke() returns ExecutionResult::Ok
    │       ├── queue_manager.ack(topic, 1)  [existing]
    │       └── offset_tracker.ack(topic, partition, offset)  [NEW]
    │
    └── After executor.execute() returns ExecutorOutcome::Ack
            └── if offset_tracker.should_commit(topic, partition):
                    consumer.store_offset(topic, partition, cursor + 1)?;
                    consumer.commit()?;  [or batch via CommitMode::Async]
```

---

## 5. MVP Definition

### Launch With (v1.3)

Minimum viable: per-message store_offset + periodic commit with highest-contiguous guarantee.

- [x] **`OffsetTracker` per `(topic, partition)`** — tracks `pending: BTreeSet<i64>`, `committed_offset: i64` (last committed to Kafka)
- [x] **`ack(topic, partition, offset)` method** — called from WorkerPool on `ExecutionResult::Ok`; inserts into pending set; advances contiguous cursor if gap is filled
- [x] **`highest_contiguous(topic, partition) -> Option<i64>`** — returns highest offset that has been fully processed (gaps in pending are respected)
- [x] **`should_commit(topic, partition) -> bool`** — returns true if `highest_contiguous > committed_offset`
- [x] **`CommitExecutor` wrapping `DefaultExecutor`** — calls `store_offset()` then `commit()` when `should_commit` is true
- [x] **Configuration: `commit_interval_ms`, `commit_max_messages`** — triggers time-based or count-based commit
- [x] **Graceful shutdown commit** — `WorkerPool::shutdown()` triggers final commit for each topic-partition before workers exit

### Add After Validation (v1.4)

- [ ] **RetryExecutor integration with OffsetTracker** — when a message is retried, it should not be acked until retry succeeds; if retry exhausts, offset should NOT advance
- [ ] **DLQ routing with offset tracking** — rejected messages should be tracked so they do not prevent commit of earlier messages
- [ ] **Commit metrics** — emit `offset_lag`, `commit_batch_size`, `pending_buffer_size` metrics

---

## 6. Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| OffsetTracker (ack, pending, cursor) | HIGH — enables at-least-once delivery | MEDIUM | P1 |
| highest_contiguous calculation | HIGH — correctness guarantee | MEDIUM | P1 |
| CommitExecutor (store_offset + commit) | HIGH — wires to rdkafka | LOW | P1 |
| No duplicate commit guard | MEDIUM — performance | LOW | P1 |
| Per-partition tracking | HIGH — required for multi-partition | LOW | P1 |
| Commit interval/count threshold | MEDIUM — throughput vs safety tradeoff | LOW | P2 |
| Graceful shutdown final commit | MEDIUM — production correctness | MEDIUM | P2 |
| Out-of-order buffer (BTreeSet) | HIGH — required for concurrent worker ack ordering | MEDIUM | P1 |
| RetryExecutor integration with offset | MEDIUM — retry vs commit coordination | HIGH | P3 |
| DLQ with offset tracking | MEDIUM — advanced | HIGH | P3 |

---

## 7. Competitor Feature Analysis

| Feature | kafka-python | confluent-kafka-python | faust | KafPy (our approach) |
|---------|--------------|------------------------|-------|---------------------|
| Manual offset commit | Yes (per-message) | Yes (per-message or batch) | Yes (changelog) | Yes (CommitExecutor, v1.3) |
| store_offset per message | No (auto only) | Yes | Yes | Yes (CommitExecutor) |
| Out-of-order ack buffering | No | No | No | Yes (OffsetTracker, v1.3) |
| Highest-contiguous tracking | No | No | No | Yes (OffsetTracker, v1.3) |
| Configurable commit batching | No | Yes | Yes | Yes (v1.3) |
| Graceful shutdown commit | No | No | Yes | Yes (WorkerPool shutdown, v1.3) |
| Automatic retry+commit coordination | No | No | Yes | Planned (v1.4) |

---

## 8. Normal vs Anti-Patterns

### Normal: Highest Contiguous Offset Commit

```
WorkerPool processes messages from partition 0 concurrently:
  Worker-A: offset 10 succeeds  → ack(10)
  Worker-B: offset 12 succeeds  → ack(12)  [out of order - 11 pending]
  Worker-A: offset 11 succeeds  → ack(11)  [gap fills, contiguous 10-12]
  OffsetTracker:
    pending = {10, 11, 12}  →  highest_contiguous = 12
    committed = 9
    should_commit = (12 > 9) = true
  CommitExecutor calls:
    store_offset("events", 0, 13)  // 12 + 1 = next to consume
    commit()
```

This correctly handles concurrent workers and out-of-order acks.

### Anti-Pattern 1: Commit Every Ack Without Buffering

```rust
// ANTI-PATTERN — commits immediately on every ack, no gap tracking
fn on_ack(msg: OwnedMessage) {
    consumer.store_offset(msg.topic, msg.partition, msg.offset + 1)?;
    consumer.commit()?;  // Every single message = expensive
}
```

**What goes wrong:** Without buffering, if offset 11 succeeds before offset 10, you commit 12 (offset 11 + 1) while 10 is still pending. If the process crashes between commit and offset 10 completing, offset 10 is skipped forever (data loss). Even without a crash, this creates a spurious gap and at-least-once is violated.

**Do this instead:** Only commit when `highest_contiguous > committed_offset`. Buffer pending acks in a `BTreeSet<i64>`.

### Anti-Pattern 2: Commit Offset N Before Processing N

```rust
// ANTI-PATTERN — commit next offset before processing current message
fn process(msg: OwnedMessage) {
    // ...
    consumer.store_offset(msg.topic, msg.partition, msg.offset + 1)?; // WRONG
    consumer.commit()?;
}
```

**What goes wrong:** If the commit succeeds but the Python callback crashes before completing business logic, the message is effectively "processed" from Kafka's perspective but the business logic never ran. On rebalance or restart, that message is skipped.

**Do this instead:** Call `store_offset` and `commit` ONLY after `ExecutionResult::Ok` is confirmed from the Python callback.

### Anti-Pattern 3: Storing Pending Offsets in a HashSet

```rust
// ANTI-PATTERN — HashSet has no ordering, cannot determine contiguous range
struct Tracker {
    pending: HashSet<i64>,  // Cannot find "highest contiguous"
}
```

**What goes wrong:** `HashSet` has no ordering. To find the highest contiguous offset, you need to scan from `committed_offset + 1` upward until you find a gap. With `HashSet`, you cannot efficiently determine "next offset" or find contiguous ranges.

**Do this instead:** Use `BTreeSet<i64>` — supports `range()` queries to efficiently find the next pending offset and detect gaps.

```rust
// CORRECT — BTreeSet enables efficient contiguous range queries
fn highest_contiguous(&self) -> Option<i64> {
    let mut cursor = self.committed_offset + 1;
    loop {
        if self.pending.contains(&cursor) {
            cursor += 1;
        } else {
            return Some(cursor - 1);
        }
    }
}
```

### Anti-Pattern 4: Per-Topic Only (Not Per-Topic-Partition)

```rust
// ANTI-PATTERN — tracking offsets only by topic loses partition granularity
struct Tracker {
    pending: HashMap<String, HashSet<i64>>,  // partition lost!
}
```

**What goes wrong:** Kafka assigns offsets independently per partition. Committing based on topic-level state could accidentally commit offset N for partition 0 using offset N from partition 1's state.

**Do this instead:** Key tracking by `(topic, partition)` — the offset space is per-partition.

### Anti-Pattern 5: Commit with CommitMode::Async Without Tracking

```rust
// ANTI-PATTERN — Async commit returns immediately without confirming write
consumer.commit_consumer_state(CommitMode::Async)?;
// No tracking of whether commit actually succeeded
```

**What goes wrong:** `Async` mode does not wait for broker acknowledgment. If the commit is lost, Kafka will re-deliver the same messages (duplicate delivery, not data loss). This is fine for at-least-once, but the caller must not assume the offset is safely committed.

For at-least-once, `Async` is acceptable because re-delivery is safe. For exactly-once, you would need `CommitMode::Sync` and track committed offsets in an external store.

**Do this instead:** For at-least-once delivery, `Async` is acceptable and preferred (non-blocking). Track committed offset locally; assume commit succeeds unless the next rebalance reveals otherwise.

---

## 9. Out-of-Order Buffering: Detailed Behavior

This is the core complexity of concurrent worker offset tracking. Here is the precise algorithm:

### State

```rust
struct PartitionTracker {
    /// Last offset committed to Kafka (or offset to resume from on startup).
    /// Start: -1 (meaning "no offset committed yet"; Kafka uses "earliest" on first run).
    committed_offset: i64,
    /// Offsets that have been acknowledged but are not yet contiguous with committed_offset.
    /// Always acked in increasing order; inserted by worker on ExecutionResult::Ok.
    pending: BTreeSet<i64>,
    /// Offsets that failed (Error/Rejected) — should not prevent commit of earlier offsets.
    /// Tracked separately so they don't sit in pending forever.
    failed: BTreeSet<i64>,
}
```

### Algorithm: `ack(topic, partition, offset)`

```rust
fn ack(&mut self, offset: i64) {
    // A message succeeded — it can now be part of the contiguous range
    self.pending.insert(offset);

    // Remove from failed set if it was there (retry succeeded)
    self.failed.remove(&offset);

    // Advance cursor: while pending contains committed_offset + 1, pop and increment
    loop {
        let next = self.committed_offset + 1;
        if self.pending.remove(&next) {
            self.committed_offset = next;
        } else {
            break;
        }
    }
}
```

### Algorithm: `on_error(topic, partition, offset)`

```rust
fn on_error(&mut self, offset: i64) {
    // Message failed — move to failed set (does not block contiguous advancement)
    self.failed.insert(offset);

    // Note: do NOT insert into pending. Failed offsets are not considered
    // part of the contiguous range. But they also don't block it — other
    // successful messages with higher offsets can still advance the cursor.
    // This is correct for at-least-once: if 10 fails and 11 succeeds,
    // we commit 12 (11 + 1) after 11 is acked. On re-delivery, 10 will
    // be retried. Message 11 is not re-delivered (correct).
}
```

### Example: Concurrent Worker Out-of-Order Ack

```
Start: committed_offset = 9, pending = {}, failed = {}

Worker-A: msg.offset=10, result=Ok
  → ack(10): pending={10}, advance cursor: 9+1=10 in pending? yes → committed=10, pending={}
  → should_commit: (10 > 9) = true → commit offset 11

Worker-B: msg.offset=12, result=Ok  [arrives before Worker-C finishes 11]
  → ack(12): pending={12}, advance cursor: 10+1=11 not in pending → stop
  → should_commit: (10 > 10) = false → no commit

Worker-C: msg.offset=11, result=Ok
  → ack(11): pending={11,12}, advance cursor: 10+1=11 yes → committed=11, pending={12}
    continue: 11+1=12 yes → committed=12, pending={}
  → should_commit: (12 > 10) = true → commit offset 13
```

### Example: Message Failure With Concurrent Success

```
Start: committed_offset = 9, pending = {}, failed = {}

Worker-A: msg.offset=10, result=Error
  → on_error(10): failed={10}
  → pending={} (nothing inserted), cursor unchanged

Worker-B: msg.offset=11, result=Ok
  → ack(11): pending={11}, advance cursor: 10 not in pending → stop
  → should_commit: (9 > 9) = false → no commit
  → Result: offset 10 failed (retry later), offset 11 succeeded but we cannot
    commit 12 because 10 is in failed set. This is correct at-least-once:
    we will re-deliver 10. We do NOT re-deliver 11.

After retry succeeds:
  Worker-A: msg.offset=10, result=Ok
    → ack(10): pending={10,11}, advance: 10 yes → committed=10, pending={11}
      continue: 11 yes → committed=11, pending={}
    → should_commit: (11 > 9) = true → commit offset 12
```

---

## 10. Integration with Existing Modules

### WorkerPool changes (minimal)

In `src/worker_pool/mod.rs`, the `worker_loop` function after `ExecutionResult::Ok`:

```rust
// Current (v1.2):
ExecutionResult::Ok => {
    queue_manager.ack(&msg.topic, 1);
}

// v1.3 addition — notify offset tracker (if CommitExecutor is active):
ExecutionResult::Ok => {
    queue_manager.ack(&msg.topic, 1);
    if let Some(ref tracker) = self.offset_tracker {
        tracker.ack(&msg.topic, msg.partition, msg.offset);
    }
}
```

### Executor changes (minimal)

In `src/python/executor.rs`, the `CommitExecutor` (new):

```rust
pub struct CommitExecutor {
    inner: Arc<dyn Executor>,
    consumer: Arc<StreamConsumer>,
    tracker: Arc<OffsetTracker>,
    commit_interval: Duration,
    commit_max_messages: usize,
    last_commit: std::sync::Mutex<std::time::Instant>,
}

impl Executor for CommitExecutor {
    fn execute(&self, ctx: &ExecutionContext, msg: &OwnedMessage, result: &ExecutionResult) -> ExecutorOutcome {
        let outcome = self.inner.execute(ctx, msg, result);

        // After every Ok, check if we should commit
        if outcome == ExecutorOutcome::Ack {
            if self.tracker.should_commit(&ctx.topic, ctx.partition) {
                if let Some(offset) = self.tracker.highest_contiguous(&ctx.topic, ctx.partition) {
                    // store_offset takes offset of next message to consume
                    let next_offset = offset + 1;
                    if let Err(e) = self.consumer.store_offset(&ctx.topic, ctx.partition, next_offset) {
                        tracing::warn!("store_offset failed: {}", e);
                    } else if let Err(e) = self.consumer.commit_consumer_state(CommitMode::Async) {
                        tracing::warn!("commit failed: {}", e);
                    }
                }
            }
        }
        outcome
    }
}
```

### New module: `src/python/offset_tracker.rs`

```rust
/// Per-partition offset tracking state.
struct PartitionTracker {
    committed_offset: i64,
    pending: std::collections::BTreeSet<i64>,
    failed: std::collections::BTreeSet<i64>,
}

/// Thread-safe offset tracker keyed by (topic, partition).
pub struct OffsetTracker {
    partitions: std::sync::Arc<parking_lot::RwLock<std::collections::HashMap<(String, i32), PartitionTracker>>>,
}

impl OffsetTracker {
    pub fn new() -> Self;

    /// Called when Python execution succeeds for a message.
    pub fn ack(&self, topic: &str, partition: i32, offset: i64);

    /// Called when Python execution fails (Error or Rejected).
    pub fn mark_failed(&self, topic: &str, partition: i32, offset: i64);

    /// Returns the highest contiguous offset that has been fully processed.
    /// Returns None if no new offsets have been acknowledged.
    pub fn highest_contiguous(&self, topic: &str, partition: i32) -> Option<i64>;

    /// Returns true if there is a new offset to commit.
    pub fn should_commit(&self, topic: &str, partition: i32) -> bool {
        self.highest_contiguous(topic, partition)
            .map(|h| h > self.committed_offset(topic, partition))
            .unwrap_or(false)
    }

    /// Returns last committed offset (for initialization / restart).
    pub fn committed_offset(&self, topic: &str, partition: i32) -> i64;
}
```

---

## Sources

- rdkafka 0.38 `Consumer` trait: `store_offset(topic, partition, offset)` and `commit_consumer_state(CommitMode)` — verified via existing `ConsumerRunner::commit()` pattern in `src/consumer/runner.rs:116-122`
- Kafka consumer group offset protocol: `committed_offset = offset of last message successfully processed + 1`
- `BTreeSet<i64>` for pending offset buffering: standard approach in Kafka client libraries
- Confluent blog: "Exactly-once semantics are harder than you think" — covers at-least-once vs exactly-once tradeoffs
- Kafka protocol docs: `OffsetCommitRequest` semantics — offset is the "next expected offset"

---
*Feature research for: Offset Commit Coordinator (v1.3)*
*Researched: 2026-04-16*
