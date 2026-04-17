//! DlqRouter — trait for computing DLQ destination topic and partition.

use std::fmt;

use super::metadata::DlqMetadata;

/// Topic and partition for a DLQ message destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicPartition {
    pub topic: String,
    pub partition: i32,
}

impl TopicPartition {
    pub fn new(topic: impl Into<String>, partition: i32) -> Self {
        Self {
            topic: topic.into(),
            partition,
        }
    }
}

/// Trait for routing failed messages to DLQ topics.
///
/// Implement this trait to customize DLQ routing behavior, e.g.,
/// per-handler DLQ topics or custom partition assignment.
///
/// # Example
/// ```ignore
/// struct CustomDlqRouter {
///     fallback: DefaultDlqRouter,
/// }
///
/// impl DlqRouter for CustomDlqRouter {
///     fn route(&self, metadata: &DlqMetadata) -> TopicPartition {
///         // Custom routing logic
///     }
/// }
/// ```
pub trait DlqRouter: Send + Sync {
    /// Computes the target topic and partition for a DLQ message.
    fn route(&self, metadata: &DlqMetadata) -> TopicPartition;
}

/// Default DLQ router using configurable topic prefix.
///
/// Routes to `{dlq_topic_prefix}{original_topic}` with partition preserved.
#[derive(Debug, Clone)]
pub struct DefaultDlqRouter {
    dlq_topic_prefix: String,
}

impl DefaultDlqRouter {
    /// Creates a new DefaultDlqRouter with the given prefix.
    ///
    /// Example: `DefaultDlqRouter::new("dlq.".to_string())`
    pub fn new(dlq_topic_prefix: String) -> Self {
        Self { dlq_topic_prefix }
    }

    /// Creates a router using the default "dlq." prefix.
    pub fn with_default_prefix() -> Self {
        Self::new("dlq.".to_string())
    }
}

impl DlqRouter for DefaultDlqRouter {
    fn route(&self, metadata: &DlqMetadata) -> TopicPartition {
        let topic = format!("{}{}", self.dlq_topic_prefix, metadata.original_topic);
        TopicPartition::new(topic, metadata.original_partition)
    }
}

impl Default for DefaultDlqRouter {
    fn default() -> Self {
        Self::with_default_prefix()
    }
}

impl fmt::Display for DefaultDlqRouter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DefaultDlqRouter(prefix={:?})", self.dlq_topic_prefix)
    }
}
