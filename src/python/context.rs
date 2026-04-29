//! Execution context — metadata attached to each message during execution.

/// Context carried through the execution pipeline.
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub topic: String,
    pub partition: i32,
    pub offset: i64,
    pub worker_id: usize,
    /// W3C trace_id (32 hex chars from traceparent)
    pub trace_id: Option<String>,
    /// W3C span_id (16 hex chars from traceparent)
    pub span_id: Option<String>,
    /// W3C trace flags (e.g., "01")
    pub trace_flags: Option<String>,
}

impl ExecutionContext {
    pub fn new(topic: String, partition: i32, offset: i64, worker_id: usize) -> Self {
        Self {
            topic,
            partition,
            offset,
            worker_id,
            trace_id: None,
            span_id: None,
            trace_flags: None,
        }
    }

    /// Create a new ExecutionContext with trace context from W3C traceparent.
    pub fn with_trace(
        topic: String,
        partition: i32,
        offset: i64,
        worker_id: usize,
        trace_id: Option<String>,
        span_id: Option<String>,
        trace_flags: Option<String>,
    ) -> Self {
        Self {
            topic,
            partition,
            offset,
            worker_id,
            trace_id,
            span_id,
            trace_flags,
        }
    }
}
