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

pub mod error;

pub use error::DispatchError;

use parking_lot::Mutex;
use std::collections::HashMap;
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;

pub use crate::consumer::OwnedMessage;

/// Outcome returned on successful dispatch.
#[derive(Debug, Clone)]
pub struct DispatchOutcome {
    /// Topic the message was dispatched to.
    pub topic: String,
    /// Partition the message was from.
    pub partition: i32,
    /// Offset of the message.
    pub offset: i64,
    /// Approximate queue depth at dispatch time (0 = placeholder, filled by Phase 7).
    pub queue_depth: usize,
}

/// Message dispatcher with per-topic handler queues.
pub struct Dispatcher {
    /// Maps topic name to the handler's sender half.
    /// Each sender has its own bounded channel created at registration time.
    handlers: Mutex<HashMap<String, mpsc::Sender<OwnedMessage>>>,
}

impl Dispatcher {
    /// Creates a new dispatcher with no registered handlers.
    pub fn new() -> Self {
        Self {
            handlers: Mutex::new(HashMap::new()),
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
        let (tx, rx) = mpsc::channel(capacity);
        self.handlers.lock().insert(topic.into(), tx);
        rx
    }

    /// Sends `message` to the handler registered for `message.topic`.
    ///
    /// Non-blocking — returns immediately. Returns [`DispatchError`] if:
    /// - No handler is registered for the topic ([`DispatchError::HandlerNotRegistered`])
    /// - The handler's queue is full ([`DispatchError::QueueFull`])
    /// - The handler's receiver has been dropped ([`DispatchError::QueueClosed`])
    pub fn send(&self, message: OwnedMessage) -> Result<DispatchOutcome, DispatchError> {
        let topic = message.topic.clone();
        let partition = message.partition;
        let offset = message.offset;

        let guard = self.handlers.lock();
        let sender = guard
            .get(&topic)
            .ok_or_else(|| DispatchError::HandlerNotRegistered(topic.clone()))?;

        match sender.try_send(message) {
            Ok(()) => Ok(DispatchOutcome {
                topic,
                partition,
                offset,
                queue_depth: 0, // Placeholder: Phase 7 adds real queue_len()
            }),
            Err(TrySendError::Full(_)) => Err(DispatchError::QueueFull(topic)),
            Err(TrySendError::Closed(_)) => Err(DispatchError::QueueClosed(topic)),
        }
    }
}

impl Default for Dispatcher {
    fn default() -> Self {
        Self::new()
    }
}
