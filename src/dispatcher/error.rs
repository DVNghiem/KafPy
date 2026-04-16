//! Dispatcher error types.

use thiserror::Error;

/// Errors produced by the dispatcher module.
#[derive(Error, Debug)]
pub enum DispatchError {
    #[error("handler queue for topic '{0}' is full")]
    QueueFull(String),

    /// Backpressure error returned when the queue is full and the
    /// [`crate::dispatcher::backpressure::BackpressurePolicy`] returns
    /// [`crate::dispatcher::backpressure::BackpressureAction::Drop`] or
    /// [`crate::dispatcher::backpressure::BackpressureAction::Wait`].
    /// Distinct from [`QueueFull`] — this is the public-facing error per DISP-08.
    #[error("handler queue for topic '{0}' is full (backpressure)")]
    Backpressure(String),

    #[error("topic '{0}' does not exist in the Kafka cluster (not subscribed)")]
    UnknownTopic(String),

    #[error("no handler registered for topic '{0}'")]
    HandlerNotRegistered(String),

    #[error("handler queue for topic '{0}' is closed")]
    QueueClosed(String),
}
