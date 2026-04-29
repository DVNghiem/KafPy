//! Built-in metrics middleware — MIDW-03.
//!
//! Records:
//! - kafpy.handler.latency (histogram, seconds)
//! - kafpy.message.throughput (counter, per message)
//!
//! Uses existing HandlerMetrics and ThroughputMetrics types which are
//! pre-registered in SharedPrometheusSink::new().

use crate::middleware::HandlerMiddleware;
use crate::observability::metrics::{HandlerMetrics, MetricLabels, SharedPrometheusSink, ThroughputMetrics};
use crate::python::context::ExecutionContext;
use crate::python::execution_result::ExecutionResult;
use std::time::Duration;

/// Built-in metrics middleware — MIDW-03.
///
/// Records latency histogram and throughput counter per handler invocation.
/// Uses pre-registered metrics: kafpy.handler.latency and kafpy.message.throughput.
pub struct Metrics {
    sink: SharedPrometheusSink,
}

impl Metrics {
    /// Create a new Metrics middleware with the shared Prometheus sink.
    pub fn new(sink: SharedPrometheusSink) -> Self {
        Self { sink }
    }
}

impl HandlerMiddleware for Metrics {
    fn after(&self, ctx: &ExecutionContext, result: &ExecutionResult, elapsed: Duration) {
        let labels = MetricLabels::new()
            .insert("handler_id", ctx.topic.as_str())
            .insert("topic", ctx.topic.as_str());

        // Record latency histogram (available on both success and error)
        HandlerMetrics.record_latency(&self.sink, &labels, elapsed);

        // Record throughput only on success
        if result.is_ok() {
            ThroughputMetrics::record_throughput(
                &self.sink,
                ctx.topic.as_str(),
                ctx.topic.as_str(),
                "SingleSync", // mode will be plumbed through ctx in future phases
            );
        }
    }
}
