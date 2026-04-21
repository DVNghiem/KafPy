//! Consumer dispatcher that wires [`ConsumerRunner`] output to per-handler channels.
//!
//! Owns `ConsumerRunner + Dispatcher` and orchestrates the async message loop.

use crate::consumer::runner::ConsumerRunner;
use crate::consumer::OwnedMessage;
use crate::dispatcher::backpressure::{
    BackpressureAction, BackpressurePolicy,
};
use crate::dispatcher::error::DispatchError;
use crate::dispatcher::{Dispatcher, DispatchOutcome, QueueManager};
use crate::observability::tracing::KafpySpanExt;
use crate::routing::chain::RoutingChain;
use crate::routing::context::RoutingContext;
use crate::routing::decision::RoutingDecision;
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
    /// Optional routing chain for handler-based routing.
    /// When set, messages are routed by handler_id instead of topic.
    routing_chain: Option<Arc<RoutingChain>>,
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
            routing_chain: None,
        }
    }

    /// Sets the routing chain for handler-based routing.
    /// When set, messages are routed using the chain instead of by topic.
    pub fn with_routing_chain(mut self, chain: Arc<RoutingChain>) -> Self {
        self.routing_chain = Some(chain);
        self
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

    /// Registers a handler by handler ID (for routing-based dispatch).
    /// Optionally limits concurrency with `max_concurrency` semaphore permits.
    pub fn register_handler_by_id(
        &self,
        handler_id: impl Into<String>,
        capacity: usize,
        max_concurrency: Option<usize>,
    ) -> mpsc::Receiver<OwnedMessage> {
        let semaphore = max_concurrency.map(|n| Arc::new(tokio::sync::Semaphore::new(n)));
        let handler_id_str = handler_id.into();
        self.dispatcher
            .register_handler_with_semaphore(handler_id_str, capacity, semaphore)
    }

    /// Runs the dispatch loop, polling the consumer stream and
    /// dispatching each message through the dispatcher.
    /// Uses the provided backpressure policy.
    ///
    /// When a routing chain is configured, messages are first evaluated through
    /// the chain to determine the target handler_id, then dispatched to that handler.
    /// When no routing chain is set, messages are dispatched by topic (backward compat).
    pub(crate) async fn run(&self, policy: &dyn BackpressurePolicy) {
        eprintln!("DISPATCH_RUN: ENTERED");
        let mut stream = self.runner.stream();
        eprintln!("DISPATCH_RUN: STREAM_CREATED");
        while let Some(result) = stream.next().await {
            match result {
                Ok(msg) => {
                    let topic = msg.topic.clone();
                    let partition = msg.partition;
                    let offset = msg.offset;
                    // Dispatch inside span; routing_decision is derived from outcome
                    let span = tracing::Span::current().kafpy_dispatch_process(
                        &topic,
                        partition,
                        offset,
                        "dispatched",
                    );
                    let (outcome, pause_signal) = {
                        let _guard = span.enter();
                        if let Some(ref chain) = self.routing_chain {
                            self.route_with_chain(msg, chain, policy).await
                        } else {
                            self.dispatcher.send_with_policy_and_signal(msg, policy).await
                        }
                    };
                    match outcome {
                        Ok(outcome) => {
                            self.check_resume(&topic, outcome.queue_depth);
                        }
                        Err(DispatchError::Backpressure(_)) => {
                            tracing::Span::current().record("routing_decision", &"backpressure");
                            if let Some(BackpressureAction::FuturePausePartition(pause_topic)) =
                                pause_signal
                            {
                                match self.pause_partition(&pause_topic) {
                                    Ok(()) => {
                                        tracing::warn!(
                                            "paused topic '{}' due to backpressure",
                                            pause_topic
                                        );
                                        self.paused_topics.lock().insert(pause_topic.clone());
                                    }
                                    Err(e) => {
                                        tracing::error!(
                                            "failed to pause topic '{}': {}",
                                            pause_topic,
                                            e
                                        );
                                    }
                                }
                            }
                        }
                        Err(DispatchError::HandlerNotRegistered(t)) => {
                            tracing::Span::current().record("routing_decision", &"not_registered");
                            tracing::debug!("no handler for topic '{}', skipping", t);
                        }
                        Err(e) => {
                            tracing::Span::current().record("routing_decision", &"queue_closed");
                            tracing::error!("dispatch error: {}", e);
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("consumer error: {}", e);
                }
            }
        }
    }

    /// Routes a message through the routing chain and dispatches to the resulting handler.
    async fn route_with_chain(
        &self,
        msg: OwnedMessage,
        chain: &Arc<RoutingChain>,
        policy: &dyn BackpressurePolicy,
    ) -> (
        Result<DispatchOutcome, DispatchError>,
        Option<BackpressureAction>,
    ) {
        let ctx = RoutingContext::from_message(&msg);
        match chain.route(&ctx) {
            RoutingDecision::Route(handler_id) => {
                // Dispatch to handler by ID
                let qm = &self.dispatcher.queue_manager;
                // Debug: log handlers map contents before send
                {
                    let guard = qm.handlers.lock();
                    tracing::debug!(handler_id = %handler_id, topics = ?guard.keys().collect::<Vec<_>>(), "route_with_chain: handlers in QM");
                }
                match qm.send_to_handler_by_id(&handler_id, msg) {
                    Ok(outcome) => (Ok(outcome), None),
                    Err(DispatchError::Backpressure(_)) => {
                        let action = policy.on_queue_full(
                            handler_id.as_str(),
                            &qm.handlers
                                .lock()
                                .get(handler_id.as_str())
                                .map(|e| &e.metadata)
                                .unwrap_or_else(|| panic!("handler '{}' not found", handler_id)),
                        );
                        match action {
                            BackpressureAction::Drop | BackpressureAction::Wait => {
                                (Err(DispatchError::Backpressure(handler_id.to_string())), None)
                            }
                            BackpressureAction::FuturePausePartition(t) => (
                                Err(DispatchError::Backpressure(handler_id.to_string())),
                                Some(BackpressureAction::FuturePausePartition(t)),
                            ),
                        }
                    }
                    Err(e) => (Err(e), None),
                }
            }
            RoutingDecision::Drop => {
                tracing::debug!("message dropped by routing chain");
                (
                    Err(DispatchError::HandlerNotRegistered("routing-drop".into())),
                    None,
                )
            }
            RoutingDecision::Reject(reason) => {
                tracing::warn!("message rejected by routing chain: {}", reason);
                (
                    Err(DispatchError::HandlerNotRegistered("routing-reject".into())),
                    None,
                )
            }
            RoutingDecision::Defer => {
                // Should not happen with properly configured chain, but handle gracefully
                tracing::warn!("routing chain returned Defer with no fallback");
                (
                    Err(DispatchError::HandlerNotRegistered("routing-defer".into())),
                    None,
                )
            }
        }
    }

    /// Checks if a paused topic should be resumed based on current queue depth.
    fn check_resume(&self, topic: &str, current_depth: usize) {
        let Some(capacity) = self.dispatcher.get_capacity(topic) else {
            return;
        };
        let threshold = (capacity as f64 * self.resume_threshold) as usize;
        if current_depth < threshold {
            if self.paused_topics.lock().remove(topic) {
                if let Err(e) = self.resume_partition(topic) {
                    tracing::error!("failed to resume topic '{}': {}", topic, e);
                } else {
                    tracing::info!(
                        "resumed topic '{}' (depth {} < threshold {})",
                        topic,
                        current_depth,
                        threshold
                    );
                }
            }
        }
    }

    /// Pauses consumption for all partitions of `topic` via rdkafka pause().
    fn pause_partition(
        &self,
        topic: &str,
    ) -> Result<(), crate::consumer::error::ConsumerError> {
        let handles = self.partition_handles.lock();
        if let Some(tpl) = handles.get(topic) {
            self.runner.pause(tpl)
        } else {
            Err(crate::consumer::error::ConsumerError::Subscription(
                format!(
                    "no partition handle for topic '{}' - call populate_partitions() first",
                    topic
                ),
            ))
        }
    }

    /// Resumes consumption for all partitions of `topic` via rdkafka resume().
    fn resume_partition(
        &self,
        topic: &str,
    ) -> Result<(), crate::consumer::error::ConsumerError> {
        let handles = self.partition_handles.lock();
        if let Some(tpl) = handles.get(topic) {
            self.runner.resume(tpl)
        } else {
            Err(crate::consumer::error::ConsumerError::Subscription(
                format!("no partition handle for topic '{}'", topic),
            ))
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
            by_topic
                .entry(topic_name.clone())
                .or_insert_with(|| rdkafka::TopicPartitionList::new());
            let tpl = by_topic.get_mut(&topic_name).unwrap();
            tpl.add_partition_offset(topic_name.as_str(), partition, offset)
                .map_err(|e| crate::consumer::error::ConsumerError::Subscription(e.to_string()))?;
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
        // NOTE: QueueManager::clone() does Arc::new(self.handlers.clone()) which
        // creates a NEW Arc pointing to the SAME underlying HashMap (shallow clone).
        // This means both the original Dispatcher's QM and the returned clone share
        // the same handlers HashMap - inserts are visible to both.
        Arc::new(self.dispatcher.queue_manager.clone())
    }
}

#[cfg(test)]
mod tests {
    use crate::dispatcher::DefaultBackpressurePolicy;

    use super::*;

    // DISP-17: Owned types - compile-time verification
    #[test]
    fn owned_message_implements_send_and_sync() {
        fn assert_ownded<T: Send + Sync>() {}
        assert_ownded::<OwnedMessage>();
    }

    // DISP-17: OwnedMessage has no lifetimes (compile-time check via PhantomData)
    #[test]
    fn dispatch_outcome_has_no_lifetimes() {
        fn assert_owned<T: Send + Sync>() {}
        assert_owned::<DispatchOutcome>();
    }

    // DISP-18: FuturePausePartition action carries topic for pause signal
    #[test]
    fn future_pause_partition_carries_topic_name() {
        let action = BackpressureAction::FuturePausePartition("my-topic".to_string());
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
        let (result, _) = dispatcher.send_with_policy_and_signal(msg, &DefaultBackpressurePolicy).await;
        assert!(result.is_ok());

        // Second send should fail with backpressure (semaphore exhausted)
        let msg2 = OwnedMessage::fake("test-topic", 1, 101);
        let (result, _) =
            dispatcher.send_with_policy_and_signal(msg2, &DefaultBackpressurePolicy).await;
        assert!(matches!(result, Err(DispatchError::Backpressure(_))));
    }

    // DISP-15: No semaphore = unlimited concurrency (bounded by channel capacity)
    #[tokio::test]
    async fn no_semaphore_allows_unlimited_dispatch() {
        let dispatcher = Dispatcher::new();
        // Capacity 100 means we can dispatch up to 100 before backpressure
        let _rx = dispatcher.register_handler("test-topic", 100);

        for i in 0..50 {
            let msg = OwnedMessage::fake("test-topic", i % 3, i as i64);
            let (result, _) =
                dispatcher.send_with_policy_and_signal(msg, &DefaultBackpressurePolicy).await;
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
        let a1 = BackpressureAction::FuturePausePartition("topic".to_string());
        let a2 = a1.clone();
        assert_eq!(a1, a2);
        assert_eq!(a1.topic(), a2.topic());
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
