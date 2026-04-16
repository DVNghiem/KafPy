//! Kafka message dispatcher core.
//!
//! Routes [`OwnedMessage`] values to per-handler bounded channels.
//!
//! ## Usage
//!
//! ```
//! use kafpy::dispatcher::{Dispatcher, DispatchOutcome};
//!
//! let dispatcher = Dispatcher::new();
//! let rx = dispatcher.register_handler("my-topic", 100);
//!
//! // In consumer loop:
//! let outcome = dispatcher.send(message).expect("handler gone");
//! ```
//!
//! ## Architecture
//!
//! Each topic gets its own [`tokio::sync::mpsc::channel`] with configurable capacity.
//! The dispatcher stores only the [`mpsc::Sender`] half; the receiver is returned
//! to the handler (Python/PyO3 layer) which pulls messages at its own pace.
//!
//! Queue depth and inflight tracking is delegated to [`QueueManager`].

pub mod backpressure;
pub mod error;
pub mod queue_manager;

pub use crate::consumer::OwnedMessage;
pub use backpressure::{BackpressureAction, DefaultBackpressurePolicy, PauseOnFullPolicy};
pub(crate) use backpressure::BackpressurePolicy;
pub use error::DispatchError;
use queue_manager::QueueManager;
use std::sync::atomic::Ordering;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;

/// Outcome returned on successful dispatch.
#[derive(Debug, Clone)]
pub struct DispatchOutcome {
    /// Topic the message was dispatched to.
    pub topic: String,
    /// Partition the message was from.
    pub partition: i32,
    /// Offset of the message.
    pub offset: i64,
    /// Approximate queue depth at dispatch time (tracked via QueueManager atomic counter).
    pub queue_depth: usize,
}

/// Message dispatcher backed by a [`QueueManager`].
pub struct Dispatcher {
    queue_manager: QueueManager,
}

impl Dispatcher {
    /// Creates a new dispatcher with no registered handlers.
    pub fn new() -> Self {
        Self {
            queue_manager: QueueManager::new(),
        }
    }

    /// Registers a handler for `topic` with a bounded queue of `capacity`.
    ///
    /// Returns a [`mpsc::Receiver`] that the handler (Python/PyO3) uses to
    /// receive messages. The receiver is owned by the caller; the dispatcher
    /// keeps only the sender.
    ///
    /// # Panics
    ///
    /// Panics if `capacity` is 0 (zero-capacity channels are not supported).
    pub fn register_handler(
        &self,
        topic: impl Into<String>,
        capacity: usize,
    ) -> mpsc::Receiver<OwnedMessage> {
        self.queue_manager
            .register_handler_with_semaphore(topic, capacity, None)
    }

    /// Registers a handler with optional semaphore for concurrency limiting.
    pub(crate) fn register_handler_with_semaphore(
        &self,
        topic: impl Into<String>,
        capacity: usize,
        semaphore: Option<Arc<tokio::sync::Semaphore>>,
    ) -> mpsc::Receiver<OwnedMessage> {
        self.queue_manager
            .register_handler_with_semaphore(topic, capacity, semaphore)
    }

