//! Execution result types — normalized outcome of Python handler execution.

/// Normalized execution result from a Python handler.
#[derive(Debug, Clone)]
pub enum ExecutionResult {
    /// Handler executed successfully.
    Ok,
    /// Python exception raised during execution.
    Error {
        /// Exception type name.
        exception: String,
        /// Formatted traceback string.
        traceback: String,
    },
    /// Handler explicitly rejected the message.
    Rejected { reason: String },
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
}
