# Architecture Research: Offset Commit Coordinator

**Domain:** PyO3 Native Extension - Kafka at-least-once delivery with highest-contiguous-offset commit
**Researched:** 2026-04-16
**Confidence:** HIGH

## Executive Summary

Adding an offset commit coordinator to KafPy requires a new `src/coordinator/` module with two components: `OffsetTracker` (per-topic-partition ack state) and `OffsetCommitter` (rdkafka commit orchestration). The coordinator integrates with the existing `WorkerPool` by extending the `Executor` trait with an ack callback, and with `ConsumerRunner` through a new commit method. No changes are needed to `ConsumerDispatcher` or `Dispatcher` -- they remain purely about message routing.

The critical insight is that out-of-order execution means we cannot commit per-message. Instead, `OffsetTracker` maintains per-partition committed and inflight state, calculates highest-contiguous-offset on each ack, and signals `OffsetCommitter` when the committed offset advances. The commit task runs as a background Tokio task, decoupled from the hot worker path.

## Current Architecture (After v1.2)

```
ConsumerRunner (src/consumer/runner.rs)
    └─ stream() → ConsumerStream
            │
ConsumerDispatcher (src/dispatcher/mod.rs)
    ├─ owns: ConsumerRunner + Dispatcher
    ├─ run() → polls ConsumerStream, calls dispatcher.send()
    └─ register_handler(topic, capacity, max_concurrency)
            └─ returns mpsc::Receiver<OwnedMessage>
                    │
                    ▼
WorkerPool (src/worker_pool/mod.rs)
    ├─ N workers polling same receiver
    ├─ worker_loop() → handler.invoke() → executor.execute()
    └─ On ExecutionResult::Ok: queue_manager.ack(topic, 1)
                                                    ↑
                                            (no offset info)
Executor trait (src/python/executor.rs)
    ├─ execute(ctx, msg, result) → ExecutorOutcome
    └─ Placeholder OffsetAck trait already exists
```

**What already exists:**
- `ConsumerRunner::commit()` -- calls `rdkafka::consumer::Consumer::commit_consumer_state(Async)`
- `ConsumerRunner::assignment()` -- returns `TopicPartitionList` for pause/resume
- `Executor` trait with placeholder `OffsetAck` and `RetryExecutor` stubs
- `QueueManager::ack(topic, count)` -- decrements inflight, no offset semantics

## Integration Design

### New Module: `src/coordinator/`

```
src/coordinator/
├── mod.rs              # Module init, public exports
├── offset_tracker.rs    # Per-topic-partition ack state machine
├── commit_task.rs       # Background Tokio task managing rdkafka commits
└── error.rs             # CoordinatorError
```

**Why a new module (not in `python/` or `dispatcher/`):**
- Coordinator orchestrates cross-cutting concerns (commit coordination) that span `WorkerPool` (ack source) and `ConsumerRunner` (commit sink). Putting it in `python/` would create an unwanted dependency direction (python should not know about ConsumerRunner).
- `dispatcher/` is message routing only -- adding offset semantics would violate single responsibility.
- A dedicated `coordinator/` module keeps all commit-related logic in one place.

### Component: OffsetTracker

**Purpose:** Track per-partition acknowledged offsets and compute highest-contiguous-offset.

```rust
// src/coordinator/offset_tracker.rs

use std::collections::HashMap;
use tokio::sync::RwLock;

/// Per-partition offset state.
struct PartitionState {
    /// Highest offset we have committed to Kafka.
    committed_offset: i64,
    /// Highest offset we have acknowledged from a worker (may be > committed if gaps exist).
    highest_ack: i64,
    /// Offsets we have acknowledged but not yet committed (gaps from out-of-order processing).
    pending_offsets: std::collections::BTreeSet<i64>,
    /// Set of offsets that are "ready to commit" (no gaps below them).
    ready_to_commit: i64,
}
```

**Key operations:**

```rust
impl OffsetTracker {
    /// Called when a worker acknowledges a message as successfully processed.
    /// Returns the new highest-contiguous-offset to commit, if any.
    pub async fn ack(&self, topic: &str, partition: i32, offset: i64) -> Option<i64>;

    /// Returns the current committed offset for a topic-partition.
    pub async fn get_committed_offset(&self, topic: &str, partition: i32) -> Option<i64>;

    /// Returns the highest offset that is safe to commit (no gaps below it).
    pub async fn highest_contiguous(&self, topic: &str, partition: i32) -> Option<i64>;
}
```

