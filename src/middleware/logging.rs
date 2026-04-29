//! Built-in logging middleware — MIDW-02.
//!
//! Emits tracing span events using existing kafpy_handler_invoke span fields.
//! Uses Python logging (via kafpy logger) so output goes to user's configured handler.
//! Zero-cost when no tracing subscriber is configured.

use crate::middleware::HandlerMiddleware;
use crate::python::context::ExecutionContext;
use crate::python::execution_result::ExecutionResult;
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
        // Create a span to mark handler start — zero-cost if no subscriber
        let _span = tracing::info_span!(
            "kafpy.middleware.logging",
            handler_id = %ctx.topic,
            topic = %ctx.topic,
            partition = ctx.partition,
            offset = ctx.offset,
        )
        .entered();

        tracing::info!(
            handler_id = %ctx.topic,
            topic = %ctx.topic,
            partition = ctx.partition,
            offset = ctx.offset,
            "handler middleware: before"
        );
    }

    fn after(&self, ctx: &ExecutionContext, result: &ExecutionResult, elapsed: Duration) {
        tracing::info!(
            handler_id = %ctx.topic,
            topic = %ctx.topic,
            partition = ctx.partition,
            offset = ctx.offset,
            elapsed_ms = elapsed.as_millis() as u64,
            result = %result.error_type_label(),
            "handler middleware: after"
        );
    }

    fn on_error(&self, ctx: &ExecutionContext, result: &ExecutionResult) {
        tracing::error!(
            handler_id = %ctx.topic,
            topic = %ctx.topic,
            partition = ctx.partition,
            offset = ctx.offset,
            error_type = %result.error_type_label(),
            "handler middleware: error"
        );
    }
}
