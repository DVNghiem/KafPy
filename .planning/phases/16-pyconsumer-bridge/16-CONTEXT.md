# Phase 16: PyO3 Bridge - Context

**Gathered:** 2026-04-17
**Status:** Ready for planning

<domain>
## Phase Boundary

Wire `OffsetTracker` + `OffsetCommitter` into `PyConsumer.start()`, spawn `OffsetCommitter` as a Tokio task, and ensure `ConsumerDispatcher` passes `Arc<dyn OffsetCoordinator>` to `WorkerPool`.

**Depends on:** Phase 15 (WorkerPool Integration), Phase 12 (OffsetCommitter), Phase 13 (ConsumerRunner store_offset)

</domain>

<decisions>
## Implementation Decisions

### D-01: `OffsetCommitter` lives in `PyConsumer.start()` — spawned via `tokio::spawn`
- `OffsetCommitter::new(runner, Arc<Clone<OffsetTracker>>, config)` is constructed in `start()`
- The `.run()` future is wrapped in `tokio::spawn` before `pool.run().await`
- `OffsetCommitter` is not stored long-term — the Tokio task handle is dropped after spawn (owned by the runtime)
- Rationale: `ConsumerRunner::run()` returns a `Receiver` not a task handle; `OffsetCommitter` is a separate concern that needs its own task

### D-02: `OffsetTracker.set_runner()` called before `pool.run().await`
- `offset_tracker.set_runner(runner_arc)` called after `ConsumerRunner` creation, before `WorkerPool::new()`
- `runner_arc` is `Arc<ConsumerRunner>` cloned from the runner used by dispatcher
- Rationale: `graceful_shutdown()` needs the runner to call `store_offset` + `commit`; `set_runner()` is the wiring point

### D-03: `OffsetCommitter` receives `Arc<ConsumerRunner>` and `Arc<OffsetTracker>`
- Both are `Arc` clones passed at construction time
- `OffsetCommitter` holds `Arc<ConsumerRunner>` for `store_offset` + `commit` calls
- `OffsetCommitter` holds `Arc<OffsetTracker>` for reading `should_commit` / `highest_contiguous`
- Rationale: `OffsetCommitter::process_ready_partitions()` needs to interrogate the tracker

### D-04: `OffsetCommitter` uses watch channel for commit signals (from Phase 12)
- `OffsetCommitter::run()` takes `watch::Receiver<TopicPartition>` — already designed this way in Phase 12
- The sender side is held by... (TBD — may need a shared commit signal channel registered in dispatcher or tracker)
- Rationale: Phase 12 already designed the committer to signal via watch; we wire the sender into the dispatcher/tracker flow

### D-05: `process_ready_partitions()` placeholder is replaced with real logic
- Phase 12 left `process_ready_partitions()` with a `vec![]` placeholder (line 166)
- Real implementation scans all partitions via `OffsetTracker::all_partitions()` and calls `should_commit()` per pair
- Commit signal from dispatcher: the watch channel signals WHICH partition triggered the cycle; the committer then scans ALL ready partitions
- Rationale: The committer needs to commit all partitions that are ready, not just the one that triggered the signal

### D-06: BRIDGE-03 already satisfied — no changes needed to dispatcher or WorkerPool wiring
- `pyconsumer.rs` line 126 already passes `offset_tracker.clone()` to `WorkerPool::new()`
- `WorkerPool` already stores `Arc<dyn OffsetCoordinator>` and calls `record_ack`/`mark_failed`/`graceful_shutdown`
- No changes needed to `ConsumerDispatcher` or `WorkerPool` for BRIDGE-03
- Rationale: The wiring was done in Phase 15 when integrating the coordinator into WorkerPool

</decisions>

<canonical_refs>
## Canonical References

### Phase 12 (OffsetCommitter)
- `src/coordinator/commit_task.rs` — `OffsetCommitter::new()`, `OffsetCommitter::run()`, `process_ready_partitions()` placeholder at line 166
- `CommitConfig` with `commit_interval_ms` and `commit_max_messages`

### Phase 13 (ConsumerRunner store_offset)
- `src/consumer/runner.rs` — `ConsumerRunner::store_offset()` and `ConsumerRunner::commit()`

### Phase 15 (WorkerPool Integration)
- `src/worker_pool/mod.rs` — `WorkerPool::new()` takes `Arc<dyn OffsetCoordinator>`
- `src/coordinator/offset_tracker.rs` — `set_runner()`, `all_partitions()`, `graceful_shutdown()`

### Current PyO3 Wiring
- `src/pyconsumer.rs` — `start()` method (lines 58-145), already creates `offset_tracker` and passes to `WorkerPool` at line 126

</canonical_refs>

<specifics>
## Specific Ideas

### What OffsetCommitter.run() actually needs
The current Phase 12 `OffsetCommitter::run()` listens on a watch channel. But the watch channel is empty in the current code — nobody sends to it yet. The Phase 16 wiring needs to:
1. Create a `watch::channel<TopicPartition>` shared between dispatcher (sender) and committer (receiver)
2. When the dispatcher dispatches a message successfully, it sends the `TopicPartition` on the watch channel
3. The committer receives this, calls `process_ready_partitions()`, which then scans all partitions via `all_partitions()`

OR alternatively:
- The committer runs on a timer interval and scans `all_partitions()` directly — no watch channel needed
- This is simpler but less responsive

### Open question: who owns the watch channel sender?
If the committer only runs on intervals, no watch channel is needed. The committer can just poll `all_partitions()` on each tick.

**If watch channel is used:** The dispatcher needs a handle to the sender. This could be stored in `ConsumerDispatcher` or passed at construction time.

**If interval-only:** The committer just calls `process_ready_partitions()` on each tick, which scans all partitions via `all_partitions()`. No watch channel needed.

### BRIDGE-01 truth: "PyConsumer creates and wires OffsetTracker + OffsetCommitter"
The current code creates `offset_tracker` but not `OffsetCommitter`. Phase 16 needs to add `OffsetCommitter` construction and spawning.

### BRIDGE-02 truth: "OffsetCommitter is spawned as a Tokio task via ConsumerRunner::run()"
This phrasing is ambiguous — `ConsumerRunner::run()` returns a channel, not a task. What it likely means is: "OffsetCommitter runs as a Tokio task alongside the ConsumerDispatcher/WorkerPool tasks". The spawning happens via `tokio::spawn` in `start()`.

### BRIDGE-03 already done
`pyconsumer.rs` line 126: `offset_tracker.clone()` passed to `WorkerPool::new()` ✓

</specifics>

<deferred>
## Deferred Ideas

- The watch channel sender ownership question — interval-only committer avoids this entirely
- Whether `process_ready_partitions()` should use the watch signal or just scan all partitions on interval tick

</deferred>
