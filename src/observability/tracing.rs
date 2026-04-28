//! Span context for trace propagation across Rust-Python boundary.
//!
//! Provides span extension trait for KafPy. Creates spans using tracing
//! but logs through Python. The tracing spans are zero-cost when no subscriber
//! is configured, and Python logging handles output formatting.

use tracing::Span;

/// Span extension trait for KafPy spans.
/// 
/// Creates spans that will be visible through Python logging configuration.
pub trait KafpySpanExt {
    /// Creates a `kafpy.handler.invoke` span.
    fn kafpy_handler_invoke(
        &self,
        handler_id: &str,
        topic: &str,
        partition: i32,
        offset: i64,
        mode: &str,
    ) -> Span;

    /// Creates a `kafpy.dispatch.process` span.
    fn kafpy_dispatch_process(
        &self,
        topic: &str,
        partition: i32,
        offset: i64,
        routing_decision: &str,
    ) -> Span;

    /// Creates a `kafpy.dlq.route` span.
    fn kafpy_dlq_route(&self, handler_id: &str, reason: &str, partition: i32) -> Span;
}

impl KafpySpanExt for Span {
    fn kafpy_handler_invoke(
        &self,
        handler_id: &str,
        topic: &str,
        partition: i32,
        offset: i64,
        mode: &str,
    ) -> Span {
        tracing::info_span!(
            "kafpy.handler.invoke",
            handler_id = handler_id,
            topic = topic,
            partition = partition,
            offset = offset,
            mode = mode,
        )
    }

    fn kafpy_dispatch_process(
        &self,
        topic: &str,
        partition: i32,
        offset: i64,
        routing_decision: &str,
    ) -> Span {
        tracing::info_span!(
            "kafpy.dispatch.process",
            topic = topic,
            partition = partition,
            offset = offset,
            routing_decision = routing_decision,
        )
    }

    fn kafpy_dlq_route(&self, handler_id: &str, reason: &str, partition: i32) -> Span {
        tracing::info_span!(
            "kafpy.dlq.route",
            handler_id = handler_id,
            reason = reason,
            partition = partition,
        )
    }
}

/// Inject trace context into headers.
/// 
/// This is a no-op — trace context is handled through Python logging infrastructure.
#[allow(dead_code)]
pub fn inject_trace_context(_headers: &mut std::collections::HashMap<String, String>) {}

