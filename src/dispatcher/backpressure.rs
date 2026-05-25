//! Backpressure action types for handling queue-full scenarios.

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

/// Default backpressure policy marker.
#[derive(Debug, Clone, Default)]
pub struct DefaultBackpressurePolicy;
