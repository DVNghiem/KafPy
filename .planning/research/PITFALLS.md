# Pitfalls Research: Shutdown and Rebalance Handling for KafPy

**Domain:** Rust/PyO3 Kafka Consumer Lifecycle -- Graceful Shutdown, Rebalance Events, Partition Ownership
**Researched:** 2026-04-19
**Confidence:** MEDIUM-HIGH -- Based on rdkafka Rust documentation (HIGH), Tokio docs (HIGH), existing KafPy codebase analysis (HIGH), general Kafka consumer patterns (MEDIUM-HIGH)

---

## Executive Summary

KafPy's v1.8 milestone adds graceful shutdown coordination and rebalance event handling. This research catalogs the pitfalls that cause consumer data loss, message duplication, infinite rebalances, and deadlock during these lifecycle transitions. The most critical pitfalls are: premature partition revocation before offset commit, lost in-flight messages on shutdown, and deadlock from circular wait conditions between workers and the dispatcher.

The project already has sound foundations: `OffsetTracker` uses highest-contiguous-offset semantics, `WorkerPool::shutdown()` waits for in-flight completion, and `CancellationToken` with `biased` select ordering is used correctly. The new pitfalls emerge from the interaction between these components when rebalance or shutdown signals arrive mid-processing.

---

## Critical Pitfalls

### Pitfall 1: Premature Partition Revocation Before Offset Commit

**What goes wrong:**
A partition is revoked during a rebalance, but offsets for in-flight messages have not yet been committed to Kafka. When the partition is reassigned to another consumer, those messages are re-processed (duplicate delivery) or skipped (data loss), depending on auto.offset.reset settings.

**Why it happens:**
Kafka's rebalance protocol revokes partitions before the consumer has a chance to commit. The time between revocation and the consumer's graceful commit is the danger window. In KafPy's current architecture, `OffsetCommitter` runs independently with no awareness of rebalance events -- if a rebalance occurs between the committer's interval ticks, offsets may not be committed.

```rust
// Current problem: OffsetCommitter has no rebalance awareness
// If rebalance fires between ticker.tick() and process_ready_partitions():
// - partition is revoked
// - offsets for in-flight messages are still in pending_offsets
// - those offsets are lost from this consumer's perspective
async fn process_ready_partitions(&self) {
    let ready = self.collect_ready_partitions(); // scans all partitions
    for (topic, partition, offset) in ready {
        self.commit_partition(topic, partition, offset).await;
    }
}
```

**Consequences:**
- **Data loss**: If auto.offset.reset=earliest is not set (or wrong), messages are skipped
- **Duplicate processing**: If auto.offset.reset=latest, same messages reprocessed by new owner
- **Consumer lag inflation**: Duplicate reprocessing causes apparent lag

**Prevention:**
1. **Rebalance-aware committer**: Before processing a rebalance event, trigger an immediate commit cycle for all partitions
2. **Store offsets eagerly**: Call `store_offset()` immediately after each `ack()`, not just on interval ticks
3. **Revocation barrier**: On rebalance revocation, pause consumption and wait for inflight to drain AND commit before acknowledging the revocation

```rust
// Recommended: Immediate commit on rebalance signal
impl OffsetCommitter {
    pub fn on_rebalance_revoked(&self, partitions: &[(String, i32)]) {
        // Wake up the committer immediately -- send signal through channel
        for (topic, partition) in partitions {
            self.tracker.force_commit(topic, partition);
        }
        // The rx.changed() branch in run() will fire on next poll
        let _ = self.tx.send(TopicPartition::new(topic.clone(), *partition));
    }
}
```

**Detection:**
- Consumer group offset lag jumps to message batch sizes during rebalances
- Duplicate message IDs in processing logs
- `committed_offset` not advancing for revoked partitions

**Phase to address:**
Phase 33 (Graceful Shutdown Coordinator) -- the `RebalanceHandler` must coordinate with `OffsetCommitter` before allowing revocation to complete.

---

### Pitfall 2: Unsafe Offset Advancement on Revoked Partitions

**What goes wrong:**
After a partition is revoked, the consumer continues to call `ack()` and `record_ack()` on messages from that partition. These offsets are stale -- the partition belongs to another consumer now. The `OffsetTracker` advances its internal state, and on the next rebalance (when partitions are reassigned), the consumer commits offsets it never actually processed.

**Why it happens:**
`PartitionOwnership` is not consulted in the `worker_loop`'s ack path. Messages for revoked partitions continue flowing through the dispatcher until the rebalance pause takes effect. Workers process these messages and call `offset_coordinator.record_ack()`.

```rust
// worker_loop continues processing after revocation until pause takes effect
// These acks are for a partition we no longer own!
ExecutionResult::Ok => {
    retry_coordinator.record_success(&ctx.topic, ctx.partition, ctx.offset);
    queue_manager.ack(&msg.topic, 1);
    offset_coordinator.record_ack(&ctx.topic, ctx.partition, ctx.offset); // DANGEROUS
}
```

