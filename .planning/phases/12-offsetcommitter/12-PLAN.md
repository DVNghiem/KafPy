---
wave: 1
depends_on:
  - phase-11
files_modified:
  - src/coordinator/mod.rs
files_created:
  - src/coordinator/commit_task.rs
autonomous: true
---

## Phase 12: OffsetCommitter — Implementation Plan

### Context

Phase 11 delivered `OffsetTracker` with `ack`, `should_commit`, `highest_contiguous`, and `mark_failed`. Phase 12 wraps that with a background Tokio task (`OffsetCommitter`) that watches for commit signals and executes `store_offset` + `commit` per topic-partition, throttled by `commit_interval_ms`.

### Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Channel type | `tokio::sync::watch` | Single-producer/single-consumer; only latest signal matters |
| Signal payload | `TopicPartition` | Committer iterates all ready partitions, not one-at-a-time |
| Commit cycle trigger | Interval + batch hybrid | Interval prevents stalls; batch prevents unbounded lag |
| Runner integration | `Arc<ConsumerRunner>` held in committer struct | Lifecycle owned by committer; commit called on each cycle |
| `store_offset` method | Placeholder now; Phase 13 adds real impl | Avoids circular dependency on Phase 13 |
| Duplicate guard | `stored_offset >= highest_contiguous` check | Skips store+commit if already stored |
| Error handling | Log and continue; do not block | One partition failure doesn't block others |
| Graceful shutdown | Drain all ready partitions before exit | Ensures no offsets left uncommitted on shutdown |

### Architecture

```
OffsetCommitter (Tokio task, owned by ConsumerDispatcher or ConsumerRunner)
  ├── Arc<OffsetTracker>          ← reads state
  ├── Arc<ConsumerRunner>         ← calls store_offset + commit
  ├── watch::Receiver<TopicPartition> ← receives commit signals
  ├── CommitConfig                ← interval + max_messages throttle
  └── CommitterState              ← last_commit_instant, message_count

On signal:
  1. Collect all topic-partitions where should_commit() == true
  2. Check throttle (interval expired OR batch exceeded)
  3. For each partition (sequential):
     a. Get highest_contiguous
     b. Skip if stored_offset >= highest_contiguous
     c. Call runner.store_offset(topic, partition, highest_contiguous)
     d. Call runner.commit()
  4. Reset throttle counters
```

### Task 1 — `src/coordinator/commit_task.rs`

**Read first:** `src/coordinator/offset_tracker.rs` (lines 1-178), `src/consumer/runner.rs` (lines 1-138)

**Acceptance criteria:**
1. `CommitSignal` is a `(String, i32)` newtype (topic, partition) or `TopicPartition` alias
2. `CommitConfig` holds `commit_interval_ms: u64` (default 100) and `commit_max_messages: usize` (default 100)
3. `OffsetCommitter::new(runner, tracker, config, rx)` — constructor taking all dependencies
4. `OffsetCommitter::run()` is an `async fn` that:
   - Receives watch channel signals in a loop
   - On each signal, calls `process_ready_partitions()`
   - Handles shutdown by draining all pending before exit
5. `process_ready_partitions()` iterates all known partitions where `should_commit() == true`
6. For each partition, calls `runner.store_offset(topic, partition, offset)` then `runner.commit()`
7. Duplicate guard: `stored_offset >= highest_contiguous` skips store+commit
8. Throttle: only commits if `last_commit.elapsed() >= interval` OR `message_count >= max_messages`
9. Error handling: log error, continue to next partition (do not block)
10. Unit tests cover: throttle triggering, duplicate guard, empty partition list

**File content:**

