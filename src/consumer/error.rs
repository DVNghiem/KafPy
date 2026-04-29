use rdkafka::error::KafkaError;
use thiserror::Error;

use super::config::BuildError;

/// Errors produced by the consumer module.
#[derive(Error, Debug)]
pub enum ConsumerError {
    #[error("kafka error: {0}")]
    Kafka(#[from] KafkaError),

    #[error("failed to subscribe to topics at broker '{broker}': {message}")]
    Subscription { broker: String, message: String },

    #[error("message receive error for handler '{handler}' after {timeout_ms}ms: {message}")]
    Receive { handler: String, timeout_ms: u64, message: String },

    #[error("deserialization failed for message at {topic}:{partition}@{offset}; bytes preview: {bytes_preview:?} — {source}")]
    Serialization {
        topic: String,
        partition: i32,
        offset: i64,
        bytes_preview: Vec<u8>,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("processing error for message at {topic}:{partition}@{offset}: {message}")]
    Processing {
        topic: String,
        partition: i32,
        offset: i64,
        message: String,
    },

    #[error("consumer not started")]
    NotStarted,

    #[error("consumer already started")]
    AlreadyStarted,

    #[error("config error: {0}")]
    Config(#[from] BuildError),
}
