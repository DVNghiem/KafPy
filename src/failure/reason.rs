use thiserror::Error;

/// High-level failure category
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureCategory {
    Retryable,
    Terminal,
    NonRetryable,
}

/// Specific retryable failure kinds
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum RetryableKind {
    #[error("network timeout")]
    NetworkTimeout,
    #[error("broker unavailable")]
    BrokerUnavailable,
    #[error("transient partition error")]
    TransientPartitionError,
}

/// Specific terminal failure kinds
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TerminalKind {
    #[error("poison message detected")]
    PoisonMessage,
    #[error("deserialization failed")]
    DeserializationFailed,
    #[error("handler panic")]
    HandlerPanic,
}

/// Specific non-retryable failure kinds
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum NonRetryableKind {
    #[error("validation error")]
    ValidationError,
    #[error("business logic error")]
    BusinessLogicError,
    #[error("configuration error")]
    ConfigurationError,
}

/// Structured failure reason with category hierarchy
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureReason {
    Retryable(RetryableKind),
    Terminal(TerminalKind),
    NonRetryable(NonRetryableKind),
}

impl FailureReason {
    pub fn category(&self) -> FailureCategory {
        match self {
            FailureReason::Retryable(_) => FailureCategory::Retryable,
            FailureReason::Terminal(_) => FailureCategory::Terminal,
            FailureReason::NonRetryable(_) => FailureCategory::NonRetryable,
        }
    }
}

impl std::fmt::Display for FailureReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FailureReason::Retryable(k) => write!(f, "Retryable: {}", k),
            FailureReason::Terminal(k) => write!(f, "Terminal: {}", k),
            FailureReason::NonRetryable(k) => write!(f, "NonRetryable: {}", k),
        }
    }
}