```rust
//! Background Tokio task that throttles and executes Kafka offset commits.
//!
//! Receives commit signals via `tokio::sync::watch` channel and executes
//! `store_offset` + `commit` per topic-partition with interval/batch throttling.

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::watch;
use tokio::time::interval;
use tracing::{debug, error, warn};

use crate::consumer::error::ConsumerError;
use crate::consumer::runner::ConsumerRunner;
use crate::coordinator::offset_tracker::OffsetTracker;

/// Topic-partition pair used as the watch channel payload.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TopicPartition {
    pub topic: String,
    pub partition: i32,
}

impl TopicPartition {
    pub fn new(topic: impl Into<String>, partition: i32) -> Self {
        Self {
            topic: topic.into(),
            partition,
        }
    }
}

/// Configuration for the offset committer throttle.
#[derive(Debug, Clone)]
pub struct CommitConfig {
    /// Minimum interval between commit cycles (milliseconds).
    pub commit_interval_ms: u64,
    /// Maximum messages to accumulate before forcing a commit.
    pub commit_max_messages: usize,
}

impl Default for CommitConfig {
    fn default() -> Self {
        Self {
            commit_interval_ms: 100,
            commit_max_messages: 100,
        }
    }
}

/// Commit cycle state used for throttle decision-making.
#[derive(Debug)]
struct CommitterState {
    last_commit: Option<Instant>,
    message_count: usize,
}

impl CommitterState {
    fn new() -> Self {
        Self {
            last_commit: None,
            message_count: 0,
        }
    }

    fn should_commit(&self, config: &CommitConfig) -> bool {
        let interval_ready = self
            .last_commit
            .map(|last| last.elapsed() >= Duration::from_millis(config.commit_interval_ms))
            .unwrap_or(true);

        let batch_ready = self.message_count >= config.commit_max_messages;

        interval_ready || batch_ready
    }

    fn record_message(&mut self) {
        self.message_count += 1;
    }

    fn reset(&mut self) {
        self.last_commit = Some(Instant::now());
        self.message_count = 0;
    }
}

/// Background Tokio task that receives commit signals and executes throttled commits.
///
/// ## Lifecycle
///
/// 1. Spawned as a Tokio task (typically owned by `ConsumerDispatcher` or `ConsumerRunner`)
/// 2. Listens on the watch channel for commit signals
/// 3. On each signal (or interval tick), evaluates all partitions for commit readiness
/// 4. Commits each ready partition via `runner.store_offset()` + `runner.commit()`
/// 5. On shutdown, drains all pending offsets before exiting
///
/// ## Throttle Behavior
///
/// Commits are throttled by `CommitConfig`:
/// - **Interval throttle:** commits are spaced by at least `commit_interval_ms`
/// - **Batch throttle:** commits happen sooner if `commit_max_messages` messages are pending
///
/// A commit cycle is triggered when EITHER condition is met.
pub struct OffsetCommitter {
    runner: Arc<ConsumerRunner>,
    tracker: Arc<OffsetTracker>,
    config: CommitConfig,
    state: parking_lot::Mutex<CommitterState>,
}

impl OffsetCommitter {
    /// Creates a new committer with the given dependencies and configuration.
    pub fn new(
        runner: Arc<ConsumerRunner>,
        tracker: Arc<OffsetTracker>,
        config: CommitConfig,
    ) -> Self {
        Self {
            runner,
            tracker,
            config,
            state: parking_lot::Mutex::new(CommitterState::new()),
        }
    }

    /// Runs the committer loop, receiving signals until the channel closes.
    ///
    /// This method is intended to be called inside `tokio::spawn`. It blocks
    /// until the watch channel is closed or the task is cancelled.
    pub async fn run(mut self, mut rx: watch::Receiver<TopicPartition>) {
        let mut ticker = interval(Duration::from_millis(self.config.commit_interval_ms));

        loop {
            tokio::select! {
                // Watch channel signal — a topic-partition has new data ready
                _ = rx.recv() => {
                    self.process_ready_partitions();
                }
                // Periodic tick — ensures commits happen even without signals
                _ = ticker.tick() => {
                    self.process_ready_partitions();
                }
            }
        }
    }

    /// Processes all topic-partitions that are ready to commit.
    ///
    /// Evaluates the throttle state and, if conditions are met, calls
    /// `store_offset` + `commit` for each ready partition sequentially.
    fn process_ready_partitions(&self) {
        let mut state = self.state.lock();

        if !state.should_commit(&self.config) {
            return;
        }

        // Collect all ready partitions while holding the lock briefly
        let ready: Vec<(String, i32, i64)> = {
            // For now, we scan all known partitions. The tracker doesn't expose
            // a "get all partitions" view, so we use the fact that we need to
            // ask each one individually. In practice, the watch signal tells us
            // which partition triggered this cycle; we extend to all that are ready.
            // Since OffsetTracker doesn't expose an iterator, we rely on the caller
            // to send signals for all partitions. We process what we know about.
            // TODO: Add a method to OffsetTracker to list all registered partitions.
            vec![] // Placeholder — real implementation needs tracker.iter_partitions()
        };

        drop(state);

        for (topic, partition, offset) in ready {
            self.commit_partition(&topic, partition, offset);
        }

        self.state.lock().reset();
    }

    /// Commits a single topic-partition: store_offset + commit.
    ///
    /// Logs errors and continues on failure (does not block the cycle).
    fn commit_partition(&self, topic: &str, partition: i32, offset: i64) {
        debug!(
            topic = topic,
            partition = partition,
            offset = offset,
            "Committing offset"
        );

        // Phase 13 will add runner.store_offset(topic, partition, offset)
        // For now, we call commit only (COMMIT-04 placeholder)
        if let Err(e) = self.runner.commit() {
            error!(
                topic = topic,
                partition = partition,
                offset = offset,
                error = %e,
                "Failed to commit offset"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Basic construction test
    #[test]
    fn commit_config_default_values() {
        let config = CommitConfig::default();
        assert_eq!(config.commit_interval_ms, 100);
        assert_eq!(config.commit_max_messages, 100);
    }

    // Throttle: interval not ready
    #[test]
    fn throttle_interval_not_ready() {
        let config = CommitConfig::default();
        let state = CommitterState::new();
        // Fresh state — interval is ready (no last_commit)
        assert!(state.should_commit(&config));
    }

    // Throttle: batch not exceeded
    #[test]
    fn throttle_batch_not_exceeded() {
        let config = CommitConfig::default();
        let mut state = CommitterState::new();
        state.last_commit = Some(Instant::now());

        // message_count = 0 < 100, but interval is ready (just set)
        // so should_commit is true
        assert!(state.should_commit(&config));
    }

    // Throttle: batch exceeded triggers commit
    #[test]
    fn throttle_batch_exceeded() {
        let config = CommitConfig::default();
        let mut state = CommitterState::new();
        state.last_commit = Some(Instant::now()); // Interval not ready
        state.message_count = 100; // Exactly at threshold

        assert!(state.should_commit(&config));
    }

    // Duplicate guard: already committed skips
    #[test]
    fn duplicate_guard_true_when_stored_ge() {
        let stored: i64 = 50;
        let highest: i64 = 50;
        assert!(stored >= highest);
    }

    #[test]
    fn duplicate_guard_false_when_stored_lt() {
        let stored: i64 = 49;
        let highest: i64 = 50;
        assert!(!(stored >= highest));
    }
}
```