**Consequences:**
- **Committing unprocessed offsets**: Consumer commits offset N, but another consumer processed up to N+M -- gap
- **Duplicate processing**: The consumer re-acquires the partition and skips N..N+M messages
- **Offset Tracker corruption**: `pending_offsets` contains offsets from a partition it no longer owns

**Prevention:**
1. **Partition ownership check before ack**: `offset_coordinator.record_ack()` must check `PartitionOwnership::is_owned(topic, partition)` before recording
2. **Mark partition as revoking**: When a partition enters the revoking state, stop recording acks for it
3. **Separate tracking for revoking partitions**: Accumulate pending offsets but do not commit until drain confirmed

```rust
impl OffsetCoordinator for OffsetTracker {
    fn record_ack(&self, topic: &str, partition: i32, offset: i64) {
        // SAFETY: Check partition ownership before recording
        if !self.ownership.is_owned(topic, partition) {
            tracing::warn!(
                topic = topic,
                partition = partition,
                offset = offset,
                "Ignoring ack for partition not owned -- likely revoked"
            );
            return;
        }
        self.ack(topic, partition, offset);
    }
}
```

**Detection:**
- `OffsetTracker::pending_offsets` contains offsets for partitions not in current assignment
- Committed offsets jump backward after rebalance (different consumer took over with lower committed)
- Logs show acks for partitions in "revoking" state

**Phase to address:**
Phase 34 (Rebalance Handling) -- requires `PartitionOwnership` integration with `OffsetTracker`.

---

### Pitfall 3: Race Between Rebalance Callback and Message Processing

**What goes wrong:**
A message is dispatched to a worker at the same moment a rebalance revokes that partition. The worker processes the message and calls `ack()` after the partition has been revoked, corrupting offset state. Alternatively, the worker processes a message for a partition that has been reassigned to it mid-processing.

**Why it happens:**
The `ConsumerDispatcher::run()` loop and the rebalance handler run concurrently. There is no synchronization between message dispatch and rebalance state changes.

```rust
// DISPATCH: message sent to worker
let outcome = dispatcher.send_with_policy_and_signal(msg, policy);

// CONCURRENTLY:
// RebalanceHandler marks partition as revoking
partition_ownership.mark_revoking(topic, partition);

// LATER:
// Worker finishes processing and calls record_ack()
// -- but partition was revoked during processing!
offset_coordinator.record_ack(topic, partition, offset);
```

**Consequences:**
- **Duplicate processing**: If ack is recorded but partition revoked before commit, message reprocessed
- **Offset gap**: If partition reassigned and consumer committed higher offset than what it actually processed
- **Inconsistent state**: `PartitionOwnership` and `OffsetTracker` have conflicting views

**Prevention:**
1. **Atomic rebalance state transitions**: Use compare-and-swap semantics when updating `PartitionOwnership`
2. **In-flight message tracking**: Track which partitions each worker is currently processing
3. **Grace period**: After revocation, allow in-flight messages a grace period to complete before blocking new dispatches for that partition

```rust
// Recommended: mark_revoking uses atomic compare-and-swap
impl PartitionOwnership {
    pub fn mark_revoking(&self, topic: &str, partition: i32) -> bool {
        let mut guard = self.revoking.write();
        // Only mark if not already revoking -- prevents double-marking
        let key = TopicPartitionKey::new(topic, partition);
        if guard.contains(&key) {
            return false; // Already revoking
        }
        guard.insert(key);
        true
    }
}

// Worker checks ownership before recording ack
if partition_ownership.is_owned(topic, partition) {
    offset_coordinator.record_ack(topic, partition, offset);
}
```

**Detection:**
- Intermittent duplicate messages during rebalances
- `OffsetTracker` state inconsistent with Kafka's committed offset
- Worker processing messages from a partition it doesn't own

**Phase to address:**
Phase 34 (Rebalance Handling) -- requires coordination protocol between `RebalanceHandler`, `ConsumerDispatcher`, and `WorkerPool`.

---

### Pitfall 4: Deadlock During Graceful Shutdown

**What goes wrong:**
Workers wait for the dispatcher to send more messages, but the dispatcher is waiting for workers to drain so it can shut down. Neither side proceeds, causing the consumer to hang indefinitely on shutdown.

**Why it happens:**
The shutdown sequence is incorrect: workers are waiting on the dispatcher's channel to close, but the dispatcher waits for workers to complete before closing its side. This creates a classic circular wait.

```
WorkerPool.shutdown() {
    drain_token.cancel()              // Workers should stop accepting new work
    workers.wait_for_idle()          // Workers waiting for queue to drain
                                      // BUT: dispatcher has already stopped producing!
    // DEADLOCK: dispatcher waiting for workers to finish
    //           workers waiting for dispatcher to send more messages
}
```

