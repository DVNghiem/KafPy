use crate::python::ExecutionContext;
use pyo3::PyErr;
use super::reason::{FailureReason, RetryableKind, TerminalKind, NonRetryableKind};

/// Trait for classifying Python exceptions into FailureReason
pub trait FailureClassifier: Send + Sync {
    fn classify(&self, error: &PyErr, context: &ExecutionContext) -> FailureReason;
}

/// Default classifier that maps Python exceptions to FailureReason
#[derive(Debug, Default)]
pub struct DefaultFailureClassifier;

impl FailureClassifier for DefaultFailureClassifier {
    fn classify(&self, error: &PyErr, _context: &ExecutionContext) -> FailureReason {
        // Use exception type name via debug representation
        let error_repr = format!("{:?}", error);
        let traceback = error.to_string();

        if error_repr.contains("TimeoutError") || traceback.contains("timeout") {
            FailureReason::Retryable(RetryableKind::NetworkTimeout)
        } else if error_repr.contains("KafkaError") || error_repr.contains("BrokerTransport") {
            FailureReason::Retryable(RetryableKind::BrokerUnavailable)
        } else if error_repr.contains("PartitionError") {
            FailureReason::Retryable(RetryableKind::TransientPartitionError)
        } else if error_repr.contains("PoisonError") || traceback.contains("poison") {
            FailureReason::Terminal(TerminalKind::PoisonMessage)
        } else if error_repr.contains("DeserializationError") || error_repr.contains("DecodeError") {
            FailureReason::Terminal(TerminalKind::DeserializationFailed)
        } else if error_repr.contains("PanicException") || error_repr.contains("SystemExit") {
            FailureReason::Terminal(TerminalKind::HandlerPanic)
        } else if error_repr.contains("ValidationError") || error_repr.contains("ValueError") {
            FailureReason::NonRetryable(NonRetryableKind::ValidationError)
        } else if error_repr.contains("BusinessLogicError") || error_repr.contains("DomainException") {
            FailureReason::NonRetryable(NonRetryableKind::BusinessLogicError)
        } else if error_repr.contains("ConfigError") || error_repr.contains("ConfigurationError") {
            FailureReason::NonRetryable(NonRetryableKind::ConfigurationError)
        } else {
            FailureReason::NonRetryable(NonRetryableKind::BusinessLogicError)
        }
    }
}