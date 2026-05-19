//! Consumer dispatcher that wires [`ConsumerRunner`] output to per-handler channels.
//!
//! Owns `ConsumerRunner + Dispatcher` and orchestrates the async message loop.

use crate::consumer::runner::ConsumerRunner;
use crate::consumer::OwnedMessage;
use crate::dispatcher::backpressure::BackpressureAction;
use crate::dispatcher::error::DispatchError;
use crate::dispatcher::{Dispatcher, QueueManager};
use crate::log::{debug, error, info, warn};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

/// Owns ConsumerRunner + Dispatcher and orchestrates the async message loop.
/// Wires consumer stream output to dispatcher input.
pub struct ConsumerDispatcher {
    runner: Arc<ConsumerRunner>,
    dispatcher: Dispatcher,
    /// Topic-partition lists for each subscribed topic (populated on assignment).
    partition_handles:
        parking_lot::Mutex<std::collections::HashMap<String, rdkafka::TopicPartitionList>>,
    /// Topics currently paused (tracked for resume logic).
    paused_topics: parking_lot::Mutex<HashSet<String>>,
    /// Backpressure threshold ratio for resume (0.0 to 1.0).
    resume_threshold: f64,
}

impl ConsumerDispatcher {
    /// Creates a new dispatcher wired to the given runner.
    pub fn new(runner: ConsumerRunner) -> Self {
        Self {
            runner: Arc::new(runner),
            dispatcher: Dispatcher::new(),
            partition_handles: parking_lot::Mutex::new(std::collections::HashMap::new()),
            paused_topics: parking_lot::Mutex::new(HashSet::new()),
            resume_threshold: 0.5,
        }
    }

    /// Registers a handler for `topic` with bounded queue of `capacity`.
    /// Optionally limits concurrency with `max_concurrency` semaphore permits.
    pub fn register_handler(
        &self,
        topic: impl Into<String>,
        capacity: usize,
        max_concurrency: Option<usize>,
    ) -> mpsc::Receiver<OwnedMessage> {
        let semaphore = max_concurrency.map(|n| Arc::new(tokio::sync::Semaphore::new(n)));
        self.dispatcher
            .register_handler_with_semaphore(topic, capacity, semaphore)
    }

    /// Runs the dispatch loop, polling the consumer stream and
    /// dispatching each message through the dispatcher.
    pub(crate) async fn run(&self) {
        // Start the stream immediately — partition assignment only happens once the
        // consumer is polled, so we must start consuming before we can populate handles.
        // Partition handles are only needed for backpressure pause/resume; they are
        // refreshed lazily on the first backpressure event if not yet available.
        let mut stream = self.runner.stream();

        // Try to populate partition handles now; likely fails on first call (no assignment yet).
        // The backpressure path below will retry when needed.
        if let Err(e) = self.populate_partitions() {
            debug!("partition handles not yet available at startup (will retry on backpressure): {}", e);
        }
        while let Some(result) = stream.next().await {
            match result {
                Ok(msg) => {
                    let topic = msg.topic.clone();
                    let (outcome, pause_signal) =
                        self.dispatcher.send_with_policy_and_signal(msg).await;
                    match outcome {
                        Ok(outcome) => {
                            self.check_resume(&topic, outcome.queue_depth);
                        }
                        Err(DispatchError::Backpressure {
                            queue_name: _,
                            reason: _,
                        }) => {
                            if let Some(BackpressureAction::PausePartition {
                                topic: pause_topic,
                                ..
                            }) = pause_signal
                            {
                                // Refresh partition handles if this is the first pause
                                // attempt and we have no handles yet (race at startup).
                                if self.partition_handles.lock().is_empty() {
                                    let _ = self.populate_partitions();
                                }
                                match self.pause_partition(&pause_topic) {
                                    Ok(()) => {
                                        warn!("paused topic '{}' due to backpressure", pause_topic);
                                        self.paused_topics.lock().insert(pause_topic.clone());
                                    }
                                    Err(e) => {
                                        error!("failed to pause topic '{}': {}", pause_topic, e);
                                    }
                                }
                            }
                        }
                        Err(DispatchError::HandlerNotRegistered { topic: t }) => {
                            debug!("no handler for topic '{}', skipping", t);
                        }
                        Err(e) => {
                            error!("dispatch error: {}", e);
                        }
                    }
                }
                Err(e) => {
                    error!("consumer error: {}", e);
                }
            }
        }
    }

    /// Checks if a paused topic should be resumed based on current queue depth.
    fn check_resume(&self, topic: &str, current_depth: usize) {
        let Some(capacity) = self.dispatcher.get_capacity(topic) else {
            return;
        };
        let threshold = (capacity as f64 * self.resume_threshold) as usize;
        if current_depth < threshold && self.paused_topics.lock().remove(topic) {
            if let Err(e) = self.resume_partition(topic) {
                error!("failed to resume topic '{}': {}", topic, e);
            } else {
                info!(
                    "resumed topic '{}' (depth {} < threshold {})",
                    topic, current_depth, threshold
                );
            }
        }
    }

    /// Pauses consumption for all partitions of `topic` via rdkafka pause().
    fn pause_partition(&self, topic: &str) -> Result<(), crate::consumer::error::ConsumerError> {
        let handles = self.partition_handles.lock();
        if let Some(tpl) = handles.get(topic) {
            self.runner.pause(tpl)
        } else {
            Err(crate::consumer::error::ConsumerError::Subscription {
                broker: topic.to_string(),
                message: "no partition handle for topic - call populate_partitions() first"
                    .to_string(),
            })
        }
    }

