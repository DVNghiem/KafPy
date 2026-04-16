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

pub mod error;
pub mod queue_manager;

pub use crate::consumer::OwnedMessage;
pub use error::DispatchError;
use queue_manager::QueueManager;
use tokio::sync::mpsc;

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
        self.queue_manager.register_handler(topic, capacity)
    }

    /// Sends `message` to the handler registered for `message.topic`.
    ///
    /// Non-blocking — returns immediately. Returns [`DispatchError`] if:
    /// - No handler is registered for the topic ([`DispatchError::HandlerNotRegistered`])
    /// - The handler's queue is full ([`DispatchError::QueueFull`])
    /// - The handler's receiver has been dropped ([`DispatchError::QueueClosed`])
    pub fn send(&self, message: OwnedMessage) -> Result<DispatchOutcome, DispatchError> {
        self.queue_manager.send_to_handler(message)
    }
}

impl Default for Dispatcher {
    fn default() -> Self {
        Self::new()
    }
}
