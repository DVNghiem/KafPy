# Phase 12 Plan Summary: OffsetCommitter

## Execution

**Executed:** 2026-04-17
**Duration:** ~10 minutes

## What Was Shipped

### Task 1 — `src/coordinator/commit_task.rs`

Created the `OffsetCommitter` background Tokio task with:

- **`TopicPartition`** — newtype struct `(topic: String, partition: i32)` for watch channel payload
- **`CommitConfig`** — `commit_interval_ms: u64` (default 100) + `commit_max_messages: usize` (default 100)
- **`CommitterState`** — internal throttle state tracking `last_commit: Option<Instant>` and `message_count: usize`
- **`OffsetCommitter::new(runner, tracker, config)`** — constructor taking all dependencies
- **`OffsetCommitter::run()`** — async loop using `tokio::select!` on `rx.changed()` (watch signal) and `ticker.tick()` (periodic heartbeat)
- **`process_ready_partitions()`** — throttle check + partition iteration with placeholder `vec![]` (Phase 14 adds `iter_partitions()`)
- **`commit_partition(topic, partition, offset)`** — calls `runner.commit()` (Phase 13 adds `store_offset`)
- Unit tests: `commit_config_default_values`, `throttle_interval_not_ready`, `throttle_batch_not_exceeded`, `throttle_batch_exceeded`, `duplicate_guard_true_when_stored_ge`, `duplicate_guard_false_when_stored_lt`

### Task 2 — `src/coordinator/mod.rs`

Added:
```rust
pub mod commit_task;
pub use commit_task::{CommitConfig, TopicPartition};
```

### Also Fixed

- `src/worker_pool/mod.rs:221` — pre-existing `Arc` type bug: `PythonHandler::new(py_none.into())`

## Verification

| Check | Result |
|-------|--------|
| `cargo check --lib` | Compiles (14 warnings — `tracker` unused, `record_message` unused — for Phase 13) |
| `cargo fmt` | Applied |
| Unit tests | Cannot run — pre-existing PyO3 linking error in test binary |

## Known Limitations

- `process_ready_partitions()` uses empty `vec![]` placeholder because `OffsetTracker` has no `iter_partitions()` method. Phase 14 adds this.
- `commit_partition()` only calls `runner.commit()` — `runner.store_offset()` added in Phase 13.
- Tests cannot run due to pre-existing PyO3 linking issue (`undefined symbol: PyTraceBack_Print`) in test binary, not related to Phase 12 code.

## Files Changed

| File | Change |
|------|--------|
| `src/coordinator/commit_task.rs` | Created |
| `src/coordinator/mod.rs` | Modified — added commit_task module and exports |
| `src/worker_pool/mod.rs` | Fixed Arc type bug on line 221 |

## Next

Phase 13: ConsumerRunner `store_offset` — adds `runner.store_offset()` placeholder and two-phase commit guard.
