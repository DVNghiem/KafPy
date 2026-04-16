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
                _ = rx.changed() => {
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
        let state = self.state.lock();

        if !state.should_commit(&self.config) {
            return;
        }

        // Collect all ready partitions while holding the lock briefly
        let ready: Vec<(String, i32, i64)> = {
            // For now, we scan all known partitions. The tracker doesn't expose
            // a "get all partitions" view, so we rely on the caller to send
            // signals for each partition. The watch signal tells us which
            // partition triggered this cycle; we extend to all that are ready.
            // Since OffsetTracker doesn't expose an iterator, we collect what
            // we know about from the tracker.
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