**Highest-contiguous-offset algorithm:**
```
On ack(partition, offset):
  1. If offset <= committed_offset: no-op (already committed)
  2. Add offset to pending_offsets
  3. If offset == committed_offset + 1:
     - committed_offset = offset
     - while committed_offset + 1 in pending_offsets:
         remove from pending_offsets
         committed_offset += 1
     - return committed_offset (new commit point)
  4. Else:
     - return None (gap exists, cannot commit yet)
```

This correctly handles out-of-order: if we ack 5, then 3, then 4:
- ack(5): pending={5}, no contiguous, return None
- ack(3): pending={3,5}, no contiguous, return None
- ack(4): pending={3,4,5}, committed=3, contiguous up to 3+1=4 in pending, committed=4, loop finds 5 is next, committed=5, return Some(5)

### Component: OffsetCommitter (Background Task)

**Purpose:** Runs as a Tokio task, receives commit signals and calls `ConsumerRunner::commit()`.

```rust
// src/coordinator/commit_task.rs

pub struct OffsetCommitter {
    runner: Arc<ConsumerRunner>,
    tracker: Arc<OffsetTracker>,
    /// Channel to receive commit signals.
    commit_rx: watch::Receiver<Option<(String, i32, i64)>>,
    /// Minimum interval between commits to avoid thrashing.
    min_commit_interval: Duration,
    last_commit: std::sync::Mutex<Instant>,
}
```

**Commit signal flow:**
```
WorkerLoop
    ↓ (on ExecutionResult::Ok)
Executor.execute()
    ↓ (calls tracker.ack())
OffsetTracker
    ↓ (if highest-contiguous advances)
commit_tx.send((topic, partition, new_highest_offset))
    ↓
OffsetCommitter task
    ↓ (receives signal)
runner.store_offset(topic, partition, offset)?  // rdkafka store
runner.commit()?                                 // rdkafka commit
```

**Key design decisions:**
1. **Signalling via `watch` channel** -- a single sender (the tracker) broadcasting to one receiver (the committer task). Simple, no ownership complexity.
2. **Commit interval throttle** -- `min_commit_interval` prevents calling rdkafka commit more than once per N ms, even if tracker signals multiple times.
3. **Store before commit** -- rdkafka requires `store_offset()` before `commit_consumer_state()` for manual offset management. This is called on the committer task before the commit.

### Modified Component: Executor Trait

The `Executor` trait is extended to receive the coordinator reference:

```rust
// src/python/executor.rs (modified)

pub trait Executor: Send + Sync {
    fn execute(
        &self,
        ctx: &ExecutionContext,
        message: &OwnedMessage,
        result: &ExecutionResult,
    ) -> ExecutorOutcome;

    /// Called on successful execution -- allows executor to update offset tracker.
    /// Default implementation does nothing (maintains backward compatibility).
    fn on_ack(&self, _ctx: &ExecutionContext) {}
}

/// Extension trait for executors that coordinate offset commits.
pub trait OffsetCommittingExecutor: Executor {
    fn track_offset(&self, topic: &str, partition: i32, offset: i64);
}
```

**Alternative (cleaner):** Rather than extending `Executor`, add a separate `OffsetCoordinator` interface that the `WorkerPool` calls directly on each ack:

```rust
// WorkerPool calls this on each ExecutionResult::Ok
pub trait OffsetCoordinator: Send + Sync {
    fn record_ack(&self, topic: &str, partition: i32, offset: i64);
}
```

**Chosen approach:** `OffsetCoordinator` trait + `OffsetTracker` implements it. `WorkerPool` holds `Arc<dyn OffsetCoordinator>`. `Executor` remains unchanged (the ack path goes through coordinator, not executor).

**Rationale:** `Executor` policy (retry vs reject) and `OffsetCoordinator` tracking (ack vs offset advance) are two separate concerns. Mixing them in `Executor` creates a violation of single responsibility and forces every executor implementation to handle offset tracking.

### Modified Component: WorkerPool

```rust
// src/worker_pool/mod.rs (modified worker_loop)

async fn worker_loop(
    // ... existing params ...
    offset_coordinator: Option<Arc<dyn OffsetCoordinator>>,
) {
    // ...
    match result {
        ExecutionResult::Ok => {
            // Existing queue ack
            queue_manager.ack(&msg.topic, 1);
            // New: record offset for commit coordination
            if let Some(ref coord) = offset_coordinator {
                coord.record_ack(&msg.topic, msg.partition, msg.offset);
            }
        }
        // ...
    }
}
```

