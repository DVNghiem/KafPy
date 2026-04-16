//! Execution policy — pluggable strategies for handling execution results.

use crate::dispatcher::OwnedMessage;
use crate::python::context::ExecutionContext;
use crate::python::execution_result::ExecutionResult;

/// Outcome of an executor's decision on what to do after execution.
#[derive(Debug, Clone)]
pub enum ExecutorOutcome {
    Ack,
    Retry,
    Rejected,
}

/// Policy trait for post-execution decisions.
pub trait Executor: Send + Sync {
    fn execute(
        &self,
        ctx: &ExecutionContext,
        _message: &OwnedMessage,
        result: &ExecutionResult,
    ) -> ExecutorOutcome;
}

/// Fire-and-forget executor. Always returns Ack.
#[derive(Debug, Clone, Default)]
pub struct DefaultExecutor;

impl Executor for DefaultExecutor {
    fn execute(&self, ctx: &ExecutionContext, _message: &OwnedMessage, result: &ExecutionResult) -> ExecutorOutcome {
        match result {
            ExecutionResult::Ok => {
                tracing::trace!(
                    topic = %ctx.topic,
                    partition = ctx.partition,
                    offset = ctx.offset,
                    worker_id = ctx.worker_id,
                    "handler executed successfully"
                );
            }
            ExecutionResult::Error { exception, .. } => {
                tracing::warn!(
                    topic = %ctx.topic,
                    partition = ctx.partition,
                    offset = ctx.offset,
                    worker_id = ctx.worker_id,
                    exception = %exception,
                    "handler raised exception"
                );
            }
            ExecutionResult::Rejected { reason } => {
                tracing::warn!(
                    topic = %ctx.topic,
                    partition = ctx.partition,
                    offset = ctx.offset,
                    worker_id = ctx.worker_id,
                    reason = %reason,
                    "handler rejected message"
                );
            }
        }
        ExecutorOutcome::Ack
    }
}

// ─── Placeholder trait interfaces ────────────────────────────────────────────

/// Placeholder for retry policy (not implemented).
pub trait RetryExecutor: Executor {}

/// Placeholder for offset tracking (not implemented).
pub trait OffsetAck: Send + Sync {}

/// Placeholder for async Python handler support (not implemented).
pub trait AsyncHandler: Send + Sync {}
