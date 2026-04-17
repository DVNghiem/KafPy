//! Per-topic-partition offset tracking with BTreeSet-based out-of-order buffering.
//!
//! # Algorithm
//!
//! 1. `PartitionState.committed_offset` starts at `-1` (before any message committed)
//! 2. `ack(topic, partition, offset)`:
//!    - Insert offset into `pending_offsets`
//!    - While `pending_offsets.contains(committed_offset + 1)`, remove from pending and increment `committed_offset`
//! 3. `highest_contiguous()` returns `committed_offset` (the last safely committed offset)
//! 4. `should_commit()` returns `pending_offsets.contains(committed_offset + 1)`
//! 5. `mark_failed()` removes offset from `pending_offsets` if present, inserts into `failed_offsets`

use crate::consumer::ConsumerRunner;
use crate::failure::{FailureCategory, FailureReason};
use parking_lot::Mutex;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Key type for topic-partition lookup.
/// Using a tuple struct for type safety over raw (String, i32) pairs.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
struct TopicPartitionKey(String, i32);

impl TopicPartitionKey {
    fn new(topic: impl Into<String>, partition: i32) -> Self {
        Self(topic.into(), partition)
    }
}

/// Per-partition offset state machine.
///
/// Maintains:
/// - `committed_offset`: highest contiguous offset that has been acknowledged
/// - `pending_offsets`: BTreeSet of offsets awaiting commit (out-of-order buffering)
/// - `failed_offsets`: BTreeSet of offsets that failed processing (not to be committed)
///
/// # Invariant
///
/// All offsets in `pending_offsets` are > `committed_offset`.
/// All offsets in `failed_offsets` are also > `committed_offset`.
#[derive(Debug, Clone)]
pub struct PartitionState {
    /// The last committed offset. Starts at -1 (no messages committed).
    /// Committed offset N means messages 0..=N have been safely processed.
    pub committed_offset: i64,
    /// Pending offsets awaiting commit. Buffered for out-of-order ack handling.
    pub pending_offsets: BTreeSet<i64>,
    /// Failed offsets that should not be committed.
    pub failed_offsets: BTreeSet<i64>,
    /// Last failure reason for this partition (for DLQ routing).
    pub last_failure_reason: Option<FailureReason>,
    /// Whether a terminal failure has been seen on this partition.
    /// When true, should_commit returns false for this partition (D-03: set once, never cleared).
    pub has_terminal: bool,
}

impl PartitionState {
    /// Creates a new partition state with no committed offsets.
    pub fn new() -> Self {
        Self {
            // -1 means "no offset committed yet" — Kafka offsets start at 0
            committed_offset: -1,
            pending_offsets: BTreeSet::new(),
            failed_offsets: BTreeSet::new(),
            last_failure_reason: None,
            has_terminal: false,
        }
    }

    /// Sets `has_terminal = true` for this partition.
    ///
    /// Idempotent — calling when already true is a no-op (D-03: set once, never clear).
    pub fn set_terminal(&mut self) {
        self.has_terminal = true;
    }

    /// Records an ack for `offset`, buffering if out-of-order.
    ///
    /// Algorithm:
    /// 1. Insert offset into pending_offsets (for now)
    /// 2. Remove from failed_offsets if it was there (retry succeeded)
    /// 3. Advance contiguous cursor while gap fills
    pub fn ack(&mut self, offset: i64) {
        self.pending_offsets.insert(offset);
        self.failed_offsets.remove(&offset);

        // Advance contiguous cursor: commit while the next offset is pending
        loop {
            let next = self.committed_offset + 1;
            if self.pending_offsets.remove(&next) {
                self.committed_offset = next;
            } else {
                break;
            }
        }
    }

    /// Marks `offset` as failed — moves from pending to failed.
    ///
    /// Does NOT advance `committed_offset`. The gap remains until the failed
    /// offset is explicitly retried and acked again.
    pub fn mark_failed(&mut self, offset: i64) {
        self.pending_offsets.remove(&offset);
        self.failed_offsets.insert(offset);
    }
}

