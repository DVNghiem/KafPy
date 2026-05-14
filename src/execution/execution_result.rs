//! Execution result types — normalized outcome of Python handler execution.

use crate::failure::FailureReason;

/// Information captured when a handler times out.
/// Carried through ExecutionResult so handle_execution_failure can populate DLQ metadata.
#[derive(Debug, Clone)]
pub struct TimeoutInfo {
    /// The configured timeout duration in milliseconds.
    pub timeout_ms: u64,
    /// The offset of the last message the handler completed before the timeout
    /// (i.e., original_offset - 1 for single-message handlers, None if not trackable).
    pub last_processed_offset: Option<i64>,
}

/// Normalized execution result from a Python handler.
#[derive(Debug, Clone)]
pub enum ExecutionResult {
    /// Handler executed successfully.
    Ok,
    /// Python exception raised during execution.
    Error {
        /// Failure reason classification.
        reason: FailureReason,
        /// Exception type name.
        exception: String,
        /// Formatted traceback string.
        traceback: String,
    },
    /// Handler explicitly rejected the message.
    Rejected {
        /// Failure reason classification.
        reason: FailureReason,
        /// Rejection reason string.
        reason_str: String,
    },
    /// Handler invocation timed out — carries TimeoutInfo so DLQ metadata can be enriched.
    Timeout {
        info: TimeoutInfo,
    },
}

impl ExecutionResult {
    pub fn is_ok(&self) -> bool {
        matches!(self, ExecutionResult::Ok)
    }
    pub fn is_error(&self) -> bool {
        matches!(self, ExecutionResult::Error { .. })
    }
    pub fn is_rejected(&self) -> bool {
        matches!(self, ExecutionResult::Rejected { .. })
    }
    pub fn is_timeout(&self) -> bool {
        matches!(self, ExecutionResult::Timeout { .. })
    }

    /// Returns the error type label for metrics recording.
    pub fn error_type_label(&self) -> &'static str {
        match self {
            ExecutionResult::Ok => "ok",
            ExecutionResult::Error { .. } => "error",
            ExecutionResult::Rejected { .. } => "rejected",
            ExecutionResult::Timeout { .. } => "timeout",
        }
    }
}

/// Batch execution result from a BatchSync or BatchAsync Python handler.
///
/// # Variants
/// - `AllSuccess(Vec<Offset>)` — every message in the batch succeeded; record_ack per offset
/// - `AllFailure(FailureReason)` — entire batch failed with same reason; route all to RetryCoordinator
/// - `PartialFailure` — **NOT IMPLEMENTED in v1.6** — extension point for v1.7+
///   (per-message outcome tracking within batches)
#[derive(Debug, Clone)]
pub enum BatchExecutionResult {
    /// Every message in the batch succeeded. Carries offsets for each message (same order as input batch).
    AllSuccess(Vec<i64>),
    /// Entire batch failed with the same reason. All messages route to RetryCoordinator.
    AllFailure(FailureReason),
    /// **EXTENSION POINT — NOT IMPLEMENTED in v1.6**
    /// Per-message outcome within batch. Implementation requires EXEC-10 to track individual offsets.
    /// Tracked as issue in deferred items for v1.7+.
    PartialFailure {
        successful_offsets: Vec<i64>,
        failed_messages: Vec<(String, i32, i64, FailureReason)>, // (topic, partition, offset, reason)
    },
}

impl BatchExecutionResult {
    /// Returns true if this is AllSuccess.
    pub fn is_all_success(&self) -> bool {
        matches!(self, BatchExecutionResult::AllSuccess(_))
    }

    /// Returns true if this is AllFailure.
    pub fn is_all_failure(&self) -> bool {
        matches!(self, BatchExecutionResult::AllFailure(_))
    }
}