**Key change:** `WorkerPool::new()` takes an optional `Arc<dyn OffsetCoordinator>`. When `None`, behavior is unchanged (backward compatible).

### Modified Component: ConsumerRunner

```rust
// src/consumer/runner.rs (new methods)

impl ConsumerRunner {
    /// Stores the current consumer offset for a topic-partition.
    /// Must be called before commit_consumer_state() for manual offset management.
    pub fn store_offset(&self, topic: &str, partition: i32, offset: i64) -> Result<(), ConsumerError> {
        let mut tpl = rdkafka::TopicPartitionList::new();
        tpl.add_partition_offset(topic, partition, rdkafka::Offset::Offset(offset))
            .map_err(|e| ConsumerError::Kafka(KafkaError::from(e)))?;
        self.consumer.store_offsets(tpl)
            .map_err(ConsumerError::from)
    }

    /// Commits all stored offsets asynchronously.
    pub fn commit(&self) -> Result<(), ConsumerError> {
        self.consumer
            .commit_consumer_state(rdkafka::consumer::CommitMode::Async)
            .map_err(ConsumerError::from)
    }
}
```

`store_offset` is needed because rdkafka requires explicit offset storage before manual commit.

## Data Flow

### Ack → Commit Flow

```
1. ConsumerDispatcher::run() dispatches message
        ↓
2. WorkerPool worker picks up from mpsc::Receiver
        ↓
3. PythonHandler::invoke() → Python callback (spawn_blocking)
        ↓
4. ExecutionResult (Ok/Error/Rejected) returned
        ↓
5. WorkerLoop: if Ok:
        queue_manager.ack(topic, 1)           // existing inflight tracking
        offset_coordinator.record_ack(topic, partition, offset)  // NEW
        ↓
6. OffsetTracker::ack(topic, partition, offset)
        - Updates pending_offsets
        - Computes highest_contiguous
        - If contiguous advance:
          commit_tx.send((topic, partition, new_offset))
        ↓
7. OffsetCommitter task (background):
        - Receives commit signal
        - runner.store_offset(topic, partition, offset)
        - runner.commit()
```

### No Duplicate Commit Logic

```
OffsetTracker maintains per-partition committed_offset.
On ack(topic, partition, offset):
  if offset <= committed_offset: return  // already committed, no-op
  // ... proceed with pending tracking
```

This prevents duplicate commits when the same offset is ack'd from multiple workers (which should not happen with single-partition mpsc channel, but is defensively prevented).

## New vs Modified Components

| Component | New/Modified | Reason |
|-----------|-------------|--------|
| `src/coordinator/mod.rs` | **NEW** | Module init for coordinator |
| `src/coordinator/offset_tracker.rs` | **NEW** | Per-partition ack state machine |
| `src/coordinator/commit_task.rs` | **NEW** | Background rdkafka commit task |
| `src/coordinator/error.rs` | **NEW** | CoordinatorError |
| `src/consumer/runner.rs` | **MODIFIED** | Add `store_offset()` method |
| `src/python/executor.rs` | **MODIFIED** | Add `OffsetCoordinator` trait (alongside existing `Executor`) |
| `src/worker_pool/mod.rs` | **MODIFIED** | Accept optional `Arc<dyn OffsetCoordinator>`, call `record_ack()` |
| `src/pyconsumer.rs` | **MODIFIED** | Wire coordinator into WorkerPool and committer task |

## Project Structure

```
src/
├── lib.rs                      # Module init, pyclass exports
├── consumer/                   # Pure Rust consumer core
│   ├── config.rs
│   ├── error.rs
│   ├── message.rs              # OwnedMessage
│   └── runner.rs               # ConsumerRunner (MODIFIED: add store_offset)
├── dispatcher/                 # Pure Rust dispatcher (unchanged)
│   ├── mod.rs                  # Dispatcher, ConsumerDispatcher
│   ├── queue_manager.rs        # QueueManager, HandlerMetadata
│   ├── backpressure.rs
│   └── error.rs
├── python/                     # Python execution lane (unchanged)
│   ├── mod.rs
│   ├── execution_result.rs
│   ├── executor.rs             # MODIFIED: add OffsetCoordinator trait
│   ├── handler.rs
│   └── context.rs
├── worker_pool/                # Worker pool (MODIFIED)
│   └── mod.rs                  # WorkerPool: add offset_coordinator param
├── coordinator/                # NEW: offset commit coordinator
│   ├── mod.rs                  # OffsetTracker, OffsetCommitter, OffsetCoordinator traits
│   ├── offset_tracker.rs       # PartitionState, highest-contiguous algorithm
│   ├── commit_task.rs          # Background Tokio task
│   └── error.rs                # CoordinatorError
├── kafka_message.rs
├── pyconsumer.rs               # PyO3 Consumer bridge (MODIFIED)
└── config.rs
```

