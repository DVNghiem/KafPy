# Phase 15: WorkerPool Integration - Context

**Gathered:** 2026-04-17
**Status:** Ready for planning

<domain>
## Phase Boundary

Wire `OffsetCoordinator` into `WorkerPool::worker_loop()` — `record_ack` on `ExecutionResult::Ok`, `mark_failed` on `ExecutionResult::Error`/`Rejected`, and `graceful_shutdown` commits highest contiguous offsets before workers exit.

**Depends on:** Phase 13 (ConsumerRunner store_offset), Phase 14 (OffsetCoordinator trait)

</domain>

<decisions>
## Implementation Decisions

### D-01: `graceful_shutdown()` enumeration via `all_partitions()`
- `OffsetTracker::all_partitions() -> Vec<(String, i32)>` will be added
- Returns all topic-partition keys that have ever been registered (i.e., have a `PartitionState` entry)
- `graceful_shutdown()` iterates this set, calls `should_commit(topic, partition)` per pair, and commits if true
- Rationale: No-arg method is cleanest; `all_partitions()` is a thin wrapper over the existing HashMap keys

### D-02: Silent no-op when nothing is ready to commit
- When iterating partitions in `graceful_shutdown()`, skip any where `should_commit()` returns false
- No warning, no error — this is expected when no messages were successfully processed before shutdown
- Rationale: Avoids noisy logs on clean shutdown with no work done

### D-03: `WorkerPool::shutdown()` calls `graceful_shutdown()`
- `WorkerPool::shutdown()` invokes `offset_coordinator.graceful_shutdown()` **before** joining all workers
- Owner (e.g., `Consumer`) calls `pool.shutdown()` — does not need to hold a direct reference to the coordinator
- Rationale: Co-locates shutdown logic with the coordinator owner; owner doesn't need to know coordinator internals

### D-04: `record_ack` — after `queue_manager.ack()`, fire-and-forget
- In `worker_loop`, after `queue_manager.ack(&msg.topic, 1)` succeeds, call `offset_coordinator.record_ack(ctx.topic, ctx.partition, ctx.offset)`
- No blocking, no waiting — `record_ack` is synchronous and fast (just inserts into BTreeSet under Mutex)
- Rationale: Consistent with queue ack ordering; coordinator insert is O(log n) under a parking_lot Mutex — acceptable latency

### D-05: `mark_failed` — after error log, fire-and-forget
- In `worker_loop`, after the error/rejected tracing log, call `offset_coordinator.mark_failed(ctx.topic, ctx.partition, ctx.offset)`
- Same fire-and-forget pattern as `record_ack`
- Rationale: Consistent with D-04; failed tracking is informational — gaps remain until retry succeeds

### D-06: Worker loop structure (Claude's discretion)
- `record_ack` and `mark_failed` calls go inside the existing `match result` block, after the `queue_manager.ack` call (for Ok) and after the tracing log (for Error/Rejected)
- No structural changes needed to `worker_loop` beyond adding 2 calls
- Exact placement: After `queue_manager.ack` in `ExecutionResult::Ok` arm; after tracing in Error/Rejected arms

### Claude's Discretion
The exact line-level placement of `record_ack`/`mark_failed` calls within the match arms — trusted to the executor to place idiomatically.
</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase 13 (ConsumerRunner store_offset)
- `src/coordinator/commit_task.rs` — two-phase `store_offset` + `commit()` guard in `commit_partition`
- `src/consumer.rs` — `ConsumerRunner::store_offset()` method

### Phase 14 (OffsetCoordinator Trait)
- `src/coordinator/offset_coordinator.rs` — `OffsetCoordinator` trait definition
- `src/coordinator/offset_tracker.rs` — `OffsetTracker` implementation + `PartitionState`
- `src/worker_pool/mod.rs` — `WorkerPool` struct + `worker_loop` function (current code)

### Phase 11 (OffsetTracker Core)
- `src/coordinator/offset_tracker.rs` — `ack`, `highest_contiguous`, `should_commit`, `mark_failed`, `committed_offset` methods

### Execution Layer
- `src/python/execution_result.rs` — `ExecutionResult::Ok`, `Error`, `Rejected` enum
- `src/dispatcher/queue_manager.rs` — `ack` method used by worker loop
- `src/python/executor.rs` — `ExecutorOutcome` enum (Ack, Retry, Rejected)

### Requirements
- `.planning/ROADMAP.md` §Phase 15 — success criteria: record_ack on Ok, mark_failed on Error/Rejected, graceful_shutdown commits before exit
- `.planning/REQUIREMENTS.md` §WORKER-04, WORKER-05, WORKER-06 — exact requirement text

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `worker_loop` in `src/worker_pool/mod.rs` already has `offset_coordinator: Arc<dyn OffsetCoordinator>` parameter — just needs calls added
- `ExecutionResult::Ok / Error / Rejected` already matched in the existing `match result` block — add coordinator calls here
- `OffsetTracker` already has all underlying data structures (HashMap<TopicPartitionKey, PartitionState>) — just needs `all_partitions()` added

### Established Patterns
- Fire-and-forget: `queue_manager.ack` is already fire-and-forget after message processing
- Tracing + outcome pattern: errors logged via `tracing::warn!` before any state changes — mark_failed follows same pattern
- Graceful shutdown token: `CancellationToken` pattern already used for worker cancellation — graceful_shutdown called before join

### Integration Points
- `worker_loop` match block (lines ~53-84 in `src/worker_pool/mod.rs`) — record_ack goes after `queue_manager.ack`, mark_failed goes after tracing
- `WorkerPool::shutdown()` (lines ~179-184) — add graceful_shutdown call before `join_set.shutdown().await`
- `OffsetTracker` impl block — add `all_partitions()` method
</code_context>

<specifics>
## Specific Ideas

No specific external references cited during discussion — standard Rust/BTreeSet approach.
</specifics>

<deferred>
## Deferred Ideas

None — all implementation questions stayed within Phase 15 scope.

</deferred>