**Current KafPy code:**
```rust
// WorkerPool::shutdown() - current (simplified)
pub async fn shutdown(&mut self) {
    self.shutdown_token.cancel();         // Workers exit when idle
    self.offset_coordinator.graceful_shutdown();  // Commit offsets
    self.join_set.shutdown().await;      // Wait for workers
    // BUT: ConsumerDispatcher may still be running, holding receivers
}
```

**Consequences:**
- **Infinite hang**: Shutdown never completes, consumer process never exits
- **Force kill required**: Requires SIGKILL to terminate
- **In-flight messages lost**: Even if killed, in-flight messages are lost

**Prevention:**
1. **Correct shutdown order**: Dispatcher stops first, then workers drain, then final commit
2. **Non-circular wait**: Use a `broadcast::Sender` that workers subscribe to for shutdown notification, not channel closure
3. **Timeout with force-abort**: After grace period, force-terminate even if not fully drained

```rust
// Recommended shutdown sequence
impl ShutdownCoordinator {
    pub async fn shutdown(&self) {
        // Phase 1: Stop producing new messages
        self.runner.stop();                    // ConsumerRunner stops
        self.abort_dispatcher();               // Dispatcher task aborts

        // Phase 2: Wait for in-flight to complete (with timeout)
        let drain_result = tokio::time::timeout(
            self.drain_timeout,
            self.workers.drain_inflight()
        ).await;

        if drain_result.is_err() {
            tracing::warn!("Drain timeout exceeded, forcing shutdown");
        }

        // Phase 3: Final commit
        self.flush_retries_to_dlq();
        self.commit_final_offsets();
    }
}
```

**Detection:**
- Consumer shutdown hangs indefinitely (30s+)
- No log messages from workers after shutdown initiated
- `tokio::time::timeout` on shutdown never fires (deadlock, not slow)

**Phase to address:**
Phase 33 (Graceful Shutdown Coordinator) -- must define correct shutdown order and implement timeout with force-abort fallback.

---

### Pitfall 5: Lost In-Flight Messages During Shutdown

**What goes wrong:**
Messages that have been dispatched to workers but not yet acknowledged are lost when shutdown occurs. The dispatcher drops messages that were in-flight at the moment of shutdown.

**Why it happens:**
`WorkerPool::shutdown()` cancels the `CancellationToken`, causing workers to exit immediately when they next check the token (at the `select!` boundary). If a message is being processed (`active_message = Some(msg))`), the worker completes processing and calls `ack()`. But if the shutdown signal arrives between dispatch and processing, the worker may exit without completing the message.

```rust
// Current worker_loop shutdown check
select! {
    Some(msg) = rx.recv() => {
        active_message = Some(msg);
    }
    _ = shutdown_token.cancelled() => {
        // Worker exits HERE if no active_message
        // But if active_message is Some, it completes first
        break;
    }
}

// Problem: If shutdown fires while active_message = Some,
// worker completes processing (correct) BUT:
// 1. If processing succeeds, calls record_ack() -- may not commit before process exits
// 2. If processing fails, calls mark_failed() -- may not flush to DLQ before process exits
```

**Current KafPy code (worker_loop):**
```rust
if shutdown_token.is_cancelled() {
    tracing::info!(worker_id = worker_id, "worker stopped (cancelled after message)");
    worker_pool_state.set_idle(worker_id);
    break;  // Worker exits, potentially losing ack/commit window
}
// Mark worker as idle after processing completes
worker_pool_state.set_idle(worker_id);
```

The `break` happens immediately after the message completes, but the `worker_loop` function returns while the async block that spawned it may not have fully completed the commit/dispatch chain.

**Consequences:**
- **Message loss**: Messages processed during shutdown are never committed
- **Duplicate on restart**: On restart, Kafka re-delivers uncommitted messages
- **DLQ entries lost**: Failed messages in-flight at shutdown are not flushed to DLQ

**Prevention:**
1. **Drain before cancel**: Shutdown must wait for all `active_message` processing to complete AND commit
2. **Graceful drain notification**: Workers must receive explicit drain signal, not just cancellation
3. **Commit-before-exit**: `OffsetCommitter` must be triggered for all in-flight offsets before workers exit

```rust
// Recommended: drain with explicit completion tracking
impl WorkerPool {
    pub async fn drain_inflight(&self) {
        // 1. Signal workers to stop accepting new work (don't cancel yet)
        self.drain_token.cancel();

        // 2. Wait for all workers to reach idle state
        loop {
            let all_idle = self.worker_states.values()
                .all(|s| matches!(s, WorkerStatus::Idle));
            if all_idle {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // 3. Now safe to cancel and commit
        self.shutdown_token.cancel();
        self.offset_coordinator.graceful_shutdown();
    }
}
```

