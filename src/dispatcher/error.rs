//! Dispatcher error types.

use thiserror::Error;

/// Errors produced by the dispatcher module.
#[derive(Error, Debug)]
pub enum DispatchError {
    #[error("handler queue '{queue_name}' is full (capacity: {capacity})")]
    QueueFull { queue_name: String, capacity: usize },

    /// Backpressure error returned when dispatch cannot enqueue a message.
    /// Distinct from [`QueueFull`] — this is the public-facing error per DISP-08.
    #[error("backpressure on handler queue '{queue_name}': {reason}")]
    Backpressure { queue_name: String, reason: String },

    #[error("topic '{topic}' not found in Kafka cluster (broker: {broker})")]
    UnknownTopic { topic: String, broker: String },

    #[error("no handler registered for topic '{topic}'")]
    HandlerNotRegistered { topic: String },

    #[error("handler queue for topic '{topic}' is closed")]
    QueueClosed { topic: String },
}
