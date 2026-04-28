//! Custom consumer context for rdkafka rebalance callbacks.
//!
//! Implements `ConsumerContext` trait from rdkafka to intercept partition
//! revocation and assignment events. Commits pending offsets synchronously
//! on revocation (cooperative-sticky strategy) and seeks to committed+1
//! on assignment.

use crate::coordinator::OffsetTracker;
use crate::dlq::produce::SharedDlqProducer;
use crate::dlq::router::DlqRouter;
use rdkafka::client::ClientContext;
use rdkafka::config::RDKafkaLogLevel;
use rdkafka::consumer::{BaseConsumer, Consumer, ConsumerContext, Rebalance};
use std::sync::Arc;
use tracing::{debug, error, info};

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
/// 1. Get last committed offset from OffsetTracker for each partition
/// 2. Seek to `committed + 1` (next unprocessed message)
/// 3. Log the seek operation
///
/// # Thread Safety
///
/// All fields are `Send + Sync` (via Arc) as required by rdkafka's context trait.
#[derive(Clone)]
pub struct CustomConsumerContext {
    offset_tracker: Arc<OffsetTracker>,
    #[allow(dead_code)]
    dlq_router: Arc<dyn DlqRouter>,
    #[allow(dead_code)]
    dlq_producer: Arc<SharedDlqProducer>,
    /// Tracks whether each topic-partition is paused (for backpressure).
    /// Key: "topic-partition", Value: bool (true = paused)
    pause_state: Arc<parking_lot::Mutex<std::collections::HashMap<String, bool>>>,
}

impl CustomConsumerContext {
    /// Creates a new CustomConsumerContext with the given dependencies.
    pub fn new(
        offset_tracker: Arc<OffsetTracker>,
        dlq_router: Arc<dyn DlqRouter>,
        dlq_producer: Arc<SharedDlqProducer>,
    ) -> Self {
        Self {
            offset_tracker,
            dlq_router,
            dlq_producer,
            pause_state: Arc::new(parking_lot::Mutex::new(
                std::collections::HashMap::new(),
            )),
        }
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
            .field("dlq_router", &"Arc<dyn DlqRouter>")
            .field("dlq_producer", &"Arc<SharedDlqProducer>")
            .finish()
    }
}

impl ClientContext for CustomConsumerContext {
    fn log(&self, level: RDKafkaLogLevel, fac: &str, log_message: &str) {
        match level {
            RDKafkaLogLevel::Emerg | RDKafkaLogLevel::Alert | RDKafkaLogLevel::Critical | RDKafkaLogLevel::Error => {
                tracing::error!(target: "librdkafka", "{} {}", fac, log_message);
            }
            RDKafkaLogLevel::Warning => {
                tracing::warn!(target: "librdkafka", "{} {}", fac, log_message);
            }
            RDKafkaLogLevel::Notice => {
                tracing::info!(target: "librdkafka", "{} {}", fac, log_message);
            }
            RDKafkaLogLevel::Info => {
                tracing::debug!(target: "librdkafka", "{} {}", fac, log_message);
            }
            RDKafkaLogLevel::Debug => {
                tracing::trace!(target: "librdkafka", "{} {}", fac, log_message);
            }
        }
    }
}

impl ConsumerContext for CustomConsumerContext {
    /// Pre-rebalance callback — called BEFORE partition assignment/revocation.
    ///
    /// For revoke events, we commit offsets synchronously to ensure no
    /// messages are reprocessed after rebalance.
    fn pre_rebalance(
        &self,
        consumer: &BaseConsumer<Self>,
        rebalance: &Rebalance<'_>,
    ) {
        match rebalance {
            Rebalance::Revoke(tpl) => {
                if tpl.count() == 0 {
                    debug!("pre_rebalance: Revoke with empty list");
                    return;
                }
                info!(
                    count = tpl.count(),
                    "rebalance: partitions revoked"
                );
                for elem in tpl.elements() {
                    let topic = elem.topic();
                    let partition = elem.partition();

                    // PITFALLS-4.2: Never call Python code in rebalance callbacks
                    // Get highest contiguous offset for this partition
                    if let Some(offset) = self.offset_tracker.highest_contiguous(topic, partition) {
                        let topic_owned = topic.to_string();
                        match consumer.store_offset(&topic_owned, partition, offset) {
                            Ok(()) => {
                                match consumer.commit_consumer_state(rdkafka::consumer::CommitMode::Sync) {
                                    Ok(()) => {
                                        info!(
                                            topic = %topic,
                                            partition = partition,
                                            offset = offset,
                                            "committed offset on revocation"
                                        );
                                    }
                                    Err(e) => {
                                        error!(
                                            topic = %topic,
                                            partition = partition,
                                            offset = offset,
                                            error = %e,
                                            "failed to commit offset on revocation"
                                        );
                                    }
                                }
                            }
                            Err(e) => {
                                error!(
                                    topic = %topic,
                                    partition = partition,
                                    offset = offset,
                                    error = %e,
                                    "failed to store offset on revocation"
                                );
                            }
                        }
                    } else {
                        debug!(
                            topic = %topic,
                            partition = partition,
                            "no committed offset to revoke"
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
                    count = tpl.count(),
                    "rebalance: partitions about to be assigned"
                );
            }
            Rebalance::Error(e) => {
                error!(
                    error = %e,
                    "rebalance error"
                );
            }
        }
    }

    /// Post-rebalance callback — called AFTER partition assignment/revocation.
    ///
    /// For assign events, we seek to the last committed offset + 1.
    fn post_rebalance(
        &self,
        consumer: &BaseConsumer<Self>,
        rebalance: &Rebalance<'_>,
    ) {
        match rebalance {
            Rebalance::Assign(tpl) => {
                if tpl.count() == 0 {
                    debug!("post_rebalance: Assign with empty list");
                    return;
                }
                info!(
                    count = tpl.count(),
                    "rebalance: partitions assigned, seeking to committed+1"
                );
                for elem in tpl.elements() {
                    let topic = elem.topic();
                    let partition = elem.partition();

                    // PITFALLS-4.2: Never call Python code in rebalance callbacks
                    // Get the last committed offset for this partition
                    let committed = self.offset_tracker.committed_offset(topic, partition);
                    // PITFALLS-4.1: Seek to committed + 1, not committed
                    let seek_offset = committed + 1;

                    let topic_owned = topic.to_string();
                    let offset = rdkafka::Offset::Offset(seek_offset);
                    match consumer.seek(&topic_owned, partition, offset, std::time::Duration::from_secs(5)) {
                        Ok(_) => {
                            info!(
                                topic = %topic,
                                partition = partition,
                                committed_offset = committed,
                                seek_offset = seek_offset,
                                "seeked to committed+1 on assignment"
                            );
                        }
                        Err(e) => {
                            error!(
                                topic = %topic,
                                partition = partition,
                                seek_offset = seek_offset,
                                error = %e,
                                "failed to seek on assignment"
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
        let ctx = CustomConsumerContext::new(
            Arc::new(OffsetTracker::new()),
            Arc::new(crate::dlq::router::DefaultDlqRouter::with_default_prefix()),
            Arc::new(crate::dlq::produce::SharedDlqProducer::new(
                &crate::consumer::ConsumerConfigBuilder::new()
                    .brokers("localhost:9092")
                    .group_id("test-group")
                    .topics(["test"])
                    .build()
                    .unwrap(),
            ).unwrap()),
        );
        let debug_str = format!("{:?}", ctx);
        assert!(debug_str.contains("CustomConsumerContext"));
    }
}