    /// Sends `message` to the handler registered for `message.topic`,
    /// using the provided backpressure policy when the queue is full.
    ///
    /// Non-blocking — returns immediately. Returns [`DispatchError`] if:
    /// - No handler is registered ([`DispatchError::HandlerNotRegistered`])
    /// - Queue is full and policy returns Drop or Wait ([`DispatchError::Backpressure`])
    /// - Handler's receiver has been dropped ([`DispatchError::QueueClosed`])
    pub(crate) fn send_with_policy(
        &self,
        message: OwnedMessage,
        policy: &dyn BackpressurePolicy,
    ) -> Result<DispatchOutcome, DispatchError> {
        let topic = message.topic.clone();
        let partition = message.partition;
        let offset = message.offset;

        // Get handler entry - must be inside the lock
        let guard = self.queue_manager.handlers.lock();
        let entry = guard
            .get(&topic)
            .ok_or_else(|| DispatchError::HandlerNotRegistered(topic.clone()))?;

        // Increment inflight immediately on dispatch attempt
        entry.metadata.inflight.fetch_add(1, Ordering::Relaxed);

        match entry.sender.try_send(message) {
            Ok(()) => {
                entry.metadata.queue_depth.fetch_add(1, Ordering::Relaxed);
                let depth = entry.metadata.queue_depth.load(Ordering::Relaxed);
                Ok(DispatchOutcome {
                    topic,
                    partition,
                    offset,
                    queue_depth: depth,
                })
            }
            Err(TrySendError::Full(_)) => {
                // Decrement inflight since we're not actually dispatching
                entry.metadata.inflight.fetch_sub(1, Ordering::Relaxed);
                let action = policy.on_queue_full(&topic, &entry.metadata);
                match action {
                    BackpressureAction::Drop | BackpressureAction::Wait => {
                        Err(DispatchError::Backpressure(topic))
                    }
                    BackpressureAction::FuturePausePartition(_) => {
                        // FuturePausePartition is a signal - still return Backpressure error
                        // Actual pause/resume implementation comes in Phase 8 (DISP-18)
                        Err(DispatchError::Backpressure(topic))
                    }
                }
            }
            Err(TrySendError::Closed(_)) => {
                entry.metadata.inflight.fetch_sub(1, Ordering::Relaxed);
                Err(DispatchError::QueueClosed(topic))
            }
        }
    }

    /// Sends `message` to the handler registered for `message.topic`.
    ///
    /// Non-blocking — returns immediately. Uses [`DefaultBackpressurePolicy`] internally.
    ///
    /// Returns [`DispatchError::Backpressure`] if the queue is full (per DISP-08).
    pub fn send(&self, message: OwnedMessage) -> Result<DispatchOutcome, DispatchError> {
        self.send_with_policy(message, &DefaultBackpressurePolicy)
    }

    /// Returns the capacity for `topic`, or `None` if not registered.
    pub fn get_capacity(&self, topic: &str) -> Option<usize> {
        self.queue_manager.get_capacity(topic)
    }
}

impl Default for Dispatcher {
    fn default() -> Self {
        Self::new()
    }
}

// NOTE: ConsumerRunner import moved to lib.rs to avoid circular deps
use crate::consumer::runner::ConsumerRunner;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio_stream::StreamExt;

/// Owns ConsumerRunner + Dispatcher and orchestrates the async message loop.
/// Wires consumer stream output to dispatcher input.
pub struct ConsumerDispatcher {
    runner: Arc<ConsumerRunner>,
    dispatcher: Dispatcher,
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
    /// Uses the provided backpressure policy.
    pub async fn run(&self, policy: &dyn BackpressurePolicy) {
        let mut stream = self.runner.stream();
        while let Some(result) = stream.next().await {
            match result {
                Ok(msg) => {
                    let topic = msg.topic.clone();
                    match self.dispatcher.send_with_policy(msg, policy) {
                        Ok(outcome) => {
                            self.check_resume(&topic, outcome.queue_depth);
                        }
                        Err(DispatchError::Backpressure(_)) => {
                            tracing::warn!("backpressure on topic '{}'", topic);
                        }
                        Err(DispatchError::HandlerNotRegistered(topic)) => {
                            tracing::debug!("no handler for topic '{}', skipping", topic);
                        }
                        Err(e) => {
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

    /// Checks if a paused topic should be resumed based on current queue depth.
    fn check_resume(&self, topic: &str, current_depth: usize) {
        let Some(capacity) = self.dispatcher.get_capacity(topic) else {
            return;
        };
        let threshold = (capacity as f64 * self.resume_threshold) as usize;
        if current_depth < threshold {
            if self.paused_topics.lock().remove(topic) {
                tracing::info!(
                    "resuming topic '{}' (depth {} < threshold {})",
                    topic,
                    current_depth,
                    threshold
                );
            }
        }
    }

    /// Returns a reference to the underlying dispatcher for inspection.
    pub fn dispatcher(&self) -> &Dispatcher {
        &self.dispatcher
    }
}
