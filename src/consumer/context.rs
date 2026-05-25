//! Custom consumer context for rdkafka rebalance callbacks.
//!
//! Implements `ConsumerContext` trait from rdkafka to intercept partition
//! revocation and assignment events. Commits pending offsets synchronously
//! on revocation (cooperative-sticky strategy) and seeks to committed+1
//! on assignment.

use crate::log::{debug, error, info, trace, warn};
use rdkafka::client::ClientContext;
use rdkafka::config::RDKafkaLogLevel;
use rdkafka::consumer::{BaseConsumer, Consumer, ConsumerContext, Rebalance};
use std::sync::Arc;

use crate::offset::offset_tracker::OffsetTracker;

/// Custom consumer context that intercepts rebalance events.
///
/// Holds references to the offset tracker and DLQ components needed
/// for safe partition revocation and assignment handling.
///
/// # Rebalance Behavior
///
/// ## On Revocation (cooperative-sticky)
///
/// In `pre_rebalance` with `Rebalance::Revoke`:
/// 1. Get highest contiguous offset from OffsetTracker for each partition
/// 2. Store offset via `consumer.store_offset()` for Kafka to persist
/// 3. Commit synchronously via `consumer.commit_consumer_state(CommitMode::Sync)`
///
/// ## On Assignment
///
/// In `post_rebalance` with `Rebalance::Assign`:
/// 1. Query Kafka for committed offsets for the assigned partitions
/// 2. If in-memory tracker has a committed offset (mid-session rebalance), seek to tracker + 1
/// 3. If Kafka has a committed offset for this group, seek to it
/// 4. Otherwise, let auto.offset.reset determine the starting position
///
/// # Thread Safety
///
/// All fields are `Send + Sync` (via Arc) as required by rdkafka's context trait.
#[derive(Clone)]
pub struct CustomConsumerContext {
    offset_tracker: Arc<OffsetTracker>,
    /// Tracks whether each topic-partition is paused (for backpressure).
    /// Key: "topic-partition", Value: bool (true = paused)
    pause_state: Arc<parking_lot::Mutex<std::collections::HashMap<String, bool>>>,
    /// Pre-fetched Kafka committed offsets, populated before polling starts.
    /// Key: (topic, partition), Value: committed offset from Kafka.
    /// Used in `post_rebalance` to seek to the correct starting position without
    /// making network calls from within the callback (which deadlocks the poll thread).
    startup_offsets: Arc<parking_lot::Mutex<std::collections::HashMap<(String, i32), i64>>>,
}

impl CustomConsumerContext {
    /// Creates a new CustomConsumerContext with the given dependencies.
    pub fn new(offset_tracker: Arc<OffsetTracker>) -> Self {
        Self {
            offset_tracker,
            pause_state: Arc::new(parking_lot::Mutex::new(std::collections::HashMap::new())),
            startup_offsets: Arc::new(parking_lot::Mutex::new(std::collections::HashMap::new())),
        }
    }

    /// Seeds the startup offset cache with committed offsets pre-fetched from Kafka.
    ///
    /// Must be called BEFORE polling starts (i.e., before `consumer.stream()` / `consumer.recv()`).
    /// Calling `committed_offsets()` from within a rebalance callback deadlocks the poll thread.
    pub fn seed_startup_offsets(&self, offsets: std::collections::HashMap<(String, i32), i64>) {
        let mut guard = self.startup_offsets.lock();
        *guard = offsets;
    }

    /// Returns a mutable reference to the pause state map.
    pub(crate) fn pause_state(
        &self,
    ) -> Arc<parking_lot::Mutex<std::collections::HashMap<String, bool>>> {
        Arc::clone(&self.pause_state)
    }
}

impl std::fmt::Debug for CustomConsumerContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CustomConsumerContext")
            .field("offset_tracker", &"Arc<OffsetTracker>")
            .finish()
    }
}

impl ClientContext for CustomConsumerContext {
    fn log(&self, level: RDKafkaLogLevel, fac: &str, log_message: &str) {
        match level {
            RDKafkaLogLevel::Emerg
            | RDKafkaLogLevel::Alert
            | RDKafkaLogLevel::Critical
            | RDKafkaLogLevel::Error => {
                error!(target: "librdkafka", "{} {}", fac, log_message);
            }
            RDKafkaLogLevel::Warning => {
                warn!(target: "librdkafka", "{} {}", fac, log_message);
            }
            RDKafkaLogLevel::Notice => {
                info!(target: "librdkafka", "{} {}", fac, log_message);
            }
            RDKafkaLogLevel::Info => {
                debug!(target: "librdkafka", "{} {}", fac, log_message);
            }
            RDKafkaLogLevel::Debug => {
                trace!(target: "librdkafka", "{} {}", fac, log_message);
            }
        }
    }
}