**Structure rationale:**
- `coordinator/` is a new top-level module because it orchestrates cross-component behavior
- `offset_tracker.rs` is pure Rust (no PyO3, no async in the tracking logic itself -- internal state uses tokio RwLock)
- `commit_task.rs` is async (Tokio task managing the rdkafka commit loop)
- No changes to `consumer/`, `dispatcher/`, or `python/` modules beyond the specific interfaces listed above

## Build Order

### Phase 1: OffsetTracker (No dependencies on other new components)

**`src/coordinator/offset_tracker.rs`**
- `PartitionState` struct with `committed_offset`, `pending_offsets`, `highest_ack`
- `OffsetTracker` struct with `RwLock<HashMap<(String, i32), PartitionState>>`
- `ack(topic, partition, offset) -> Option<i64>` -- the core algorithm
- `highest_contiguous(topic, partition) -> Option<i64>`
- Unit tests for the contiguous-offset algorithm

**`src/coordinator/error.rs`**
- `CoordinatorError` enum

**Testing:** Can be tested with plain Rust -- no Kafka, no Python, no async I/O needed for unit tests of the algorithm.

### Phase 2: OffsetCommitter (Depends on Phase 1 + ConsumerRunner)

**`src/coordinator/commit_task.rs`**
- `OffsetCommitter::new(runner, tracker, ...)`
- `spawn()` -- launches background Tokio task
- Uses `watch` channel for commit signals

**`src/coordinator/mod.rs`**
- Exports `OffsetTracker`, `OffsetCommitter`, `OffsetCoordinator` trait

**Testing:** Needs mock `ConsumerRunner` -- use `mockall` for trait mocking.

### Phase 3: ConsumerRunner store_offset (No dependencies)

**`src/consumer/runner.rs`**
- Add `store_offset(topic, partition, offset)` method
- Delegates to `rdkafka::consumer::Consumer::store_offsets()`

**Testing:** Integration test with real Kafka or testcontainers.

### Phase 4: Executor OffsetCoordinator Trait (No dependencies)

**`src/python/executor.rs`**
- Add `OffsetCoordinator` trait: `record_ack(topic, partition, offset)`
- `OffsetTracker` implements `OffsetCoordinator`
- Add `Arc<dyn OffsetCoordinator>` field to WorkerPool constructor

**Testing:** Unit test with mock OffsetCoordinator.

### Phase 5: WorkerPool Integration (Depends on Phase 1-4)

**`src/worker_pool/mod.rs`**
- `WorkerPool::new()` takes optional `Arc<dyn OffsetCoordinator>`
- `worker_loop()` calls `offset_coordinator.record_ack()` on `ExecutionResult::Ok`
- Existing `queue_manager.ack()` call remains unchanged

**Testing:** Integration test with mock Python callback + mock OffsetCoordinator.

### Phase 6: PyO3 Bridge (Depends on Phase 5)

**`src/pyconsumer.rs`**
- Create `OffsetTracker` and `OffsetCommitter` instances
- Wire `OffsetCommitter` background task
- Pass `Arc<dyn OffsetCoordinator>` to `WorkerPool::new()`

**Testing:** End-to-end test with real Kafka + Python callback.

## Commit/Store Coordination Flow

```
WorkerPool worker (tokio task, holds GIL briefly via spawn_blocking)
    │
    ├── ExecutionResult::Ok
    │
    └── offset_coordinator.record_ack(topic, partition, offset)
            │
            ▼
OffsetTracker::ack()
    │
    ├── Updates pending_offsets (BTreeSet)
    │
    ├── Computes highest_contiguous
    │
    └── If committed_offset advances:
            commit_tx.send((topic, partition, new_highest))
                    │
                    ▼
OffsetCommitter task (background Tokio task, NOT holding GIL)
    │
    ├── Receives commit signal
    │
    ├── Checks min_commit_interval throttle
    │
    ├── runner.store_offset(topic, partition, offset)  // rdkafka store
    │
    └── runner.commit()                               // rdkafka async commit
```

