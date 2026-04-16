//! Dispatcher error types.

use thiserror::Error;

/// Errors produced by the dispatcher module.
#[derive(Error, Debug)]
pub enum DispatchError {
    #[error("handler queue for topic '{0}' is full")]
    QueueFull(String),

    #[error("topic '{0}' does not exist in the Kafka cluster (not subscribed)")]
    UnknownTopic(String),

    #[error("no handler registered for topic '{0}'")]
    HandlerNotRegistered(String),

    #[error("handler queue for topic '{0}' is closed")]
    QueueClosed(String),
}