impl Default for PartitionState {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-topic-partition offset tracker using BTreeSet for out-of-order buffering.
///
/// Thread-safe via `parking_lot::Mutex` — all state access goes through the mutex.
pub struct OffsetTracker {
    partitions: Mutex<HashMap<TopicPartitionKey, PartitionState>>,
    /// ConsumerRunner for Kafka commit operations. Set via set_runner() before use.
    runner: Mutex<Option<Arc<ConsumerRunner>>>,
}

impl OffsetTracker {
    /// Creates a new empty offset tracker.
    pub fn new() -> Self {
        Self {
            partitions: Mutex::new(HashMap::new()),
            runner: Mutex::new(None),
        }
    }

    /// Sets the ConsumerRunner for Kafka commit operations.
    /// Called once during setup before the worker pool starts.
    pub fn set_runner(&self, runner: Arc<ConsumerRunner>) {
        *self.runner.lock() = Some(runner);
    }

    /// Records an ack for the given topic-partition at `offset`.
    ///
    /// Creates the partition state if it doesn't exist.
    /// Advances the contiguous cursor if the ack fills a gap.
    ///
    /// # Panics
    ///
    /// Panics if `offset` is negative.
    pub fn ack(&self, topic: &str, partition: i32, offset: i64) {
        assert!(offset >= 0, "offset must be >= 0, got {}", offset);
        let key = TopicPartitionKey::new(topic, partition);
        let mut guard = self.partitions.lock();
        let state = guard.entry(key).or_insert_with(PartitionState::new);
        state.ack(offset);
    }

    /// Returns the highest contiguous offset for the given topic-partition.
    ///
    /// Returns `None` if the topic-partition has not been registered.
    pub fn highest_contiguous(&self, topic: &str, partition: i32) -> Option<i64> {
        let key = TopicPartitionKey::new(topic, partition);
        let guard = self.partitions.lock();
        guard.get(&key).map(|s| s.committed_offset)
    }

    /// Returns `true` if the next offset after `committed_offset` is pending.
    ///
    /// This indicates that we should call `store_offset` + `commit` to persist
    /// the new high watermark.
    ///
    /// Returns `false` if `has_terminal=true` for this partition (D-01: terminal
    /// messages block commit for that partition only — other partitions unaffected).
    pub fn should_commit(&self, topic: &str, partition: i32) -> bool {
        let key = TopicPartitionKey::new(topic, partition);
        let guard = self.partitions.lock();
        guard.get(&key).is_some_and(|s| {
            // D-01: commit gating — terminal blocks commit for this partition only
            if s.has_terminal {
                return false;
            }
            s.pending_offsets.contains(&(s.committed_offset + 1))
        })
    }

    /// Marks `offset` as failed for the given topic-partition.
    ///
    /// The offset is moved from `pending_offsets` to `failed_offsets`.
    /// Does NOT advance `committed_offset`.
    ///
    /// If the topic-partition is not registered, this is a no-op.
    pub fn mark_failed(&self, topic: &str, partition: i32, offset: i64) {
        let key = TopicPartitionKey::new(topic, partition);
        let mut guard = self.partitions.lock();
        if let Some(state) = guard.get_mut(&key) {
            state.mark_failed(offset);
        }
    }

    /// Returns the last committed offset for the given topic-partition.
    ///
    /// Returns `-1` if the topic-partition has not been registered.
    pub fn committed_offset(&self, topic: &str, partition: i32) -> i64 {
        let key = TopicPartitionKey::new(topic, partition);
        let guard = self.partitions.lock();
        guard.get(&key).map_or(-1, |s| s.committed_offset)
    }

