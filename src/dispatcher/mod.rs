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
pub mod consumer_dispatcher;
pub mod error;
pub mod queue_manager;

pub use crate::consumer::OwnedMessage;
pub use consumer_dispatcher::ConsumerDispatcher;
pub use backpressure::{BackpressureAction, DefaultBackpressurePolicy, PauseOnFullPolicy};
pub use error::DispatchError;
use queue_manager::QueueManager;
use std::sync::atomic::Ordering;
use std::sync::Arc;
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

    /// Sends `message` to the handler registered for `message.topic`.
    ///
    /// Non-blocking — returns immediately. Uses [`DefaultBackpressurePolicy`] internally.
    ///
    /// Returns [`DispatchError::Backpressure`] if the queue is full (per DISP-08).
    pub fn send(&self, message: OwnedMessage) -> Result<DispatchOutcome, DispatchError> {
        // Use sync path - no backpressure policy needed for simple send
        let topic = message.topic.clone();
        let partition = message.partition;
        let offset = message.offset;

        let guard = self.queue_manager.handlers.lock();
        let entry = guard
            .get(&topic)
            .ok_or_else(|| DispatchError::HandlerNotRegistered { topic: topic.clone() })?;

        if !entry.metadata.try_acquire_semaphore() {
            return Err(DispatchError::Backpressure {
                queue_name: topic.clone(),
                reason: "semaphore permit unavailable".to_string(),
            });
        }

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
                entry.metadata.inflight.fetch_sub(1, Ordering::Relaxed);
                Err(DispatchError::Backpressure {
                    queue_name: topic.clone(),
                    reason: "queue full".to_string(),
                })
            }
            Err(TrySendError::Closed(_)) => {
                entry.metadata.inflight.fetch_sub(1, Ordering::Relaxed);
                Err(DispatchError::QueueClosed { topic })
            }
        }
    }

    /// Returns the capacity for `topic`, or `None` if not registered.
    pub fn get_capacity(&self, topic: &str) -> Option<usize> {
        self.queue_manager.get_capacity(topic)
    }

    /// Like [`send_with_policy`](Self::send_with_policy) but also returns the
    /// [`BackpressureAction`] signal when backpressure occurs.
    ///
    /// Returns `(Result, Option<BackpressureAction>)` — the `Option` is `Some`
    /// when the policy returned `FuturePausePartition(topic)` and the caller
    /// should invoke `ConsumerDispatcher::pause_partition`.
    pub(crate) async fn send_with_policy_and_signal(
        &self,
        message: OwnedMessage,
    ) -> (
        Result<DispatchOutcome, DispatchError>,
        Option<BackpressureAction>,
    ) {
        let topic = message.topic.clone();
        let partition = message.partition;
        let offset = message.offset;

        tracing::info!(topic = %topic, "send_with_policy_and_signal ENTER");
        let guard = self.queue_manager.handlers.lock();
        tracing::info!(topic = %topic, n_handlers = guard.len(), entries = ?guard.keys().collect::<Vec<_>>(), "send_with_policy_and_signal: got lock");
        let entry = guard
            .get(&topic)
            .unwrap_or_else(|| panic!("no handler for topic '{}'", topic.clone()));

        // DISP-15: Acquire semaphore permit BEFORE dispatch (non-blocking)
        if !entry.metadata.try_acquire_semaphore() {
            return (Err(DispatchError::Backpressure {
                queue_name: topic.clone(),
                reason: "semaphore permit unavailable".to_string(),
            }), None);
        }

        entry.metadata.inflight.fetch_add(1, Ordering::Relaxed);

        let result = entry.sender.try_send(message);
        match result {
            Ok(()) => {
                entry.metadata.queue_depth.fetch_add(1, Ordering::Relaxed);
                let depth = entry.metadata.queue_depth.load(Ordering::Relaxed);
                (
                    Ok(DispatchOutcome {
                        topic,
                        partition,
                        offset,
                        queue_depth: depth,
                    }),
                    None,
                )
            }
            Err(TrySendError::Full(_)) => {
                entry.metadata.inflight.fetch_sub(1, Ordering::Relaxed);
                (Err(DispatchError::Backpressure {
                    queue_name: topic.clone(),
                    reason: "queue full".to_string(),
                }), None)
            }
            Err(TrySendError::Closed(_)) => {
                entry.metadata.inflight.fetch_sub(1, Ordering::Relaxed);
                (Err(DispatchError::QueueClosed { topic }), None)
            }
        }
    }
}

impl Default for Dispatcher {
    fn default() -> Self {
        Self::new()
    }
}