**Detection:**
- Messages re-delivered after consumer restart
- DLQ not populated with messages that failed during shutdown
- Offset commits missing messages that were processed at shutdown time

**Phase to address:**
Phase 33 (Graceful Shutdown Coordinator) -- requires drain-before-cancel sequence and explicit completion tracking.

---

### Pitfall 6: Boolean Flag Soup vs Explicit State Machines

**What goes wrong:**
Lifecycle state is encoded as multiple boolean flags (`is_shutting_down`, `is_draining`, `is_revoking`, `is_paused`) scattered across components. The interactions between flags create impossible states (e.g., `is_shutting_down=true` AND `is_revoking=true` AND `is_paused=false`), leading to incorrect behavior.

**Why it happens:**
Boolean flags are easy to add individually but their interactions are not modeled. Each new condition adds a flag, leading to 2^N possible states.

```rust
// Boolean soup example
struct WorkerState {
    is_running: bool,
    is_shutting_down: bool,
    is_draining: bool,
    is_paused: bool,
    is_revoking: bool,
}

// Impossible states:
// - is_shutting_down=true AND is_paused=false (should pause on shutdown?)
// - is_revoking=true AND is_draining=false (should drain on revoke?)
```

**Current KafPy state:**
From `STATE.md`: "Partition ownership state: assigned / paused / draining / revoked" -- this is already an enum state machine design decision. The risk is implementing it as flags instead.

**Consequences:**
- **Impossible states**: Components enter states that should be unreachable
- **Non-deterministic behavior**: Same input produces different outputs depending on flag order
- **Race conditions**: Flags checked in different order in different code paths

**Prevention:**
Use explicit state enums that make illegal states unrepresentable:

```rust
// CORRECT: Enum state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleState {
    /// Normal operation
    Running,
    /// Shutdown in progress, no new dispatches
    Draining,
    /// Rebalance in progress for specific partitions
    Revoking {
        partitions: Vec<(String, i32)>,
    },
    /// Partition revoked but not yet drained
    PartiallyRevoked,
    /// Shutdown complete, finalizing
    Finalizing,
    /// All done
    Done,
}

// Transitions are explicit and enforced
impl LifecycleState {
    pub fn transition_to(self, next: Self) -> Result<(), InvalidTransition> {
        match (self, next) {
            (Running, Draining) => Ok(()),
            (Running, Revoking { .. }) => Ok(()),
            (Draining, Finalizing) => Ok(()),
            (Revoking { .. }, PartiallyRevoked) => Ok(()),
            // Invalid transitions caught at compile time
            (Done, _) => Err(InvalidTransition),
            _ => Err(InvalidTransition),
        }
    }
}
```

**Detection:**
- Bugs where shutdown and rebalance interact incorrectly
- State checks that don't make sense (e.g., "if not shutting_down and not paused")
- Flags checked in different order in different places

**Phase to address:**
Phase 33 (Graceful Shutdown Coordinator) -- define the `LifecycleState` enum first before implementing any shutdown/rebalance logic.

---

### Pitfall 7: Mixing Lifecycle State with Business Logic

**What goes wrong:**
Business logic (retry policy, DLQ routing, backpressure) is conditioned on lifecycle flags. This creates tight coupling: changing retry policy requires understanding shutdown behavior, or shutdown behavior changes unexpectedly when retry policy is modified.

**Why it happens:**
Lifecycle checks are scattered throughout business logic code:

```rust
// Mixed concerns in worker_loop
if should_retry {
    tokio::time::sleep(delay).await;
    // But what if shutdown started during the sleep?
    active_message = Some(msg);
    continue;
}

if shutdown_token.is_cancelled() {
    // Exit now, but we have a pending retry!
    break;
}
```

**Consequences:**
- **Test complexity**: Business logic tests must mock lifecycle state
- **Fragility**: Changes to lifecycle handling break business logic
- **Hidden dependencies**: It's unclear which code depends on which lifecycle flags

**Prevention:**
1. **Lifecycle as a membrane**: Lifecycle state gates entry to business logic, not execution within it
2. **Separation of concerns**: `RebalanceHandler` manages lifecycle; `WorkerPool` manages processing; `RetryCoordinator` manages retry
3. **Explicit lifecycle transitions**: Business logic responds to lifecycle events, not flags

```rust
// CORRECT: Lifecycle gates dispatch, not processing
impl ConsumerDispatcher {
    async fn dispatch_loop(&self) {
        while let Some(msg) = self.message_rx.recv().await {
            // Lifecycle check: reject new dispatches if not Running
            match self.lifecycle.state() {
                LifecycleState::Running => {}
                LifecycleState::Draining => {
                    tracing::debug!("Draining, dropping message");
                    continue;
                }
                LifecycleState::Revoking { partitions } => {
                    if partitions.contains(&(msg.topic.clone(), msg.partition)) {
                        tracing::debug!("Partition revoking, dropping message");
                        continue;
                    }
                }
                _ => continue,
            }
            // Business logic below -- no lifecycle checks
            self.send_to_worker(msg).await;
        }
    }
}
```

