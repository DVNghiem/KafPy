use crate::execution::ExecutionContext;
use crate::failure::{FailureCategory, FailureReason};
use crate::log::{info, warn, error, trace};

/// Log a failure with structured context through Python's logging module.
///
/// Level is determined by failure category:
/// - Retryable: WARNING
/// - Terminal: ERROR
/// - NonRetryable: INFO
pub fn log_failure(
    context: &ExecutionContext,
    reason: &FailureReason,
    exception_name: &str,
    is_retry: bool,
) {
    let (level, msg) = match reason.category() {
        FailureCategory::Retryable => (
            "WARNING",
            format!(
                "topic={} partition={} offset={} reason={} exception={} is_retry={}",
                context.topic, context.partition, context.offset, reason, exception_name, is_retry
            ),
        ),
        FailureCategory::Terminal => (
            "ERROR",
            format!(
                "topic={} partition={} offset={} reason={} exception={}",
                context.topic, context.partition, context.offset, reason, exception_name
            ),
        ),
        FailureCategory::NonRetryable => (
            "INFO",
            format!(
                "topic={} partition={} offset={} reason={} exception={}",
                context.topic, context.partition, context.offset, reason, exception_name
            ),
        ),
    };
    match level {
        "INFO" => info!("{}", msg),
        "WARNING" => warn!("{}", msg),
        "ERROR" => error!("{}", msg),
        _ => trace!("{}", msg), // Fallback to trace for unexpected levels
    }
}
