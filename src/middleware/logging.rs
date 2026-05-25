//! Built-in logging middleware — MIDW-02.
//!
//! Emits log events on handler start/complete/error via the `log` crate,
//! which routes to Python's logging module through the KafPy logger bridge.

use crate::execution::context::ExecutionContext;
use crate::execution::execution_result::ExecutionResult;
use crate::log::{error, info};
use crate::middleware::HandlerMiddleware;
use std::time::Duration;

/// Built-in logging middleware — MIDW-02.
///
/// Emits span events on handler start/complete/error with trace context.
/// Reuses existing `kafpy.handler.invoke` span field names for consistency.
pub struct Logging;

impl Logging {
    /// Create a new Logging middleware instance.
    pub fn new() -> Self {
        Self
    }
}

impl Default for Logging {
    fn default() -> Self {
        Self::new()
    }
}

impl HandlerMiddleware for Logging {
    fn before(&self, ctx: &ExecutionContext) {
        info!(
            "handler middleware: before: handler_id={} topic={} partition={} offset={}",
            ctx.topic, ctx.topic, ctx.partition, ctx.offset
        );
    }

    fn after(&self, ctx: &ExecutionContext, result: &ExecutionResult, elapsed: Duration) {
        info!(
            "handler middleware: after: handler_id={} topic={} partition={} offset={} elapsed_ms={} result={}",
            ctx.topic, ctx.topic, ctx.partition, ctx.offset,
            elapsed.as_millis(), result.error_type_label()
        );
    }

    fn on_error(&self, ctx: &ExecutionContext, result: &ExecutionResult) {
        error!(
            "handler middleware: error: handler_id={} topic={} partition={} offset={} error_type={}",
            ctx.topic, ctx.topic, ctx.partition, ctx.offset, result.error_type_label()
        );
    }
}