    /// Returns all registered topic-partition pairs.
    ///
    /// Used by `graceful_shutdown()` to enumerate partitions for final commit.
    pub fn all_partitions(&self) -> Vec<(String, i32)> {
        let guard = self.partitions.lock();
        guard
            .keys()
            .map(|key| {
                let TopicPartitionKey(t, p) = key;
                (t.clone(), *p)
            })
            .collect()
    }
}

use super::OffsetCoordinator;

impl Default for OffsetTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl OffsetCoordinator for OffsetTracker {
    fn record_ack(&self, topic: &str, partition: i32, offset: i64) {
        self.ack(topic, partition, offset);
    }

    fn mark_failed(&self, topic: &str, partition: i32, offset: i64, reason: &FailureReason) {
        let key = TopicPartitionKey::new(topic, partition);
        let mut guard = self.partitions.lock();
        if let Some(state) = guard.get_mut(&key) {
            state.mark_failed(offset);
            state.last_failure_reason = Some(reason.clone());
            // D-05: set has_terminal on Terminal category (D-03: set once, never clear — idempotent)
            if reason.category() == FailureCategory::Terminal {
                state.has_terminal = true;
            }
        }
    }

    /// Commits all ready offsets for each registered partition.
///
/// Called by WorkerPool::shutdown AFTER flush_failed_to_dlq() has drained
/// all failed messages to DLQ. This ensures all terminal/retryable failures
/// are flushed before final offset commit (D-02).
fn graceful_shutdown(&self) {
        let partitions = self.all_partitions();
        for (topic, partition) in partitions {
            if !self.should_commit(&topic, partition) {
                continue;
            }
            if let Some(offset) = self.highest_contiguous(&topic, partition) {
                let runner_guard = self.runner.lock();
                if let Some(ref runner_arc) = *runner_guard {
                    let _ = runner_arc.store_offset(&topic, partition, offset);
                    let _ = runner_arc.commit();
                }
            }
        }
    }