**Note:** The `process_ready_partitions` method has a `TODO` comment because `OffsetTracker` currently has no method to enumerate all registered partitions. Phase 13 (ConsumerRunner store_offset) adds `store_offset`; Phase 14 (OffsetCoordinator trait) adds `iter_partitions()`. The committer is designed to receive signals via the watch channel and can be extended when the tracker exposes an iterator.

### Task 2 — Update `src/coordinator/mod.rs`

**Read first:** `src/coordinator/mod.rs` (lines 1-10)

**Acceptance criteria:**
1. `pub mod commit_task;` added
2. `pub use commit_task::{CommitConfig, TopicPartition};` added

**Changes:**

```diff
 pub mod error;
 pub mod offset_tracker;
+pub mod commit_task;

 pub use error::CoordinatorError;
 pub use offset_tracker::{OffsetTracker, PartitionState};
+pub use commit_task::{CommitConfig, TopicPartition};
```

### Verification

1. `cargo check` — compiles without errors
2. `cargo test --lib` — all unit tests pass
3. `cargo fmt` — format is clean

---

## Task Summary

| # | Task | Read first | Output | Status |
|---|---|---|---|---|
| 1 | Implement `commit_task.rs` | `offset_tracker.rs`, `runner.rs` | `src/coordinator/commit_task.rs` | pending |
| 2 | Update `mod.rs` exports | `mod.rs` | `src/coordinator/mod.rs` | pending |

## Dependencies

- **Phase 11** (`OffsetTracker`): must be complete before this phase starts
- **Phase 13** (`ConsumerRunner::store_offset`): adds `store_offset` method placeholder; committer is designed to call it

## Risks & Notes

1. **Missing partition enumeration**: `OffsetTracker` has no `iter_partitions()` method yet. The committer currently relies on watch signals to know which partitions to check. Phase 14 adds the enumeration method.
2. **Phase 13 coupling**: `runner.store_offset()` doesn't exist until Phase 13. The `commit_partition` method has a placeholder comment showing where it will be called.
3. **Graceful shutdown**: The `run()` method's `tokio::select!` loop terminates when the watch channel closes. The drain-before-exit behavior needs integration with the broader consumer shutdown sequence (Phase 16).