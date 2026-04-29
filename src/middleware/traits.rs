//! Handler middleware trait for cross-cutting concerns.
//!
//! Middleware wraps Python handler invocation to add logging, metrics, retries,
//! and other cross-cutting behavior without duplicating handler code.

use crate::python::context::ExecutionContext;
use crate::python::execution_result::ExecutionResult;
use std::time::Duration;

/// User-implemented middleware trait for cross-cutting handler concerns.
/// All methods are optional — default implementations are no-ops.
/// Middleware are composed in a chain; after/on_error run in reverse order.
pub trait HandlerMiddleware: Send + Sync {
    /// Called before the handler is invoked.
    /// Use for: pre-processing, span creation, resource acquisition.
    fn before(&self, _ctx: &ExecutionContext) {}

    /// Called after the handler succeeds or fails.
    /// Use for: logging, metrics recording, cleanup.
    /// `elapsed` is the total wall-clock time since before() was called.
    fn after(&self, _ctx: &ExecutionContext, _result: &ExecutionResult, _elapsed: Duration) {}

    /// Called when the handler invocation returns an error result.
    /// Default implementation calls `after` for consistent middleware that handle both success and error.
    fn on_error(&self, _ctx: &ExecutionContext, _result: &ExecutionResult) {}
}