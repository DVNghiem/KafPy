//! DlqMetadata — structured envelope for DLQ messages.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Metadata envelope attached to every DLQ message.
///
/// Contains the original message context plus failure information
/// needed for debugging, replay, and alerting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DlqMetadata {
    /// Original Kafka topic the message came from.
    pub original_topic: String,
    /// Original partition the message was assigned to.
    pub original_partition: i32,
    /// Original offset of the message.
    pub original_offset: i64,
    /// Why the message was sent to DLQ.
    pub failure_reason: String,
    /// How many times the handler was invoked before DLQ routing.
    pub attempt_count: u32,
    /// When the first failure occurred (UTC).
    pub first_failure_timestamp: DateTime<Utc>,
    /// When the most recent failure occurred (UTC).
    pub last_failure_timestamp: DateTime<Utc>,
    /// Timeout duration in seconds when a handler timeout triggered DLQ routing.
    /// None when routing for non-timeout reasons.
    pub timeout_duration: Option<u64>,
    /// Offset of the last message the handler completed before timeout fired.
    /// None when not trackable or for non-timeout reasons.
    pub last_processed_offset: Option<i64>,
}

impl DlqMetadata {
    /// Creates a new DlqMetadata with the given fields.
    pub fn new(
        original_topic: String,
        original_partition: i32,
        original_offset: i64,
        failure_reason: String,
        attempt_count: u32,
        first_failure_timestamp: DateTime<Utc>,
        last_failure_timestamp: DateTime<Utc>,
        timeout_duration: Option<u64>,
        last_processed_offset: Option<i64>,
    ) -> Self {
        Self {
            original_topic,
            original_partition,
            original_offset,
            failure_reason,
            attempt_count,
            first_failure_timestamp,
            last_failure_timestamp,
            timeout_duration,
            last_processed_offset,
        }
    }
}