    /// Resumes consumption for all partitions of `topic` via rdkafka resume().
    fn resume_partition(&self, topic: &str) -> Result<(), crate::consumer::error::ConsumerError> {
        let handles = self.partition_handles.lock();
        if let Some(tpl) = handles.get(topic) {
            self.runner.resume(tpl)
        } else {
            Err(crate::consumer::error::ConsumerError::Subscription {
                broker: topic.to_string(),
                message: "no partition handle for topic".to_string(),
            })
        }
    }

    /// Populates the internal partition handle map from the consumer's current assignment.
    /// Must be called after the consumer has been assigned partitions and before pause/resume.
    pub fn populate_partitions(&self) -> Result<(), crate::consumer::error::ConsumerError> {
        let assignment = self.runner.assignment()?;
        let mut by_topic: std::collections::HashMap<String, rdkafka::TopicPartitionList> =
            std::collections::HashMap::new();
        for elem in assignment.elements() {
            let topic_name = elem.topic().to_string();
            let partition = elem.partition();
            let offset = elem.offset();
            by_topic.entry(topic_name.clone()).or_default();
            let tpl = by_topic.get_mut(&topic_name).unwrap();
            tpl.add_partition_offset(topic_name.as_str(), partition, offset)
                .map_err(|e| crate::consumer::error::ConsumerError::Subscription {
                    broker: topic_name,
                    message: e.to_string(),
                })?;
        }
        *self.partition_handles.lock() = by_topic;
        Ok(())
    }

    /// Returns a reference to the underlying dispatcher for inspection.
    pub fn dispatcher(&self) -> &Dispatcher {
        &self.dispatcher
    }

    /// Returns an `Arc<QueueManager>` for WorkerPool ack integration (EXEC-13).
    pub(crate) fn queue_manager(&self) -> Arc<QueueManager> {
        // This means both the original Dispatcher's QM and the returned clone share
        // the same handlers HashMap - inserts are visible to both.
        Arc::new(self.dispatcher.queue_manager.clone())
    }
}

// Fake OwnedMessage for testing
#[cfg(test)]
impl OwnedMessage {
    pub(crate) fn fake(topic: &str, partition: i32, offset: i64) -> Self {
        use crate::consumer::MessageTimestamp;

        OwnedMessage {
            topic: topic.to_string(),
            partition,
            offset,
            key: None,
            payload: None,
            timestamp: MessageTimestamp::NotAvailable,
            headers: vec![],
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    // DISP-17: Owned types - compile-time verification
    #[test]
    fn owned_message_implements_send_and_sync() {
        fn assert_ownded<T: Send + Sync>() {}
        assert_ownded::<OwnedMessage>();
    }

    // DISP-18: PausePartition action carries topic for pause signal
    #[test]
    fn pause_partition_carries_topic_name() {
        let action = BackpressureAction::PausePartition {
            topic: "my-topic".to_string(),
            partition: 0,
        };
        assert_eq!(action.topic(), Some("my-topic"));
    }

    #[test]
    fn drop_action_has_no_topic() {
        let action = BackpressureAction::Drop;
        assert_eq!(action.topic(), None);
    }

    // DISP-15: Semaphore try_acquire returns false when no permit
    #[tokio::test]
    async fn semaphore_blocks_when_no_permit() {
        let dispatcher = Dispatcher::new();
        let _rx = dispatcher.register_handler_with_semaphore(
            "test-topic",
            10,
            Some(Arc::new(tokio::sync::Semaphore::new(1))),
        );

        let msg = OwnedMessage::fake("test-topic", 0, 100);
        // First send should succeed
        let (result, _) = dispatcher.send_with_policy_and_signal(msg).await;
        assert!(result.is_ok());

        // Second send should fail with backpressure (semaphore exhausted)
        let msg2 = OwnedMessage::fake("test-topic", 1, 101);
        let (result, _) = dispatcher.send_with_policy_and_signal(msg2).await;
        assert!(matches!(result, Err(DispatchError::Backpressure { .. })));
    }

    // DISP-15: No semaphore = unlimited concurrency (bounded by channel capacity)
    #[tokio::test]
    async fn no_semaphore_allows_unlimited_dispatch() {
        let dispatcher = Dispatcher::new();
        // Capacity 100 means we can dispatch up to 100 before backpressure
        let _rx = dispatcher.register_handler("test-topic", 100);

        for i in 0..50 {
            let msg = OwnedMessage::fake("test-topic", i % 3, i as i64);
            let (result, _) = dispatcher.send_with_policy_and_signal(msg).await;
            assert!(result.is_ok(), "dispatch {} should succeed", i);
        }
    }

    // DISP-18: ConsumerDispatcher cannot be tested without a real consumer
    // Integration tests in tests/ directory would use a real ConsumerRunner
    // Here we verify the type-level contract only

    // Verify Dispatcher send_with_policy and send work together
    #[test]
    fn send_uses_default_policy() {
        let dispatcher = Dispatcher::new();
        let _rx = dispatcher.register_handler("test-topic", 10);
        let msg = OwnedMessage::fake("test-topic", 0, 0);
        let result = dispatcher.send(msg);
        assert!(result.is_ok());
    }

    // DISP-18: Verify get_capacity returns correct capacity
    #[test]
    fn get_capacity_returns_registered_capacity() {
        let dispatcher = Dispatcher::new();
        let _rx = dispatcher.register_handler("test-topic", 42);
        assert_eq!(dispatcher.get_capacity("test-topic"), Some(42));
        assert_eq!(dispatcher.get_capacity("unknown-topic"), None);
    }

    // DISP-18: BackpressureAction variant testing
    #[test]
    fn backpressure_action_clone_eq() {
        let a1 = BackpressureAction::PausePartition {
            topic: "topic".to_string(),
            partition: 0,
        };
        let a2 = a1.clone();
        assert_eq!(a1, a2);
        assert_eq!(a1.topic(), a2.topic());
    }
}
