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
        handler_name: &str,
        topic: &str,
        partition: i32,
        offset: i64,
        mode: &str,
        attempt: u32,
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
        handler_name: &str,
        topic: &str,
        partition: i32,
        offset: i64,
        mode: &str,
        attempt: u32,
    ) -> Span {
        tracing::info_span!(
            "kafpy.handler.invoke",
            handler_id = handler_id,
            handler_name = handler_name,
            topic = topic,
            partition = partition,
            offset = offset,
            mode = mode,
            attempt = attempt,
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

/// Parse W3C traceparent header and inject trace_id + span_id into the output map.
///
/// Format: 00-{trace_id:32}-{span_id:16}-{flags:2}
/// Examples:
///   00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01
///   00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01
///
/// Set `trace_flags` from the third segment (e.g. "01").
/// Also handles `tracestate` header if present.
pub fn inject_trace_context(
    headers: &std::collections::HashMap<String, String>,
    out: &mut std::collections::HashMap<String, String>,
) {
    // W3C traceparent header name
    if let Some(traceparent) = headers.get("traceparent") {
        let parts: Vec<&str> = traceparent.split('-').collect();
        if parts.len() == 4 {
            // parts[0] = version (always "00" for this spec)
            // parts[1] = trace_id (32 hex chars)
            // parts[2] = span_id (16 hex chars)
            // parts[3] = flags (e.g., "01")
            out.insert("trace_id".to_string(), parts[1].to_string());
            out.insert("span_id".to_string(), parts[2].to_string());
            out.insert("trace_flags".to_string(), parts[3].to_string());
        }
    }
    // Also handle tracestate if present
    if let Some(tracestate) = headers.get("tracestate") {
        out.insert("tracestate".to_string(), tracestate.clone());
    }
}

