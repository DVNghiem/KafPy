# Architecture Research: KafPy Lifecycle Management

**Domain:** Rust/PyO3 Kafka Consumer Lifecycle -- Shutdown, Rebalance, Partition Ownership
**Researched:** 2026-04-19
**Confidence:** MEDIUM-HIGH

## Executive Summary

KafPy's lifecycle management today is fragmented: `WorkerPool::shutdown()` handles worker drain, `OffsetTracker::graceful_shutdown()` handles final commits, `OffsetCommitter` runs independently without shutdown awareness, and there is no rebalance response infrastructure. The current `stop()` on `Consumer` only cancels the `CancellationToken` -- no coordination across components.

This research recommends adding a `ShutdownCoordinator` that orchestrates a 3-phase shutdown (stop consuming, drain workers, commit final offsets) and a `RebalanceHandler` that responds to partition assignment changes by updating `PartitionOwnership` and signaling the `ShutdownCoordinator` when partitions are revoked.

**Key finding:** `rdkafka` does not support native Rust callbacks for rebalance events. Rebalance detection requires either a polling approach (check assignment changes on an interval) or subscribing to the rdkafka consumer's assignment via the existing `StreamConsumer` loop. The recommended approach is to detect rebalance inside the existing `ConsumerRunner::run()` loop by comparing consecutive `assignment()` results, since the run loop already polls `consumer.recv()`.

---

## Existing Architecture

```
pyconsumer.rs (Consumer)
    shutdown_token: CancellationToken
    handlers: Arc<Mutex<HashMap<String, Arc<Py<PyAny>>>>>

start() {
    runner_arc = Arc::new(ConsumerRunner::new(...))
    offset_tracker = Arc::new(OffsetTracker::new())
    offset_tracker.set_runner(runner_arc.clone())
    dispatcher = ConsumerDispatcher::new(runner.clone())

    receivers = dispatcher.register_handler(...)  // per-topic mpsc::Receiver

    handler = Arc::new(PythonHandler::new(...))
    pool = WorkerPool::new(n_workers, receivers, handler, ...,
        offset_tracker.clone(), retry_coordinator, shutdown_token.clone())

    committer = OffsetCommitter::new(runner_arc, offset_tracker, config)
    spawn(committer.run(rx))

    spawn(dispatcher.run(...))

    pool.run().await  // blocks until shutdown
    // dispatcher_handle and committer_handle dropped here
}
```

**Current shutdown flow:**
1. `Consumer::stop()` -> `shutdown_token.cancel()` (only affects WorkerPool)
2. `WorkerPool::shutdown()` -> cancels token, flushes failed to DLQ, calls `graceful_shutdown()`, awaits join_set
3. `OffsetTracker::graceful_shutdown()` -> iterates all partitions, stores + commits highest contiguous
4. `OffsetCommitter::run()` -> loops on watch channel + ticker; exits when channel closes (but nobody closes it on shutdown!)
5. `ConsumerDispatcher::run()` -> async loop on `runner.stream()`; exits when stream returns `None` (sender dropped)

**Current problems identified:**

| Problem | Impact | Location |
|---------|--------|----------|
| `stop()` only cancels token, doesn't stop dispatcher | Dispatcher continues polling until ConsumerRunner stops | `pyconsumer.rs:184-186` |
| `OffsetCommitter` has no shutdown signal | Committer task runs forever even after pool shutdown | `commit_task.rs:124-139` |
| No rebalance response | Partitions revoked mid-processing leave worker in undefined state | `consumer/runner.rs` |
| No partition ownership tracking | Dispatcher can't know which partitions it currently owns | `dispatcher/mod.rs:257-266` |
| `BatchAccumulator` not drained on rebalance | In-flight batches for revoked partitions lost | `worker_pool/mod.rs:565-611` |
| `retry_coordinator` not consulted on shutdown | In-flight retries not flushed before final commit | `worker_pool/mod.rs:1108-1117` |

---

## Recommended Architecture

### Phase 1: ShutdownCoordinator

A single orchestrator that drives the 3-phase shutdown sequence. Owned by `Consumer`, created in `start()`.