    fn flush_failed_to_dlq(
        &self,
        dlq_router: &std::sync::Arc<dyn crate::dlq::DlqRouter>,
        dlq_producer: &std::sync::Arc<crate::dlq::SharedDlqProducer>,
    ) {
        use crate::dlq::DlqMetadata;

        let partitions = self.all_partitions();
        let now = chrono::Utc::now();

        for (topic, partition) in partitions {
            let failed_offsets: Vec<i64>;
            let last_reason: Option<String>;

            {
                let key = TopicPartitionKey::new(&topic, partition);
                let guard = self.partitions.lock();
                if let Some(state) = guard.get(&key) {
                    failed_offsets = state.failed_offsets.iter().cloned().collect();
                    last_reason = state.last_failure_reason.as_ref().map(|r| r.to_string());
                } else {
                    continue;
                }
            }

            if failed_offsets.is_empty() {
                continue;
            }

            let reason_str = last_reason.unwrap_or_else(|| "unknown".to_string());

            for &offset in &failed_offsets {
                // OffsetTracker does not store original payload/key — log as limitation
                tracing::warn!(
                    topic = %topic,
                    partition = partition,
                    offset = offset,
                    reason = %reason_str,
                    "flush_failed_to_dlq: original payload not available, producing empty DLQ message"
                );

                let metadata = DlqMetadata::new(
                    topic.clone(),
                    partition,
                    offset,
                    reason_str.clone(),
                    1, // attempt_count: not tracked per-offset in OffsetTracker
                    now,
                    now,
                );

                let tp = dlq_router.route(&metadata);
                tracing::info!(
                    topic = %topic,
                    partition = partition,
                    offset = offset,
                    dlq_topic = %tp.topic,
                    dlq_partition = tp.partition,
                    "flushing failed offset to DLQ"
                );

                // Fire-and-forget — don't await
                dlq_producer.produce_async(
                    tp.topic,
                    tp.partition,
                    vec![],   // empty payload — OffsetTracker doesn't store it
                    None,     // no key
                    &metadata,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // OFFSET-03: Sequential acks — all contiguous
    #[test]
    fn sequential_acks_advance_contiguous() {
        let tracker = OffsetTracker::new();
        tracker.ack("topic", 0, 10);
        tracker.ack("topic", 0, 11);
        tracker.ack("topic", 0, 12);
        assert_eq!(tracker.highest_contiguous("topic", 0), Some(12));
    }

    // OFFSET-03: Out-of-order acks — buffering fills gaps
    #[test]
    fn out_of_order_acks_buffer_until_gap_fills() {
        let tracker = OffsetTracker::new();
        tracker.ack("topic", 0, 10);
        tracker.ack("topic", 0, 12); // Out of order
        assert_eq!(tracker.highest_contiguous("topic", 0), Some(10)); // 11 missing, can't advance

        tracker.ack("topic", 0, 11); // Now fills the gap
        assert_eq!(tracker.highest_contiguous("topic", 0), Some(12)); // All contiguous now
    }

    // OFFSET-05: should_commit — true when next offset is pending
    #[test]
    fn should_commit_true_when_next_offset_pending() {
        let tracker = OffsetTracker::new();
        tracker.ack("topic", 0, 10);
        // committed_offset is now 10 (10 filled the initial -1 gap)
        // next offset to commit is 11
        assert!(tracker.should_commit("topic", 0));

        // After committing 11:
        tracker.ack("topic", 0, 11);
        assert_eq!(tracker.highest_contiguous("topic", 0), Some(11));
        // Next is 12 — not pending yet, should_commit should be false
        assert!(!tracker.should_commit("topic", 0));
    }

    // OFFSET-06: Failed offset — does NOT advance cursor
    #[test]
    fn failed_offset_does_not_advance_cursor() {
        let tracker = OffsetTracker::new();
        tracker.ack("topic", 0, 10);
        tracker.ack("topic", 0, 11);
        tracker.ack("topic", 0, 12);
        assert_eq!(tracker.highest_contiguous("topic", 0), Some(12));

        tracker.mark_failed("topic", 0, 11);
        // 11 moved to failed, pending still has 10+1=12
        // but committed_offset was 11 when 11 was removed...
        // Wait — committed_offset was advanced to 11 when we acked 11.
        // mark_failed removes 11 from pending but does NOT revert committed_offset.
        // So after mark_failed(11):
        // - pending_offsets: {12} (since 10 and 11 were removed when they advanced cursor)
        // - committed_offset: 11
        // - next offset to commit is 12, which is pending → should_commit = true
        assert_eq!(tracker.highest_contiguous("topic", 0), Some(11));
        assert!(tracker.should_commit("topic", 0));
    }

    // OFFSET-07: Starting state — new partition has committed_offset = -1
    #[test]
    fn new_partition_starts_at_minus_one() {
        let tracker = OffsetTracker::new();
        assert_eq!(tracker.committed_offset("topic", 0), -1);
        assert_eq!(tracker.highest_contiguous("topic", 0), None); // Not registered yet
    }

    // Basic sanity: registering a new partition then acking
    #[test]
    fn first_ack_advances_from_minus_one() {
        let tracker = OffsetTracker::new();
        tracker.ack("topic", 0, 0);
        assert_eq!(tracker.highest_contiguous("topic", 0), Some(0));
        assert!(!tracker.should_commit("topic", 0)); // Next is 1, not pending
    }

    // Multiple partitions are independent
    #[test]
    fn multiple_partitions_independent() {
        let tracker = OffsetTracker::new();
        tracker.ack("topic", 0, 5);
        tracker.ack("topic", 1, 10);

        assert_eq!(tracker.highest_contiguous("topic", 0), Some(5));
        assert_eq!(tracker.highest_contiguous("topic", 1), Some(10));

        tracker.ack("topic", 0, 7); // Gap at 6
        assert_eq!(tracker.highest_contiguous("topic", 0), Some(5)); // Stuck

        tracker.ack("topic", 1, 12);
        assert_eq!(tracker.highest_contiguous("topic", 1), Some(12));
    }
}
