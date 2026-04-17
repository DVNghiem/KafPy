use crate::python::ExecutionContext;
use crate::failure::{FailureReason, FailureCategory};

/// Log a failure with structured context at appropriate level
pub fn log_failure(context: &ExecutionContext, reason: &FailureReason, exception_name: &str, is_retry: bool) {
    match reason.category() {
        FailureCategory::Retryable => {
            tracing::warn!(
                target: "kafpy::failure",
                topic = %context.topic,
                partition = context.partition,
                offset = context.offset,
                reason = %reason,
                exception = %exception_name,
                is_retry = is_retry,
                "Retryable failure in handler"
            );
        }
        FailureCategory::Terminal => {
            tracing::error!(
                target: "kafpy::failure",
                topic = %context.topic,
                partition = context.partition,
                offset = context.offset,
                reason = %reason,
                exception = %exception_name,
                "Terminal failure — will not be retried"
            );
        }
        FailureCategory::NonRetryable => {
            tracing::info!(
                target: "kafpy::failure",
                topic = %context.topic,
                partition = context.partition,
                offset = context.offset,
                reason = %reason,
                exception = %exception_name,
                "Non-retryable failure"
            );
        }
    }
}