```rust
/// Orchestrates graceful shutdown across all KafPy components.
///
/// Phase 1: Signal all components to stop accepting new work
/// Phase 2: Wait for in-flight work to complete (with timeout)
/// Phase 3: Commit final offsets and drain remaining state
///
/// ## Shutdown Order
///
/// 1. ConsumerRunner -- stop producing messages
/// 2. WorkerPool -- stop accepting new dispatches, wait for inflight
/// 3. BatchAccumulator -- flush all pending batches
/// 4. RetryCoordinator -- flush retry backlog
/// 5. OffsetTracker -- commit highest contiguous offsets
/// 6. OffsetCommitter -- stop accepting new commits
pub struct ShutdownCoordinator {
    phase: parking_lot::Mutex<ShutdownPhase>,
    /// Notifies all components to enter drain mode.
    drain_token: CancellationToken,
    /// Signaled after Phase 2 (inflight complete) to trigger Phase 3.
    drain_complete: broadcast::Sender<DrainComplete>,
    /// JoinHandle for OffsetCommitter task (so we can cancel it in Phase 3).
    committer_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
    /// JoinHandle for Dispatcher task.
    dispatcher_handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownPhase {
    /// Normal operation -- all components running.
    Running,
    /// Phase 1 -- drain signal sent, waiting for inflight to complete.
    Draining,
    /// Phase 3 -- final commits in progress.
    Finalizing,
    /// Shutdown complete.
    Done,
}
```

**Integration with existing components:**

1. **ConsumerRunner**: `ShutdownCoordinator` calls `runner.stop()` in Phase 1 (sends broadcast to `shutdown_rx` in `run()` loop)
2. **ConsumerDispatcher**: `ShutdownCoordinator` drops the dispatcher handle in Phase 1, causing `run()` stream to close
3. **WorkerPool**: `ShutdownCoordinator` calls `pool.shutdown()` in Phase 2 (waits with timeout)
4. **BatchAccumulator**: `batch_worker_loop` already drains on `shutdown_token.cancelled()` -- Phase 2 waits for this
5. **RetryCoordinator**: Phase 3 calls `retry_coordinator.flush_pending_retries()` (new method needed)
6. **OffsetTracker**: Phase 3 calls `graceful_shutdown()` (already exists)
7. **OffsetCommitter**: Phase 3 cancels the committer via `committer_handle.abort()`

**Python API:**

```rust
// pyconsumer.rs
#[pymethods]
impl Consumer {
    pub fn start(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let coordinator = ShutdownCoordinator::new(
                shutdown_token.clone(),
                runner_arc.clone(),
                offset_tracker.clone(),
                pool.clone(),
                dispatcher.clone(),
                committer_handle,  // passed in to be owned
            );

            // Spawn all tasks under coordinator's management
            // ...
        })
    }

    pub fn stop(&self) {
        // NEW: Coordinated shutdown instead of just token cancel
        self.coordinator.shutdown();
    }
}
```

### Phase 2: PartitionOwnership

Tracks which partitions the consumer currently owns and whether they are active/revoked.

```rust
/// Thread-safe partition ownership state.
///
/// Updated on every rebalance. Used by Dispatcher and WorkerPool
/// to determine which partitions they should be processing.
pub struct PartitionOwnership {
    /// Current assignment: topic -> Vec<Partition>
    assignment: parking_lot::RwLock<HashMap<String, Vec<i32>>>,
    /// Partitions that were revoked but not yet fully drained
    revoking: parking_lot::RwLock<HashMap<String, Vec<i32>>>,
    /// Assignment metadata for pause/resume
    partition_handles: Arc<parking_lot::Mutex<HashMap<String, rdkafka::TopicPartitionList>>>,
}

impl PartitionOwnership {
    /// Updates assignment on rebalance.
    /// Returns (newly_owned, newly_revoked) for downstream processing.
    pub fn update_assignment(
        &self,
        new_assignment: rdkafka::TopicPartitionList,
    ) -> (Vec<(String, i32)>, Vec<(String, i32)>) {
        // Compare with current, compute delta
        // Update assignment and revoking sets
    }

    /// Returns partitions currently owned.
    pub fn current_ownership(&self) -> Vec<(String, i32)> { }

    /// Marks a partition as fully drained after rebalance.
    pub fn mark_drained(&self, topic: &str, partition: i32) { }

    /// Returns true if the partition is currently owned.
    pub fn is_owned(&self, topic: &str, partition: i32) -> bool { }

    /// Returns the TopicPartitionList for pause/resume.
    pub fn get_handles(&self) -> Arc<parking_lot::Mutex<HashMap<String, rdkafka::TopicPartitionList>>> { }
}
```