**Why separate the commit task:**
1. `store_offset` and `commit` are rdkafka operations -- they should not be on the worker hot path
2. `spawn_blocking` (Python GIL) must be brief -- rdkafka network I/O should not compete
3. The commit interval throttle prevents commit storms when many offsets become contiguous at once
4. The watch channel is single-producer (tracker) single-consumer (committer) -- perfect fit

## Anti-Patterns to Avoid

### 1. Committing Per-Message on the Worker Path

**What people do:** Call `commit()` immediately after `ExecutionResult::Ok` on the worker task.

**Why it's wrong:** rdkafka commits involve network I/O. Doing this on the worker hot path adds latency and can cause duplicate commits when offsets are acknowledged out-of-order.

**Do this instead:** Decouple via `OffsetTracker` + `OffsetCommitter` task. Workers call `record_ack()` which is a pure in-memory update. Commit is a separate background task.

### 2. Storing Offset State in QueueManager

**What people do:** Add offset tracking to `QueueManager` alongside queue depth/inflight.

**Why it's wrong:** `QueueManager` already has one responsibility (queue metadata tracking). Adding offset semantics violates single responsibility and creates a circular dependency (QueueManager would need to know about ConsumerRunner for commits).

**Do this instead:** `OffsetTracker` is a separate component with its own module. `QueueManager` remains unchanged.

### 3. Blocking on Commit in Worker Loop

**What people do:** `runner.commit().await` on the worker task.

**Why it's wrong:** `commit_consumer_state(Async)` returns immediately but internally queues the commit request. However, the worker task becomes coupled to the consumer's commit machinery. More importantly, if multiple workers all call commit simultaneously, you get redundant commits.

**Do this instead:** Single `OffsetCommitter` task owns the commit path. All workers send signals via the tracker.

### 4. Not Storing Offsets Before Committing

**What people do:** Calling `commit()` without `store_offset()` first.

**Why it's wrong:** rdkafka requires explicit offset storage via `store_offsets()` before `commit_consumer_state()` for manual offset management. Without it, the commit uses the consumer's internal auto-commit state.

**Do this instead:** Always call `store_offset()` before `commit()` in the `OffsetCommitter` task.

## Integration Points

### With WorkerPool

| Boundary | Communication | Notes |
|----------|---------------|-------|
| WorkerPool → OffsetCoordinator | `record_ack(topic, partition, offset)` | Called on each `ExecutionResult::Ok` |
| OffsetCoordinator → OffsetTracker | Internal `Arc<RwLock<...>>` | Tracker is the state backend |
| OffsetTracker → OffsetCommitter | `watch::Sender<Option<(String,i32,i64)>>` | Signal on contiguous advance |

### With ConsumerRunner

| Method | Purpose | Notes |
|--------|---------|-------|
| `store_offset(topic, partition, offset)` | Persist offset to rdkafka | Must be called before `commit()` |
| `commit()` | Async commit of stored offsets | Already exists, wraps `commit_consumer_state(Async)` |

### With Executor (unchanged)

`Executor::execute()` is still called on each execution result. The offset ack path goes through `OffsetCoordinator` trait, not through `Executor`. This keeps the two concerns separate.

## Scaling Considerations

| Scale | Concern | Approach |
|-------|---------|----------|
| 1-10 partitions | Memory for pending_offsets | BTreeSet per partition is efficient up to ~100K pending offsets |
| 10-100 partitions | RwLock contention on tracker | Partition state is per-(topic, partition) -- fine-grained locking |
| 100+ partitions | Commit thrashing | `min_commit_interval` throttle (e.g., 100ms default) |
| High throughput | Many offsets per second | `OffsetTracker::ack` is O(log N) where N = pending offsets per partition |

**Memory estimate:** For a partition with 1M message lag and 1s commit interval, worst-case pending_offsets BTreeSet holds ~1000 offsets (1s of processing at 1K msg/s). Each i64 is 8 bytes + BTreeSet overhead. Negligible.

## Sources

- rdkafka `Consumer::store_offsets` and `commit_consumer_state` API (confirmed via existing `ConsumerRunner::commit()` implementation)
- Tokio `watch` channel for single-producer single-consumer signalling (standard pattern)
- Existing `Executor` trait and `OffsetAck` placeholder stub in `src/python/executor.rs`
- `WorkerPool::worker_loop` ack pattern in `src/worker_pool/mod.rs`

---
*Architecture research for: offset commit coordinator*
*Researched: 2026-04-16*