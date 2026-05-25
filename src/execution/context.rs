//! Execution context — metadata attached to each message during execution.

/// Trace context fields extracted from W3C traceparent header.
#[derive(Debug, Clone, Default)]
pub struct TraceContext {
    /// W3C trace_id (32 hex chars from traceparent)
    pub trace_id: Option<String>,
    /// W3C span_id (16 hex chars from traceparent)
    pub span_id: Option<String>,
    /// W3C trace flags (e.g., "01")
    pub trace_flags: Option<String>,
}

/// Context carried through the execution pipeline.
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub topic: String,
    pub partition: i32,
    pub offset: i64,
    pub worker_id: usize,
    /// Trace context extracted from W3C traceparent header.
    pub trace: TraceContext,
    /// Fan-out branch ID. None when not a fan-out branch.
    pub branch_id: Option<u64>,
    /// Fan-out dispatch ID (unique per primary message). None when not a fan-out dispatch.
    pub fan_out_id: Option<u64>,
    /// Source topic for fan-in messages. Set when message arrived from a different topic
    /// than ctx.topic (e.g., round-robin multiplexed handler). For non-fan-in handlers,
    /// defaults to empty string.
    pub source_topic: String,
}

impl ExecutionContext {
    pub fn new(topic: String, partition: i32, offset: i64, worker_id: usize) -> Self {
        Self {
            topic,
            partition,
            offset,
            worker_id,
            trace: TraceContext::default(),
            branch_id: None,
            fan_out_id: None,
            source_topic: String::new(),
        }
    }

    /// Create a new ExecutionContext with trace context from W3C traceparent.
    #[allow(clippy::too_many_arguments)]
    pub fn with_trace(
        topic: String,
        partition: i32,
        offset: i64,
        worker_id: usize,
        trace: TraceContext,
        branch_id: Option<u64>,
        fan_out_id: Option<u64>,
        source_topic: String,
    ) -> Self {
        Self {
            topic,
            partition,
            offset,
            worker_id,
            trace,
            branch_id,
            fan_out_id,
            source_topic,
        }
    }
}