**Detection:**
- `shutdown_token.is_cancelled()` checks inside business logic (worker_loop processing)
- Retry scheduling happens without checking if partition is being revoked
- DLQ routing checks lifecycle state

**Phase to address:**
Phase 33 (Graceful Shutdown Coordinator) -- lifecycle state should be owned by `ShutdownCoordinator`, not checked in business logic paths.

---

### Pitfall 8: Heavy Rebalance Callbacks Causing Rebalance Storms

**What goes wrong:**
The rebalance handler does too much work synchronously in the detection path (offset commits, worker cancellation, queue draining). This causes the consumer to appear unresponsive to the Kafka broker, triggering more rebalances (the consumer is "slow" from Kafka's perspective).

**Why it happens:**
Kafka's consumer group protocol has a rebalance timeout. If a consumer doesn't respond to a rebalance event within `session.timeout.ms`, it is kicked from the group and a new rebalance begins. Heavy callback work extends the window.

```rust
// TOO HEAVY for rebalance callback
async fn on_rebalance(&self, event: RebalanceEvent) {
    // This ALL happens before responding to Kafka!
    self.commit_all_offsets().await;           // Slow: network I/O
    self.flush_all_dlq().await;                // Slow: more I/O
    self.drain_all_workers().await;            // Slow: waits for processing
    self.update_ownership(event.new_assignment);// Fast: memory only
    // By now, session.timeout might have fired!
}
```

**Consequences:**
- **Rebalance loop**: Consumer repeatedly kicked from group, causing continuous rebalances
- **Consumer lag**: Messages not processed during rebalance storms
- **Group instability**: Consumer group never stabilizes

**Prevention:**
1. **Minimal revocation response**: Mark partition as revoking, signal workers -- do this FAST
2. **Async commit after acknowledgment**: Commit offsets AFTER Kafka has acknowledged the rebalance
3. **Background processing**: Do heavy work (DLQ flush, final offset commit) in background after rebalance completes

```rust
// FAST revocation response
async fn handle_rebalance(&self, event: RebalanceEvent) {
    // Phase 1: FAST -- update ownership, signal workers (in-memory)
    for (topic, partition) in &event.newly_revoked {
        self.partition_ownership.mark_revoking(topic, partition);
        self.pause_partition(topic); // rdkafka pause() is fast
    }
    // Acknowledge to Kafka immediately -- we're within session.timeout

    // Phase 2: BACKGROUND -- do heavy work after rebalance acknowledged
    tokio::spawn(async move {
        self.commit_revoked_offsets(event.newly_revoked).await;
        self.drain_revoked_partitions(event.newly_revoked).await;
    });
}
```

**Detection:**
- Multiple rebalance events in logs within short time window
- `session.timeout` warnings in Kafka broker logs
- Consumer lag spikes during rebalances

**Phase to address:**
Phase 34 (Rebalance Handling) -- define fast-path revocation response separate from background reconciliation.

---

### Pitfall 9: In-Flight Messages on Revoked Partition That Cannot Complete

**What goes wrong:**
A partition is revoked while messages are in-flight. Some messages are still being processed by workers. The rebalance requires acknowledging revocation, but the in-flight messages cannot complete (e.g., the Python handler is stuck on an I/O operation with no timeout).

**Why it happens:**
There is no mechanism to forcibly complete or cancel in-flight messages when a partition is revoked. The `CancellationToken` is per-worker, not per-partition, so canceling the token cancels all workers, not just those processing revoked partitions.

```rust
// Worker has 10 messages in-flight for partition 0
// Partition 0 is revoked
// RebalanceHandler: "please acknowledge revocation"
// What happens to the 10 in-flight messages?

// Current options:
// 1. Wait indefinitely -- rebalance never completes
// 2. Drop messages -- data loss
// 3. Let them complete, then revoke -- long delay in rebalance
```

**Consequences:**
- **Rebalance deadlock**: Cannot acknowledge revocation until all in-flight complete
- **Data loss**: If forced to acknowledge, in-flight messages are lost
- **Stuck consumer**: If handler hangs, rebalance never completes

**Prevention:**
1. **Per-partition cancellation**: Map partitions to workers, cancel only affected workers on revocation
2. **Handler timeout**: Python handlers must have a timeout -- if they exceed it, they are canceled
3. **Drain deadline**: After a grace period, force-complete with whatever state exists

```rust
// Recommended: per-partition worker assignment
struct PartitionWorkerMap {
    // partition -> worker_id
    mapping: HashMap<(String, i32), usize>,
    // worker_id -> active partitions
    reverse: HashMap<usize, Vec<(String, i32)>>,
}

impl PartitionWorkerMap {
    fn revoke_partition(&self, topic: &str, partition: i32) -> Vec<usize> {
        let worker_id = self.mapping.remove(&(topic.to_string(), partition)).unwrap();
        self.reverse.get_mut(&worker_id).map(|v| v.retain(|p| p != &(topic.to_string(), partition)));
        vec![worker_id] // Return affected workers
    }
}

// On revocation: cancel only workers processing that partition
let affected_workers = partition_map.revoke_partition(topic, partition);
for worker_id in affected_workers {
    self.worker_tokens[worker_id].cancel(); // Only cancels one worker
}
```

**What to do with messages that can't complete within deadline:**

Option A: **Commit up to last known good offset** (best effort)
```rust
// Record the last successfully processed offset before revocation
// On rebalance, commit up to that offset
let last_known_good = offset_tracker.last_successful_offset(topic, partition);
runner.store_offset(topic, partition, last_known_good).await;
```

Option B: **Drain to DLQ with partial batch**
```rust
// Flush partial batch to DLQ -- messages retain original offset metadata
// New consumer can pick up from committed offset
```

Option C: **Accept data loss with clear documentation**
```rust
// If graceful_shutdown_timeout is exceeded, acknowledge revocation anyway
// Messages in-flight are lost -- document this as a known limitation
```

**Phase to address:**
Phase 34 (Rebalance Handling) -- requires per-partition cancellation and handler timeout enforcement.

---

## Moderate Pitfalls

### Pitfall 10: `OffsetCommitter` Running Independently Without Shutdown Awareness

**What goes wrong:**
`OffsetCommitter::run()` is a Tokio task that runs indefinitely. When `WorkerPool::shutdown()` completes, the committer task is still running. It continues committing offsets for partitions that no longer have active workers, and may interfere with rebalance operations.

**Why it happens:**
Current `pyconsumer.rs` spawns the committer task but never stores a handle to abort it. The committer's `run()` method only exits when the watch channel closes -- but nobody closes it on shutdown.

```rust
// Current: committer runs forever
tokio::spawn(async move {
    committer.run(rx).await; // rx never closed on shutdown
});
```

**Prevention:**
Store `JoinHandle` for committer task and abort it during shutdown:
```rust
struct ConsumerState {
    committer_handle: Option<tokio::task::JoinHandle<()>>,
}

impl ConsumerState {
    pub fn stop(&self) {
        if let Some(handle) = self.committer_handle.take() {
            handle.abort();
        }
    }
}
```

**Phase to address:**
Phase 33 (Graceful Shutdown Coordinator) -- `ShutdownCoordinator` must own and abort all spawned tasks.

---

### Pitfall 11: `BatchAccumulator` Not Drained on Rebalance Revocation

**What goes wrong:**
`BatchAccumulator` holds messages per-partition for performance. When a partition is revoked mid-batch, the accumulated messages for that partition are lost. The messages were dispatched to the accumulator but never processed.

**Why it happens:**
`batch_worker_loop` handles `shutdown_token.cancelled()` by flushing all accumulators, but it has no awareness of rebalance. When a partition is revoked, the accumulator for that partition is not flushed.

```rust
// batch_worker_loop -- only handles full shutdown, not rebalance
tokio::select! {
    _ = shutdown_token.cancelled() => {
        // Flushes ALL accumulators on full shutdown
        let partitions = accumulator.flush_all();
        // ... process and ack
    }
    // No rebalance-specific handling!
}
```

**Prevention:**
Add rebalance event handling to `batch_worker_loop`:
```rust
tokio::select! {
    _ = rebalance_tx.recv() => {
        // Flush accumulators for revoked partitions
        let revoked = partition_ownership.revoking_partitions();
        for (topic, partition) in revoked {
            if let Some(batch) = accumulator.flush_partition(partition) {
                // Process and ack, or mark for re-queue
            }
        }
    }
}
```

**Phase to address:**
Phase 35 (Integration) -- batch accumulator must integrate with `PartitionOwnership`.

---

### Pitfall 12: Forgetting `biased` in `tokio::select!` for Rebalance Handling

**What goes wrong:**
`tokio::select!` without `biased` chooses branches randomly when multiple are ready. On a busy consumer, the rebalance branch may never be selected, causing rebalance events to be delayed indefinitely.

**Why it happens:**
`select!` without `biased` uses random fairness. If both a rebalance event and a message arrive simultaneously, the rebalance may not be processed until many message iterations.

**Current correct usage in KafPy:**
`ConsumerRunner::run()` uses `biased`:
```rust
tokio::select! {
    biased;
    _ = shutdown_rx.recv() => { break; }
    message_result = consumer.recv() => { /* ... */ }
}
```

**Prevention:**
Always use `biased` in shutdown/rebalance-sensitive `select!` loops:
```rust
tokio::select! {
    biased;
    _ = shutdown_token.cancelled() => { /* shutdown first */ }
    _ = rebalance_rx.recv() => { /* rebalance second */ }
    msg = rx.recv() => { /* messages last */ }
}
```

**Phase to address:**
Phase 33/34 -- audit all `select!` usage for shutdown and rebalance paths.

---

## Minor Pitfalls

### Pitfall 13: Thread Safety of `parking_lot::Mutex` in Async Context

**What goes wrong:**
`parking_lot::Mutex` is used in async contexts (e.g., `PartitionOwnership::assignment`). When an async task holds the mutex across an `.await` point, other async tasks are blocked. Since `parking_lot::Mutex` is not async-aware, it blocks the Tokio thread, preventing other tasks from running.

**Why it happens:**
`parking_lot::Mutex` is not designed for async -- it blocks the thread, not just the task. `tokio::sync::Mutex` yields the task, allowing other tasks to run.

**Current KafPy usage:**
```rust
// PartitionOwnership uses parking_lot::RwLock
pub struct PartitionOwnership {
    assignment: parking_lot::RwLock<HashMap<String, Vec<i32>>>,
    revoking: parking_lot::RwLock<HashMap<String, Vec<i32>>>,
}

// BUT: if async code holds these across .await:
// async fn handle_rebalance(&self) {
//     let guard = self.assignment.read(); // Mutex held
//     self.process_assignment(guard).await; // DANGEROUS: other tasks blocked
// }
```

**Prevention:**
Use `tokio::sync::RwLock` for async-accessed state:
```rust
pub struct PartitionOwnership {
    // Use tokio RwLock for async context safety
    assignment: tokio::sync::RwLock<HashMap<String, Vec<i32>>>,
    revoking: tokio::sync::RwLock<HashMap<String, Vec<i32>>>,
}
```

**Phase to address:**
Phase 34 (Rebalance Handling) -- if `PartitionOwnership` is accessed from async contexts, use `tokio::sync::RwLock`.

---

### Pitfall 14: No Timeout on `store_offset` + `commit` During Shutdown

**What goes wrong:**
During `graceful_shutdown()`, `store_offset()` and `commit()` are called without timeouts. If the Kafka broker is slow or unreachable, shutdown hangs indefinitely even though `ShutdownCoordinator` has a drain timeout.

**Why it happens:**
```rust
// graceful_shutdown -- no timeout on individual operations
impl OffsetTracker {
    fn graceful_shutdown(&self) {
        for (topic, partition) in self.all_partitions() {
            runner_arc.store_offset(topic, partition, offset); // May hang!
            runner_arc.commit(); // May hang!
        }
    }
}
```

**Prevention:**
Use `tokio::time::timeout` for each commit operation:
```rust
async fn commit_with_timeout(&self, topic: &str, partition: i32, offset: i64) {
    let store_result = tokio::time::timeout(
        Duration::from_secs(5),
        self.runner.store_offset(topic, partition, offset)
    ).await;

    if store_result.is_err() {
        tracing::warn!("store_offset timed out for {}:{}", topic, partition);
    }
    // Similar for commit()
}
```

**Phase to address:**
Phase 33 (Graceful Shutdown Coordinator) -- add timeouts to all I/O operations during shutdown.

---

### Pitfall 15: `has_terminal` Blocking Commit Indefinitely on Rebalance

**What goes wrong:**
`OffsetTracker::should_commit()` returns `false` when `has_terminal=true` for a partition. During rebalance, if a terminal failure occurred on a partition that is being revoked, the partition's committed offset never advances. On re-acquisition, the consumer starts from the old committed offset, potentially skipping messages.

**Why it happens:**
```rust
// offset_tracker.rs
pub fn should_commit(&self, topic: &str, partition: i32) -> bool {
    guard.get(&key).is_some_and(|s| {
        if s.has_terminal {
            return false; // Terminal failures block commit FOREVER
        }
        s.pending_offsets.contains(&(s.committed_offset + 1))
    })
}
```

**Prevention:**
Commit all non-failed offsets regardless of `has_terminal` during shutdown:
```rust
fn graceful_shutdown(&self) {
    for (topic, partition) in self.all_partitions() {
        // Always commit the highest contiguous, even if has_terminal
        if let Some(offset) = self.highest_contiguous(topic, partition) {
            // ... commit regardless of has_terminal state
        }
    }
}
```

**Phase to address:**
Phase 33 (Graceful Shutdown Coordinator) -- override `has_terminal` gating during shutdown commit.

---

## Phase-Specific Warning Matrix

| Pitfall | Phase 33 (Shutdown) | Phase 34 (Rebalance) | Phase 35 (Integration) |
|---------|---------------------|---------------------|----------------------|
| Premature revocation before commit | **PRIMARY** -- commit before revoke | Coordinates with commit trigger | Wired to lifecycle |
| Unsafe offset on revoked partition | Add ownership check in record_ack | Implement is_owned guard | Validate integration |
| Race between rebalance and dispatch | N/A | **PRIMARY** -- atomic state transitions | Test with concurrent events |
| Deadlock during shutdown | **PRIMARY** -- correct shutdown order | N/A | Stress test |
| Lost in-flight on shutdown | **PRIMARY** -- drain-before-cancel | N/A | Integration test |
| Boolean flag soup | Define LifecycleState enum | Use across all rebalance code | Enforce at boundaries |
| Mixing lifecycle and business | Enforce separation | Enforce at boundaries | Validate no cross-talk |
| Heavy rebalance callbacks | N/A | **PRIMARY** -- fast-path + background | Monitor rebalance duration |
| In-flight on revoked that can't complete | N/A | **PRIMARY** -- per-partition cancel + timeout | DLQ integration |
| OffsetCommitter independent | **PRIMARY** -- abort on shutdown | N/A | N/A |
| BatchAccumulator not drained on revoke | N/A | Add revoke handling | **PRIMARY** |
| Missing `biased` select | Audit shutdown paths | Audit rebalance paths | N/A |
| Thread safety in async | N/A | Use tokio::sync primitives | Validate Send+Sync |
| No timeout on commit | **PRIMARY** -- add timeouts | N/A | N/A |
| has_terminal blocking commit | **PRIMARY** -- override on shutdown | N/A | N/A |

---

## Warning Signs to Log

Add structured log events for these conditions to aid debugging:

```rust
// On rebalance event received
tracing::warn!(
    event_type = %event.type(),
    newly_revoked = ?event.newly_revoked(),
    newly_owned = ?event.newly_owned(),
    "Rebalance event received"
);

// On partition enter revoking state
tracing::info!(
    topic = %topic,
    partition = partition,
    pending_offsets = ?pending_count,
    inflight = ?inflight_count,
    "Partition entering revoking state"
);

// On shutdown drain timeout
tracing::error!(
    elapsed_ms = elapsed.as_millis(),
    still_active = still_active_count,
    "Shutdown drain timeout exceeded"
);

// On ack for non-owned partition (warning, not error -- expected during rebalance)
tracing::warn!(
    topic = %topic,
    partition = partition,
    offset = offset,
    "Ack received for partition not currently owned"
);
```

---

## Sources

- [rdkafka Rust consumer rebalance documentation](https://docs.rs/rdkafka/latest/rdkafka/consumer/trait.Consumer.html) -- assignment/rebalance callbacks (HIGH)
- [rdkafka StreamConsumer](https://docs.rs/rdkafka/latest/rdkafka/consumer/struct.StreamConsumer.html) -- polling-based rebalance detection (HIGH)
- [tokio CancellationToken](https://docs.rs/tokio-util/latest/tokio_util/sync/struct.CancellationToken.html) -- shutdown signaling (HIGH)
- [tokio select! biased](https://docs.rs/tokio/latest/tokio/macro.select.html) -- priority ordering (HIGH)
- [parking_lot Mutex vs tokio sync Mutex](https://docs.rs/parking_lot/latest/parking_lot/struct.Mutex.html) -- async context safety (HIGH)
- [Kafka consumer group rebalance protocol](https://kafka.apache.org/documentation/#impl_rebalance) -- rebalance timeout semantics (MEDIUM)
- [Apache Kafka Javadoc RebalanceCallback](https://kafka.apache.org/10/javadoc/org/apache/kafka/clients/consumer/ConsumerRebalanceListener.html) -- Java semantics reference (MEDIUM)
- [Confluent blog:_rebalance handling](https://www.confluent.io/blog/kafka-rebalance-listener-java-tutorial-2/) -- common patterns (MEDIUM)

---

## Open Questions Requiring Deeper Research

1. **rdkafka rebalance detection granularity**: Does rdkafka expose whether a rebalance event is an assignment vs revocation at the Rust level, or only the resulting assignment diff?

2. **OffsetCommitter abort safety**: When `OffsetCommitter` is aborted mid-commit, the offset may be partially committed. Need to verify idempotency of `store_offset` + `commit` with Kafka.

3. **BatchAccumulator + rebalance interaction**: Does the current `flush_all()` on shutdown correctly handle per-partition revocation, or does it need a targeted `flush_partition()` approach?

4. **Python handler timeout enforcement**: Can Python handlers specify timeouts, or does KafPy need to enforce them from the Rust side via `tokio::time::timeout` on the `spawn_blocking` call?

---

*Pitfalls research for: KafPy v1.8 Graceful Shutdown & Rebalance Handling*
*Researched: 2026-04-19*
