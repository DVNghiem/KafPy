use rdkafka::error::KafkaError;

// Kafka error wrapper
#[derive(thiserror::Error, Debug)]
#[allow(dead_code)]
pub enum KafkaConsumerError {
    #[error("Kafka error: {0}")]
    Kafka(#[from] KafkaError),

    #[error("Message processing error: {0}")]
    Processing(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

// Common error types for domain operations
#[derive(thiserror::Error, Debug)]
#[allow(dead_code)]
pub enum DomainError {
    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Business rule violation: {0}")]
    BusinessRule(String),

    #[error("Entity not found: {0}")]
    NotFound(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Processing error: {0}")]
    Processing(String),
}