**Integration with ConsumerDispatcher:**

```rust
// In ConsumerDispatcher
pub struct ConsumerDispatcher {
    runner: Arc<ConsumerRunner>,
    dispatcher: Dispatcher,
    partition_ownership: Arc<PartitionOwnership>,  // NEW
    // ...
}

impl ConsumerDispatcher {
    /// Called on rebalance. Updates ownership and pauses revoked partitions.
    pub fn on_rebalance(&self, assignment: rdkafka::TopicPartitionList) {
        let (owned, revoked) = self.partition_ownership.update_assignment(assignment);
        // Pause revoked partitions
        for (topic, partition) in &revoked {
            self.pause_partition(topic);
        }
    }

    /// Called by WorkerPool when a partition is fully drained post-revoke.
    pub fn on_partition_drained(&self, topic: &str, partition: i32) {
        self.partition_ownership.mark_drained(topic, partition);
    }
}
```

### Phase 3: RebalanceHandler

Detects rebalance events and coordinates the response.

**Key architectural question:** How does rdkafka expose rebalance to Rust?

Looking at the rdkafka source and documentation, `StreamConsumer` does not have a native callback mechanism in Rust (unlike the Java client). The recommended detection approach is:

1. **Polling-based**: Periodically call `consumer.assignment()` and compare with the previous snapshot. If changed, a rebalance occurred.
2. **Message-based**: Most rebalances are preceded by a `KafkaError::PartitionEof` or similar signal, but this is unreliable.
3. **Consumer loop integration**: Compare assignment at the start of each `consumer.recv()` iteration.

**Recommended approach (Option 3):** Detect rebalance inside `ConsumerRunner::run()` by storing the previous assignment snapshot and comparing on each iteration. When assignment changes:
1. Emit a rebalance event (via broadcast channel)
2. `RebalanceHandler` receives it, calls `ConsumerDispatcher::on_rebalance()`
3. Dispatcher pauses revoked partitions, updates `PartitionOwnership`

```rust
// In ConsumerRunner::run()
async fn run(&self) -> mpsc::Receiver<Result<OwnedMessage, ConsumerError>> {
    let (tx, rx) = mpsc::channel(1000);
    let consumer = Arc::clone(&self.consumer);
    let mut shutdown_rx = self.shutdown_tx.subscribe();
    let mut prev_assignment = consumer.assignment().ok();
    let rebalance_tx = self.rebalance_tx.clone(); // NEW: broadcast channel for rebalance events

    tokio::spawn(async move {
        loop {
            select! {
                biased;
                _ = shutdown_rx.recv() => {
                    break;
                }
                message_result = consumer.recv() => {
                    // Check for rebalance
                    let current_assignment = consumer.assignment().ok();
                    if current_assignment != prev_assignment {
                        let (owned, revoked) = diff_assignments(&prev_assignment, &current_assignment);
                        let _ = rebalance_tx.send(RebalanceEvent { owned, revoked });
                        prev_assignment = current_assignment;
                    }

                    match message_result {
                        // ...
                    }
                }
            }
        }
    });
    rx
}
```

**RebalanceEvent broadcast:**

```rust
/// Emitted by ConsumerRunner when partition assignment changes.
#[derive(Debug, Clone)]
pub struct RebalanceEvent {
    pub newly_owned: Vec<(String, i32)>,
    pub newly_revoked: Vec<(String, i32)>,
}
```

**RebalanceHandler integration:**

```rust
// In pyconsumer.rs start()
let rebalance_tx = broadcast::Sender::<RebalanceEvent>::new(1);
let rebalance_rx = rebalance_tx.subscribe();

let rebalance_handler = RebalanceHandler::new(
    Arc::clone(&dispatcher),
    Arc::clone(&partition_ownership),
    Arc::clone(&retry_coordinator),
);

tokio::spawn(async move {
    rebalance_handler.run(rebalance_rx).await;
});
```

