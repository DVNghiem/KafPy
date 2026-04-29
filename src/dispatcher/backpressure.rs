//! Backpressure policy for handling queue-full scenarios.
//!
//! When a handler's queue is full, the dispatcher consults a [`BackpressurePolicy`]
//! to decide what action to take. The policy returns a [`BackpressureAction`].

use crate::dispatcher::queue_manager::HandlerMetadata;

/// Action to take when a handler's queue is full.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackpressureAction {
    /// Discard the message — fire-and-forget.
    /// The sender does not wait; the message is dropped.
    Drop,
    /// Block briefly and retry. Not recommended for async hot paths.
    /// Currently maps to returning Backpressure error (non-blocking per DISP-08).
    Wait,
    /// Signal to pause the partition for this topic.
    /// Carries topic and partition for targeted pause.
    PausePartition { topic: String, partition: i32 },
    /// Signal to resume the partition for this topic.
    /// Carries topic and partition for targeted resume.
    ResumePartition { topic: String, partition: i32 },
}

impl BackpressureAction {
    /// Returns the topic name if this action carries one.
    pub fn topic(&self) -> Option<&str> {
        match self {
            BackpressureAction::PausePartition { topic, .. } => Some(topic),
            BackpressureAction::ResumePartition { topic, .. } => Some(topic),
            _ => None,
        }
    }
}

/// Policy for handling backpressure when a handler queue is full.
///
/// Implementors define custom behavior when `try_send` fails with `Full`.
/// The policy receives the topic name and handler metadata for inspection.
pub(crate) trait BackpressurePolicy: Send + Sync {
    /// Called when a send attempt fails because the handler's queue is full.
    ///
    /// The handler metadata can be inspected for queue_depth, inflight, capacity.
    /// Return the action to take: Drop, Wait, or FuturePausePartition.
    fn on_queue_full(&self, topic: &str, handler: &HandlerMetadata) -> BackpressureAction;
}

/// Default backpressure policy: drop messages on queue full.
/// Safe for most use cases; messages are lost but consumer continues.
#[derive(Debug, Clone, Default)]
pub struct DefaultBackpressurePolicy;

impl BackpressurePolicy for DefaultBackpressurePolicy {
    fn on_queue_full(&self, _topic: &str, _handler: &HandlerMetadata) -> BackpressureAction {
        BackpressureAction::Drop
    }
}

/// Backpressure policy that signals pause on queue full.
/// Intended for use cases where dropping messages is unacceptable.
#[derive(Debug, Clone, Default)]
pub struct PauseOnFullPolicy;

impl BackpressurePolicy for PauseOnFullPolicy {
    fn on_queue_full(&self, topic: &str, _handler: &HandlerMetadata) -> BackpressureAction {
        BackpressureAction::PausePartition {
            topic: topic.to_string(),
            partition: -1, // -1 indicates all partitions (consumer-level pause)
        }
    }
}
