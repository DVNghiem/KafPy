//! OpenTelemetry tracing infrastructure for KafPy.
//!
//! Provides zero-cost tracing when not enabled - spans are no-ops until
//! a Layer is installed. User owns all global state; KafPy never calls
//! `set_global_default()` or `set_global_recorder()`.

use tracing::Span;


/// Span extension trait for KafPy spans.
///
/// Provides `kafpy_*` methods that create spans following the
/// `kafpy.{component}.{operation}` naming convention.
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

// Note: extract_trace_headers and inject_trace_context are deleted
// because they had zero internal references and only called Vec::new().
// Full W3C trace context implementation deferred to Phase 29 Wave 3.

// Stub to satisfy import in python/handler.rs — real implementation uses
// W3C trace context propagation across GIL boundary in Phase 29 Wave 3.
#[allow(dead_code)]
pub fn inject_trace_context(_headers: &mut std::collections::HashMap<String, String>) {}
