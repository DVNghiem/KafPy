use rdkafka::error::KafkaError;
use thiserror::Error;

use super::config::BuildError;

/// Errors produced by the consumer module.
#[derive(Error, Debug)]
pub enum ConsumerError {
    #[error("kafka error: {0}")]
    Kafka(#[from] KafkaError),

    #[error("subscription error: {0}")]
    Subscription(String),

    #[error("message receive error: {0}")]
    Receive(String),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("message processing error: {0}")]
    Processing(String),

    #[error("consumer not started")]
    NotStarted,

    #[error("consumer already started")]
    AlreadyStarted,

    #[error("config error: {0}")]
    Config(#[from] BuildError),
}
