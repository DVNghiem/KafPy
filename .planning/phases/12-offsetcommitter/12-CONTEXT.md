# Phase 12: OffsetCommitter - Context

**Gathered:** 2026-04-17
**Status:** Ready for planning

<domain>
## Phase Boundary

Implement `OffsetCommitter` — background Tokio task that receives commit signals via watch channel and executes `store_offset()` + `commit()` for each ready topic-partition. Throttled by `commit_interval_ms`. Handles failure gracefully.

</domain>

<decisions>
## Implementation Decisions

### COMMIT-03: OffsetCommitter Architecture
- **D-01:** `tokio::sync::watch` channel — single-producer (OffsetTracker) sends to single-consumer (OffsetCommitter task)
- **D-02:** Watch channel payload: `(topic: String, partition: i32)` — signal that this topic-partition has a new contiguous offset ready
- **D-03:** `OffsetCommitter` runs as spawned Tokio task, owned by `ConsumerRunner`
- **D-04:** `min_commit_interval` throttle — commits throttled to not happen more frequently than this interval

### COMMIT-04: Commit Execution Order
- **D-05:** Iterate all topic-partitions that have `should_commit() == true`
- **D-06:** For each: call `runner.store_offset(topic, partition, highest_contiguous)` then `runner.commit()`
- **D-07:** Sequential per-partition — not parallelized within a single commit cycle

### COMMIT-05: Duplicate Commit Guard
- **D-08:** `stored_offset` checked before calling `store_offset()` — if `stored_offset >= highest_contiguous`, skip store+commit for that partition

### CONFIG-03/04: Throttle Configuration
- **D-09:** `commit_interval_ms` config — minimum interval between commit cycles (default: 100ms)
- **D-10:** `commit_max_messages` config — trigger commit when total acked messages exceeds this (default: 100), OR interval expires
- **D-11:** Hybrid throttle: commit if BOTH conditions met OR interval expires (interval acts as heartbeat to prevent stalls)

### Error Handling
- **D-12:** On `store_offset()` failure: log error, do NOT retry within same cycle, allow next cycle to retry
- **D-13:** On `commit()` failure: log error, continue to next partition, do NOT block the commit cycle
- **D-14:** Error handling does NOT advance `committed_offset` — only successful `store_offset` + `commit()` advances it

### Graceful Shutdown
- **D-15:** On shutdown signal: drain all pending contiguous offsets for all topic-partitions before exit
- **D-16:** Shutdown waits for in-flight commit cycle to complete before exiting

### Claude's Discretion
- Watch channel buffer size (default 1 is sufficient — only care about latest signal)
- Tokio task spawn priority (normal is fine — commit is not latency-critical)
- Metric/logging verbosity level

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase 11 Artifacts
- `src/coordinator/offset_tracker.rs` — OffsetTracker interface (ack, should_commit, highest_contiguous, mark_failed)
- `src/coordinator/mod.rs` — Module exports

### Milestone Research
- `.planning/research/SUMMARY.md` — Architecture approach, Phase 2 OffsetCommitter section (lines 87-90)
- `.planning/research/STACK.md` — rdkafka store_offset + commit_consumer_state API

### Existing Code
- `src/consumer/runner.rs` — Existing `commit()` method using `CommitMode::Async` (reference for rdkafka API)
- `src/dispatcher/mod.rs` — Module pattern (reference for how to organize a Tokio background task)

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `parking_lot::Mutex` already used in Phase 11 — use same pattern for internal state
- `tokio::task::spawn` pattern from existing code — spawn `OffsetCommitter` as detached task

### Integration Points
- `OffsetCommitter` receives `Arc<OffsetTracker>` and `ConsumerRunner` handle
- `OffsetCommitter` sends commit signals TO `ConsumerRunner` (store_offset + commit)
- Watch channel sender lives in `OffsetTracker` or a wrapper around it

### Established Patterns
- Error handling: return `Result<(), CoordinatorError>` for commit operations
- Module structure mirrors `src/dispatcher/` pattern

</code_context>

<specifics>
## Specific Ideas

- No specific requirements — open to standard approaches
- rdkafka `store_offset()` + `commit_consumer_state()` two-phase: store is fast/in-memory, commit is network round-trip
- Commit throttle uses `Instant` to track last commit time

</specifics>

<deferred>
## Deferred Ideas

- Commit interval tuning (100ms default needs profiling at 10K msg/s) — deferred to load testing
- DLQ routing on repeated `mark_failed` — deferred to v2

</deferred>
