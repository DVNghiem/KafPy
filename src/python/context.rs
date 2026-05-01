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
    /// Fan-out branch ID. None when not a fan-out branch.
    pub branch_id: Option<u64>,
    /// Fan-out dispatch ID (unique per primary message). None when not a fan-out dispatch.
    pub fan_out_id: Option<u64>,
    /// Source topic for fan-in messages. Set when message arrived from a different topic
    /// than ctx.topic (e.g., round-robin multiplexed handler). For non-fan-in handlers,
    /// defaults to empty string.
    pub source_topic: String,
    /// Fan-in group ID. Set when the handler was registered via register_fanin().
    /// None when not a fan-in handler.
    pub fan_in_id: Option<u64>,
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
            branch_id: None,
            fan_out_id: None,
            source_topic: String::new(),
            fan_in_id: None,
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
        branch_id: Option<u64>,
        fan_out_id: Option<u64>,
        source_topic: String,
        fan_in_id: Option<u64>,
    ) -> Self {
        Self {
            topic,
            partition,
            offset,
            worker_id,
            trace_id,
            span_id,
            trace_flags,
            branch_id,
            fan_out_id,
            source_topic,
            fan_in_id,
        }
    }
}
