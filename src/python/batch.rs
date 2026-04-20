//! Batch accumulator for handler-mode batch execution.

use std::collections::HashMap;
use parking_lot::Mutex;

use crate::dispatcher::OwnedMessage;
use crate::worker_pool::accumulator::PerPartitionBuffer;

/// Per-handler batch accumulator.
///
/// Accumulates messages per-partition (preserving ordering within partition).
/// Batches are formed per-partition, then combined when flushed.
/// Uses parking_lot::Mutex for interior mutability (matches OffsetTracker pattern).
pub struct BatchAccumulator {
    partition_accumulators: Mutex<HashMap<i32, PerPartitionBuffer>>,
    max_batch_size: usize,
    max_batch_wait: std::time::Duration,
}

impl BatchAccumulator {
    /// Create a new BatchAccumulator with the given batch policy.
    pub fn new(max_batch_size: usize, max_batch_wait_ms: u64) -> Self {
        Self {
            partition_accumulators: Mutex::new(HashMap::new()),
            max_batch_size,
            max_batch_wait: std::time::Duration::from_millis(max_batch_wait_ms),
        }
    }

    /// Returns true if adding this message would fill its partition to max_batch_size.
    /// Used to trigger a preemptive flush before adding.
    pub fn would_fill_partition(&self, partition: i32) -> bool {
        let guard = self.partition_accumulators.lock();
        guard
            .get(&partition)
            .map(|acc| acc.len() >= self.max_batch_size)
            .unwrap_or(false)
    }

    /// Add a message to the appropriate partition accumulator.
    /// Starts the fixed-window timer if this is the first message for that partition.
    pub fn add(&self, msg: OwnedMessage) {
        let partition = msg.partition;
        let mut guard = self.partition_accumulators.lock();
        let acc = guard
            .entry(partition)
            .or_insert_with(|| PerPartitionBuffer::new());
        acc.add(msg, self.max_batch_wait);
    }

    /// Returns the earliest deadline across all partitions, or None if all empty.
    pub fn next_deadline(&self) -> Option<tokio::time::Instant> {
        let guard = self.partition_accumulators.lock();
        guard.values().filter_map(|p| p.deadline()).min()
    }

    /// Returns true if any partition has messages with an expired deadline.
    pub fn is_any_deadline_expired(&self) -> bool {
        let guard = self.partition_accumulators.lock();
        for acc in guard.values() {
            if !acc.is_empty() && acc.is_deadline_expired() {
                return true;
            }
        }
        false
    }

    /// Returns true if the accumulator is completely empty.
    pub fn is_empty(&self) -> bool {
        let guard = self.partition_accumulators.lock();
        guard.values().all(|acc| acc.is_empty())
    }

    /// Flush and return all nonempty partitions as (partition, messages) pairs.
    /// Clears all partition accumulators after flushing.
    pub fn flush_all(&self) -> Vec<(i32, Vec<OwnedMessage>)> {
        let mut guard = self.partition_accumulators.lock();
        let mut result = Vec::new();
        for (partition, acc) in guard.iter_mut() {
            if !acc.is_empty() {
                result.push((*partition, acc.take_messages()));
            }
        }
        result
    }

    /// Flush and return messages from a specific partition.
    pub fn flush_partition(&self, partition: i32) -> Option<Vec<OwnedMessage>> {
        let mut guard = self.partition_accumulators.lock();
        guard.get_mut(&partition).and_then(|acc| {
            if acc.is_empty() {
                None
            } else {
                Some(acc.take_messages())
            }
        })
    }

    /// Returns total message count across all partitions.
    pub fn total_len(&self) -> usize {
        let guard = self.partition_accumulators.lock();
        guard.values().map(|acc| acc.len()).sum()
    }
}