---

## Integration with Existing Components

### Integration with BatchAccumulator

`BatchAccumulator` is per-worker, embedded in `batch_worker_loop`. On rebalance revocation:

1. `RebalanceHandler` calls `partition_ownership.mark_revoked(topic, partition)`
2. WorkerPool workers observe revocation and drain:
   - `batch_worker_loop` already handles `shutdown_token.cancelled()` by flushing all
   - Need a separate signal for rebalance-specific drain (different from full shutdown)
3. Add `partition_revoked` field to signal targeted drain vs. full shutdown

**Alternative:** Use existing `shutdown_token` for rebalance drain too -- when `CancellationToken` is cancelled, all workers drain all batches. This is simpler but less targeted.

**Recommendation:** Use `CancellationToken` for both shutdown and rebalance drain. When a partition is revoked, cancel only the workers that own that partition (partition-to-worker mapping needed).

### Integration with RetryCoordinator

`RetryCoordinator` has pending retry state in `HashMap<RetryKey, MessageRetryState>`. On shutdown:

1. Phase 3 (Finalizing): iterate all retry state, for each pending retry:
   - Produce message to DLQ (don't await)
   - Remove from retry state

**New method on RetryCoordinator:**

```rust
/// Flushes all pending retries to DLQ.
/// Called during graceful shutdown before final offset commit.
pub fn flush_pending_retries(
    &self,
    dlq_router: &Arc<dyn DlqRouter>,
    dlq_producer: &Arc<SharedDlqProducer>,
) {
    let state_guard = self.state.lock();
    for (key, msg_state) in state_guard.iter() {
        let metadata = DlqMetadata::new(
            msg_state.topic.clone(),
            msg_state.partition,
            msg_state.offset,
            msg_state.last_failure.as_ref().map(|r| r.to_string()).unwrap_or_else(|| "unknown".to_string()),
            msg_state.attempt as u32,
            chrono::Utc::now(),
            chrono::Utc::now(),
        );
        let tp = dlq_router.route(&metadata);
        dlq_producer.produce_async(tp.topic, tp.partition, vec![], None, &metadata);
    }
}
```

### Integration with OffsetCommitter

The `OffsetCommitter` task currently runs forever with no shutdown signal. On `ShutdownCoordinator::shutdown()`:

1. Phase 1: abort `committer_handle`
2. Phase 3: before `graceful_shutdown()`, ensure no new commits can start

**Current problem:** `OffsetCommitter::run()` uses `tokio::select!` on `rx.changed()` and `ticker.tick()`. When the watch channel closes (tx dropped), the loop exits -- but in current code, tx is never dropped until `pyconsumer` drops the committer after `pool.run().await`. This works only because the channel is scoped to the async block.

**Fix:** `ShutdownCoordinator` owns the committer's `JoinHandle` and calls `.abort()` on Phase 1. This is safe because `OffsetCommitter::run()` checks for cancellation:

```rust
// In commit_task.rs
pub async fn run(mut self, mut rx: watch::Receiver<TopicPartition>) {
    let mut ticker = interval(Duration::from_millis(self.config.commit_interval_ms));

    loop {
        tokio::select! {
            _ = rx.changed() => { ... }
            _ = ticker.tick() => { ... }
            // NEW: catch cancellation
            _ = tokio::task::yield_now() => { /* check for cancellation */ }
        }
    }
}
```

Actually `.abort()` on a Tokio task causes it to simply stop -- no cleanup needed. So Phase 1 calling `committer_handle.abort()` is sufficient.

---

## New Components Specification

### ShutdownCoordinator

```rust
pub struct ShutdownCoordinator {
    phase: AtomicU8,
    drain_token: CancellationToken,
    drain_complete: broadcast::Sender<DrainComplete>,
    runner: Arc<ConsumerRunner>,
    offset_tracker: Arc<OffsetTracker>,
    worker_pool: Arc<WorkerPool>,
    dispatcher_handle: Mutex<Option<JoinHandle<()>>>,
    committer_handle: Mutex<Option<JoinHandle<()>>>,
}

impl ShutdownCoordinator {
    /// Initiates graceful shutdown. Non-blocking.
    pub fn shutdown(&self) {
        self.transition_to(ShutdownPhase::Draining);
        self.drain_token.cancel();
    }

    /// Blocks until shutdown is complete (Phase 3 done).
    pub async fn await_shutdown(&self) { }

    /// Transitions to Phase 3 (finalize) after drain complete.
    async fn finalize(&self) {
        self.transition_to(ShutdownPhase::Finalizing);
        // Phase 3: flush retries, commit offsets, abort committer
        self.flush_retries().await;
        self.commit_offsets().await;
        self.abort_committer();
        self.transition_to(ShutdownPhase::Done);
    }
}
```

### RebalanceHandler

```rust
pub struct RebalanceHandler {
    dispatcher: Arc<ConsumerDispatcher>,
    partition_ownership: Arc<PartitionOwnership>,
    retry_coordinator: Arc<RetryCoordinator>,
}

impl RebalanceHandler {
    /// Listens for rebalance events and coordinates response.
    pub async fn run(&self, mut rx: broadcast::Receiver<RebalanceEvent>) {
        loop {
            match rx.recv().await {
                Ok(event) => self.handle_event(event).await,
                Err(broadcast::Error::Lagged(_)) => { /* log and continue */ }
                Err(broadcast::Error::Closed) => break,
            }
        }
    }

    async fn handle_event(&self, event: RebalanceEvent) {
        // 1. Pause revoked partitions (calls dispatcher.pause_partition)
        for (topic, _) in &event.newly_revoked {
            if let Err(e) = self.dispatcher.pause_partition(topic) {
                tracing::error!("failed to pause revoked topic '{}': {}", topic, e);
            }
        }

        // 2. Mark partitions as revoking in PartitionOwnership
        for (topic, partition) in &event.newly_revoked {
            self.partition_ownership.mark_revoking(topic, partition);
        }

        // 3. Wait for workers to drain revoked partitions
        self.wait_for_drain(&event.newly_revoked).await;

        // 4. Mark partitions as fully revoked
        for (topic, partition) in &event.newly_revoked {
            self.partition_ownership.mark_revoked(topic, partition);
        }

        // 5. Resume newly owned partitions
        for (topic, _) in &event.newly_owned {
            if let Err(e) = self.dispatcher.resume_partition(topic) {
                tracing::error!("failed to resume owned topic '{}': {}", topic, e);
            }
        }
    }
}
```

### PartitionOwnership

```rust
pub struct PartitionOwnership {
    assignment: RwLock<HashMap<String, Vec<i32>>>,
    revoking: RwLock<HashMap<String, Vec<i32>>>,
    partition_handles: Arc<parking_lot::Mutex<HashMap<String, rdkafka::TopicPartitionList>>>,
}

impl PartitionOwnership {
    /// Returns (newly_owned, newly_revoked) by comparing with current assignment.
    pub fn update_assignment(
        &self,
        new_assignment: &rdkafka::TopicPartitionList,
    ) -> (Vec<(String, i32)>, Vec<(String, i32)>) { }

    pub fn mark_revoking(&self, topic: &str, partition: i32) { }
    pub fn mark_drained(&self, topic: &str, partition: i32) { }
    pub fn is_owned(&self, topic: &str, partition: i32) -> bool { }
}
```

---

## Modified Components

### ConsumerRunner (src/consumer/runner.rs)

**Change:** Add `rebalance_tx` broadcast channel to emit rebalance events.

```rust
pub struct ConsumerRunner {
    consumer: Arc<StreamConsumer>,
    shutdown_tx: broadcast::Sender<()>,
    rebalance_tx: broadcast::Sender<RebalanceEvent>,  // NEW
    prev_assignment: parking_lot::Mutex<Option<rdkafka::TopicPartitionList>>,
}
```

**In `run()` loop:** Before processing each message, compare `consumer.assignment()` with `prev_assignment`. If changed, emit `RebalanceEvent`.

### ConsumerDispatcher (src/dispatcher/mod.rs)

**Change:** Add `partition_ownership: Arc<PartitionOwnership>` field.

```rust
pub struct ConsumerDispatcher {
    runner: Arc<ConsumerRunner>,
    dispatcher: Dispatcher,
    partition_ownership: Arc<PartitionOwnership>,  // NEW
    // ...
}
```

**New methods:**
- `pause_partition(topic)` -- pause all partitions of topic (uses `partition_handles`)
- `resume_partition(topic)` -- resume all partitions of topic
- `on_rebalance(assignment)` -- called by RebalanceHandler

### WorkerPool (src/worker_pool/mod.rs)

**Change:** `shutdown()` method already does the right thing -- cancel token, flush DLQ, graceful shutdown. Add integration with `PartitionOwnership` to mark partition drain complete.

### OffsetTracker (src/coordinator/offset_tracker.rs)

**Change:** `graceful_shutdown()` already exists and commits all partitions. Add check for `should_commit()` before committing (already implemented).

### RetryCoordinator (src/coordinator/retry_coordinator.rs)

**Change:** Add `flush_pending_retries()` method for Phase 3 of shutdown.

### QueueManager (src/dispatcher/queue_manager.rs)

**Change:** `ack()` already decrements inflight. No changes needed.

---

## Build Order

### Phase 1: ShutdownCoordinator (Foundation)

1. Create `src/coordinator/shutdown_coordinator.rs`
2. Define `ShutdownPhase` enum and `ShutdownCoordinator` struct
3. Add `drain_token: CancellationToken` to `ShutdownCoordinator`
4. Wire `ShutdownCoordinator::shutdown()` to `Consumer::stop()`
5. Modify `pyconsumer.rs` to create and store `ShutdownCoordinator`

**Rationale:** Establishes the central shutdown orchestration before adding rebalance handling.

### Phase 2: PartitionOwnership

1. Create `src/coordinator/partition_ownership.rs`
2. Define `PartitionOwnership` struct with `RwLock` assignment map
3. Add `update_assignment()` method returning (newly_owned, newly_revoked)
4. Modify `ConsumerDispatcher` to hold `Arc<PartitionOwnership>`
5. Modify `populate_partitions()` to initialize `PartitionOwnership`

**Rationale:** PartitionOwnership is needed by RebalanceHandler, which needs it to coordinate pause/resume.

### Phase 3: RebalanceHandler

1. Create `src/coordinator/rebalance_handler.rs`
2. Define `RebalanceEvent` and `RebalanceHandler` struct
3. Modify `ConsumerRunner` to emit rebalance events on assignment change
4. Add `RebalanceHandler::run()` that calls `ConsumerDispatcher::on_rebalance()`
5. Wire into `pyconsumer.rs::start()`

**Rationale:** Rebalance handling is the most complex new component. Build it after basic infrastructure.

### Phase 4: ShutdownCoordinator Phase Integration

1. Wire Phase 1 of `ShutdownCoordinator` to call `runner.stop()` and abort handles
2. Wire Phase 2 to wait for `WorkerPool::shutdown()` with timeout
3. Add `RetryCoordinator::flush_pending_retries()` method
4. Wire Phase 3 to flush retries, commit offsets, abort committer
5. Add graceful timeout (default 30s) after which forced shutdown occurs

**Rationale:** Integrates all components into the coordinated shutdown sequence.

### Phase 5: BatchAccumulator Revoke Integration

1. Add `revoke_partition()` method to `BatchAccumulator`
2. Modify `batch_worker_loop` to check `PartitionOwnership::is_owned()` before processing
3. On revocation: drain the accumulator for that partition, ack with 0 (don't reprocess)

**Rationale:** Ensures no messages are lost during partition revocation.

---

## Data Flow: Shutdown Sequence

```
Consumer::stop()
    |
    v
ShutdownCoordinator::shutdown()
    |
    +-- Phase 1: Signal stop
    |       |
    |       +-- runner.stop()  [broadcast to ConsumerRunner shutdown_rx]
    |       +-- abort(dispatcher_handle)
    |       +-- abort(committer_handle)
    |       +-- drain_token.cancel()
    |
    +-- Phase 2: Drain inflight
    |       |
    |       +-- pool.shutdown()  [cancels worker token, flushes DLQ, awaits workers]
    |       +-- flush_pending_retries()  [RetryCoordinator]
    |
    +-- Phase 3: Finalize
            |
            +-- offset_tracker.graceful_shutdown()  [store_offset + commit all]
            +-- phase = Done
```

---

## Data Flow: Rebalance Sequence

```
ConsumerRunner::run() loop
    |
    v
consumer.assignment() != prev_assignment
    |
    v
rebalance_tx.send(RebalanceEvent { owned, revoked })
    |
    v
RebalanceHandler::run() receives event
    |
    +-- dispatcher.pause_partition(revoked_topics)
    +-- partition_ownership.mark_revoking(topic, partition)
    |
    v
Workers drain messages for revoked partitions
    |
    v
partition_ownership.mark_drained() for each revoked partition
    |
    v
dispatcher.resume_partition(newly_owned_topics)
```

---

## Anti-Patterns to Avoid

### Anti-Pattern 1: Blocking in Shutdown

**What:** Calling `.await` on a future inside a drop implementation or synchronous cleanup.

**Why:** Can cause deadlock if the future holds a lock that another task needs.

**Do this instead:** Use `spawn()` for async cleanup and detach. Use `tokio::task::spawn_blocking` for sync cleanup.

### Anti-Pattern 2: Shared Mutex on Hot Path

**What:** Using `parking_lot::Mutex` or `std::sync::Mutex` in the message processing path (e.g., in `worker_loop`).

**Why:** Blocks the async executor. `tokio::sync::Mutex` is async-aware; `parking_lot::Mutex` is not.

**Do this instead:** Use `parking_lot::Mutex` only in initialization and shutdown paths. For hot path, use atomic operations (already done correctly with `AtomicUsize` in `QueueManager`).

### Anti-Pattern 3: Forgetting Cancellation in Select

**What:** `tokio::select!` without a `biased` directive and without checking for cancellation branch first.

**Why:** Can cause shutdown to not trigger until the next event (non-deterministic).

**Do this instead:** Use `biased` and put the cancellation branch first in every `select!` in shutdown-sensitive loops (already done in `ConsumerRunner::run()` and `batch_worker_loop`).

### Anti-Pattern 4: Losing Messages on Rebalance

**What:** Not waiting for worker drain before marking partition as revoked.

**Why:** In-flight messages for the revoked partition are lost (processed as success but never committed).

**Do this instead:** `RebalanceHandler::handle_event()` waits for drain signal from workers before marking partition fully revoked.

---

## Open Questions

1. **Partition-to-Worker mapping:** When revoking a partition, we need to cancel only the workers processing that partition. Current `CancellationToken` is shared across all workers. Need per-partition tokens or worker-to-partition mapping.

2. **Rebalance detection granularity:** The current approach (compare assignment on each `consumer.recv()` iteration) detects changes but not the type (revoked vs assigned). rdkafka's underlying API does expose assignment type -- research needed to see if Rust bindings expose it.

3. **OffsetCommitter abort safety:** When `OffsetCommitter` is aborted during a commit operation, the offset may or may not have been committed to Kafka. Need to verify idempotency of `store_offset` + `commit` calls.

4. **DLQ flush on normal shutdown (non-rebalance):** Currently `WorkerPool::shutdown()` calls `flush_failed_to_dlq()` and then `graceful_shutdown()`. This is correct. Need to ensure `ShutdownCoordinator` doesn't call these again (duplicate DLQ produce).

---

## Sources

- [rdkafka Rust StreamConsumer docs](https://docs.rs/rdkafka/latest/rdkafka/consumer/struct.StreamConsumer.html) -- rebalance via assignment polling
- [tokio CancellationToken docs](https://docs.rs/tokio-util/latest/tokio_util/sync/struct.CancellationToken.html) -- shutdown signaling
- [parking_lotRwLock vs tokio::sync::RwLock](https://docs.rs/parking_lot/latest/parking_lot/struct.RwLock.html) -- when to use which (hot path vs blocking)
- [tokio select biased](https://docs.rs/tokio/latest/tokio/macro.select.html) -- priority ordering for shutdown

---

*Architecture research for: KafPy Lifecycle Management*
*Researched: 2026-04-19*