impl ConsumerContext for CustomConsumerContext {
    /// Pre-rebalance callback — called BEFORE partition assignment/revocation.
    ///
    /// For revoke events, we commit offsets synchronously to ensure no
    /// messages are reprocessed after rebalance.
    fn pre_rebalance(&self, consumer: &BaseConsumer<Self>, rebalance: &Rebalance<'_>) {
        match rebalance {
            Rebalance::Revoke(tpl) => {
                if tpl.count() == 0 {
                    debug!("pre_rebalance: Revoke with empty list");
                    return;
                }
                info!("rebalance: partitions revoked: count={}", tpl.count());
                for elem in tpl.elements() {
                    let topic = elem.topic();
                    let partition = elem.partition();

                    // Get highest contiguous offset for this partition
                    if let Some(offset) = self.offset_tracker.highest_contiguous(topic, partition) {
                        let topic_owned = topic.to_string();
                        match consumer.store_offset(&topic_owned, partition, offset) {
                            Ok(()) => {
                                match consumer
                                    .commit_consumer_state(rdkafka::consumer::CommitMode::Sync)
                                {
                                    Ok(()) => {
                                        info!(
                                            "committed offset on revocation: topic={} partition={} offset={}",
                                            topic,
                                            partition,
                                            offset
                                        );
                                    }
                                    Err(e) => {
                                        error!(
                                            "failed to commit offset on revocation: topic={} partition={} offset={} error={}",
                                            topic,
                                            partition,
                                            offset,
                                            e
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                error!(
                                    "failed to store offset on revocation: topic={} partition={} offset={} error={}",
                                    topic,
                                    partition,
                                    offset,
                                    e
                                );
                            }
                        }
                    } else {
                        debug!(
                            "no committed offset to revoke: topic={} partition={}",
                            topic, partition
                        );
                    }

                    // Clear pause state for this partition
                    let key = format!("{}-{}", topic, partition);
                    self.pause_state.lock().remove(&key);
                }
            }
            Rebalance::Assign(tpl) => {
                if tpl.count() == 0 {
                    debug!("pre_rebalance: Assign with empty list");
                    return;
                }
                info!(
                    "rebalance: partitions about to be assigned: count={}",
                    tpl.count()
                );
            }
            Rebalance::Error(e) => {
                error!("rebalance error: error={}", e);
            }
        }
    }

    /// Post-rebalance callback — called AFTER partition assignment/revocation.
    ///
    /// For assign events, we seek to the last committed offset + 1.
    fn post_rebalance(&self, consumer: &BaseConsumer<Self>, rebalance: &Rebalance<'_>) {
        match rebalance {
            Rebalance::Assign(tpl) => {
                if tpl.count() == 0 {
                    debug!("post_rebalance: Assign with empty list");
                    return;
                }
                info!("rebalance: partitions assigned: count={}", tpl.count());

                // rd_kafka_assign/incremental_assign sets the initial fetch position to
                // auto.offset.reset BEFORE committed offsets are fetched from Kafka.
                // We must explicitly seek here using pre-fetched Kafka committed offsets
                // (seeding happens in ConsumerRunner::new before polling starts).
                //
                // Priority:
                //   1. In-memory tracker has a committed offset (mid-session rebalance) → seek to tracker + 1
                //   2. Pre-fetched Kafka committed offset exists → seek to it
                //   3. Neither → leave position as-is (auto.offset.reset applies)
                let startup_offsets = self.startup_offsets.lock();

                for elem in tpl.elements() {
                    let topic = elem.topic();
                    let partition = elem.partition();
                    let topic_owned = topic.to_string();

                    let in_memory = self.offset_tracker.committed_offset(topic, partition);

                    let seek_to: Option<i64> = if in_memory >= 0 {
                        // Mid-session rebalance: in-memory tracker is authoritative
                        Some(in_memory + 1)
                    } else {
                        // Fresh startup: use pre-fetched Kafka committed offset
                        startup_offsets
                            .get(&(topic_owned.clone(), partition))
                            .copied()
                    };

                    match seek_to {
                        Some(seek_offset) => {
                            match consumer.seek(
                                &topic_owned,
                                partition,
                                rdkafka::Offset::Offset(seek_offset),
                                std::time::Duration::from_secs(5),
                            ) {
                                Ok(_) => {
                                    info!(
                                        "seeked on assignment: topic={} partition={} seek_offset={}",
                                        topic, partition, seek_offset
                                    );
                                }
                                Err(e) => {
                                    error!(
                                        "failed to seek on assignment: topic={} partition={} seek_offset={} error={}",
                                        topic, partition, seek_offset, e
                                    );
                                }
                            }
                        }
                        None => {
                            debug!(
                                "no committed offset found, using auto.offset.reset: topic={} partition={}",
                                topic, partition
                            );
                        }
                    }
                }
            }
            Rebalance::Revoke(_) => {
                // Nothing to do on revoke - offsets were committed in pre_rebalance
            }
            Rebalance::Error(_) => {
                // Error already logged in pre_rebalance
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_consumer_context_debug() {
        // Verify Debug impl compiles and produces expected output
        let ctx = CustomConsumerContext::new(Arc::new(OffsetTracker::new()));
        let debug_str = format!("{:?}", ctx);
        assert!(debug_str.contains("CustomConsumerContext"));
    }
}
