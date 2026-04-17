pub mod reason;
pub mod classifier;
pub mod logging;
pub mod tests;

pub use reason::{
    FailureReason, FailureCategory, RetryableKind, TerminalKind, NonRetryableKind,
};
pub use classifier::{FailureClassifier, DefaultFailureClassifier};