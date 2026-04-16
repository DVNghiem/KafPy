# Phase 13: ConsumerRunner store_offset - Context

**Gathered:** 2026-04-17
**Status:** Ready for planning

<domain>
## Phase Boundary

Add `store_offset()` method to `ConsumerRunner` that calls rdkafka's `store_offset` for two-phase manual offset management. Disable `enable.auto.offset.store` in config. Implement the two-phase guard (`store_offset` then `commit`) that underpins at-least-once delivery.

</domain>

<decisions>
## Implementation Decisions

### COMMIT-01/02: store_offset + two-phase guard
- **D-01:** `ConsumerRunner::store_offset(topic, partition, offset)` — async fn that calls rdkafka `store_offset` via `tokio::task::spawn_blocking` to avoid blocking the async executor
- **D-02:** Two-phase guard: `OffsetCommitter::commit_partition` checks `highest_contiguous > committed_offset` before calling `store_offset` + `commit`; skips both if condition not met
- **D-03:** `store_offset` returns `Result<(), ConsumerError>`; errors do not advance stored offset — next cycle retries

### CONFIG-01/02: Auto-commit and auto-offset-store disabled
- **D-04:** `ConsumerConfig` already has `enable_auto_commit: bool` defaulting to `false` (CONFIG-01 satisfied) — no change needed
- **D-05:** Add `enable_auto_offset_store: bool` field to `ConsumerConfig`, builder, and `into_rdkafka_config()`, defaulting to `false` (CONFIG-02)
- **D-06:** Set `enable.auto.offset.store=false` unconditionally in `into_rdkafka_config()` when manual offset management is active

### Integration Points
- **D-07:** `OffsetCommitter::commit_partition` calls `runner.store_offset(topic, partition, offset)` then `runner.commit()` per topic-partition
- **D-08:** `store_offset` on `ConsumerRunner` uses `rdkafka::consumer::Consumer::store_offset` internally

### Claude's Discretion
- Exact rdkafka API call syntax for `store_offset` — researcher to confirm
- Error variant naming for store_offset failures

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase 11 Artifacts
- `src/coordinator/offset_tracker.rs` — `OffsetTracker::highest_contiguous()` and `OffsetTracker::committed_offset()` interface
- `src/coordinator/mod.rs` — Module exports

### Phase 12 Artifacts
- `src/coordinator/commit_task.rs` — `OffsetCommitter::commit_partition()` — where store_offset + commit will be called (lines 178-198)
- `src/coordinator/commit_task.rs` — Phase 13 comment on line 189: "Phase 13 will add runner.store_offset(topic, partition, offset)"

### Existing Code
- `src/consumer/runner.rs` — `ConsumerRunner::commit()` method using `commit_consumer_state` (reference for rdkafka API pattern, lines 113-122)
- `src/consumer/config.rs` — `ConsumerConfig` struct and `into_rdkafka_config()` method (lines 219-271) — add `enable_auto_offset_store` here

### Requirements
- `.planning/REQUIREMENTS.md` — COMMIT-01, COMMIT-02, CONFIG-01, CONFIG-02 requirements

</canonical_refs>

<reuse>
## Existing Code Insights

### Reusable Assets
- `ConsumerRunner::commit()` pattern — `self.consumer.commit_consumer_state(rdkafka::consumer::CommitMode::Async)` — `store_offset` uses similar `self.consumer.store_offset()` pattern
- `parking_lot::Mutex` — already used in Phase 11, `OffsetCommitter` uses it for throttle state

### Established Patterns
- `spawn_blocking` for blocking C library calls — used in `WorkerPool` for Python handler invocation
- `ConsumerError` enum already covers Kafka errors — `store_offset` errors can use same enum

### Integration Points
- `ConsumerRunner` already holds `Arc<StreamConsumer>` — `store_offset` calls `self.consumer.store_offset(topic, partition, offset)`
- `OffsetCommitter::commit_partition(topic, partition, offset)` calls `runner.store_offset()` before `runner.commit()`

</reuse>

<specifics>
## Specific Ideas

- No specific requirements beyond what requirements/RESEARCH specify — open to standard approaches
- rdkafka `store_offset()` + `commit_consumer_state()` two-phase: store is fast/in-memory, commit is network round-trip

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>
