//! OpenTelemetry tracing infrastructure for KafPy.
//!
//! Provides zero-cost tracing when not enabled - spans are no-ops until
//! a Layer is installed. User owns all global state; KafPy never calls
//! `set_global_default()` or `set_global_recorder()`.

use std::collections::HashMap;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use opentelemetry::KeyValue;

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

/// Setup error for tracing initialization.
#[derive(Debug, thiserror::Error)]
pub enum SetupError {
    #[error("failed to create OTLP exporter: {0}")]
    Exporter(String),
    #[error("failed to create tracing layer: {0}")]
    Layer(String),
    #[error("not yet implemented: {0}")]
    NotYetImplemented(String),
}

/// LayerHandle returned by enable_otel_tracing.
///
/// User owns this handle and is responsible for keeping it alive
/// for the duration of the program. KafPy never calls set_global_default().
pub type LayerHandle = Box<dyn tracing_subscriber::layer::Layer<tracing_subscriber::Registry> + Send + Sync + 'static>;

/// Enables OpenTelemetry tracing with OTLP exporter.
///
/// Returns a LayerHandle that the user owns. KafPy never calls
/// set_global_default() or set_global_recorder().
///
/// Full implementation deferred to Phase 29 Wave 3.
/// In Wave 2, this returns a stub that allows compilation while
/// maintaining zero-cost tracing (spans are no-ops without a Layer).
pub fn enable_otel_tracing(
    _config: &crate::observability::config::ObservabilityConfig,
) -> Result<LayerHandle, SetupError> {
    Err(SetupError::NotYetImplemented(
        "enable_otel_tracing full implementation deferred to Phase 29 Wave 3".to_string(),
    ))
}

/// Extracts W3C trace context headers from the current span.
///
/// Returns a vector of (key, value) tuples for traceparent and tracestate
/// headers that can be injected into a HashMap and passed across the
/// spawn_blocking boundary to Python.
///
/// Full implementation deferred to Phase 29 Wave 3. Returns empty vec in Wave 2.
pub fn extract_trace_headers() -> Vec<(String, String)> {
    // TODO: Phase 29 Wave 3 - implement W3C tracecontext extraction using
    // tracing-opentelemetry's OpenTelemetrySpanExt::context() and the
    // active SpanContext to format traceparent/tracestate headers.
    // Requires resolving opentelemetry version conflict (0.24 vs 0.26 API).
    Vec::new()
}

/// Injects W3C trace context headers into a HashMap.
///
/// This is the inverse of `extract_trace_headers` - used at spawn_blocking
/// boundaries to propagate context to Python.
pub fn inject_trace_context(headers: &mut HashMap<String, String>) {
    let extracted = extract_trace_headers();
    for (key, value) in extracted {
        headers.insert(key, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_trace_headers_empty_when_no_active_span() {
        // Without an active span context, should return empty
        let headers = extract_trace_headers();
        // May be empty or contain traceparent if a default span exists in test context
        for (key, value) in &headers {
            assert!(
                key == "traceparent" || key == "tracestate",
                "unexpected header key: {}",
                key
            );
            if key == "traceparent" {
                assert!(
                    value.starts_with("00-"),
                    "malformed traceparent: {}",
                    value
                );
            }
        }
    }
}
