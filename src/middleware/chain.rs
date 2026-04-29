//! Middleware chain — composes multiple HandlerMiddleware in order.
//!
//! before() executes in natural order (0..N).
//! after()/on_error() execute in reverse order (N..0) — inner-to-outer decorator pattern.

use crate::python::context::ExecutionContext;
use crate::python::execution_result::ExecutionResult;
use std::time::Duration;

use super::traits::HandlerMiddleware;

/// Holds ordered middleware and delegates through them.
/// before() executes in natural order (0..N).
/// after()/on_error() execute in reverse order (N..0) — inner-to-outer decorator pattern.
pub struct MiddlewareChain {
    middleware: Vec<Box<dyn HandlerMiddleware>>,
}

impl MiddlewareChain {
    /// Creates a new empty middleware chain.
    pub fn new() -> Self {
        Self { middleware: Vec::new() }
    }

    /// Add middleware to the chain. Returns self for builder-style chaining.
    pub fn add(mut self, m: Box<dyn HandlerMiddleware>) -> Self {
        self.middleware.push(m);
        self
    }

    /// Returns true if the chain has no middleware.
    pub fn is_empty(&self) -> bool {
        self.middleware.is_empty()
    }

    /// Call before() on all middleware in natural order (outer-to-inner).
    pub fn before_all(&self, ctx: &ExecutionContext) {
        for m in &self.middleware {
            m.before(ctx);
        }
    }

    /// Call after() on all middleware in reverse order (inner-to-outer).
    pub fn after_all(&self, ctx: &ExecutionContext, result: &ExecutionResult, elapsed: Duration) {
        for m in self.middleware.iter().rev() {
            m.after(ctx, result, elapsed);
        }
    }

    /// Call on_error() on all middleware in reverse order (inner-to-outer).
    pub fn on_error_all(&self, ctx: &ExecutionContext, result: &ExecutionResult) {
        for m in self.middleware.iter().rev() {
            m.on_error(ctx, result);
        }
    }
}

impl Default for MiddlewareChain {
    fn default() -> Self {
        Self::new()
    }
}