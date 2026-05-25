//! W3C trace context propagation utilities.
//!
//! Parses W3C `traceparent` / `tracestate` headers from Kafka message headers
//! and extracts trace_id, span_id, and trace_flags for forwarding into Python
//! handler execution context.

/// Parse W3C traceparent header and inject trace_id + span_id into the output map.
///
/// Format: 00-{trace_id:32}-{span_id:16}-{flags:2}
/// Examples:
///   00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01
///   00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01
pub fn inject_trace_context(
    headers: &std::collections::HashMap<String, String>,
    out: &mut std::collections::HashMap<String, String>,
) {
    if let Some(traceparent) = headers.get("traceparent") {
        let parts: Vec<&str> = traceparent.split('-').collect();
        if parts.len() == 4 {
            out.insert("trace_id".to_string(), parts[1].to_string());
            out.insert("span_id".to_string(), parts[2].to_string());
            out.insert("trace_flags".to_string(), parts[3].to_string());
        }
    }
    if let Some(tracestate) = headers.get("tracestate") {
        out.insert("tracestate".to_string(), tracestate.clone());
    }
}
