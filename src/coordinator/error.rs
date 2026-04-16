//! Coordinator error types.

use thiserror::Error;

/// Errors produced by the coordinator module.
#[derive(Error, Debug)]
pub enum CoordinatorError {
    #[error("topic '{0}' partition {1} not registered in offset tracker")]
    TopicPartitionNotFound(String, i32),

    #[error("offset {0} is invalid (must be >= 0)")]
    InvalidOffset(i64),
}
