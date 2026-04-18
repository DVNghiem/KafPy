// src/observability/mod.rs
// Module re-exports for observability infrastructure

pub mod config;
pub mod kafka_metrics;
pub mod metrics;
pub mod prometheus;
pub mod tracing;

pub use config::{LogFormat, ObservabilityConfig};
pub use kafka_metrics::{KafkaMetrics, KafkaMetricsSnapshot, KafkaMetricsTask, OffsetCommitLatencyTracker};
pub use metrics::{HandlerMetrics, MetricLabels, MetricsSink, QueueSnapshot};
pub use prometheus::PrometheusMetricsSink;
pub use tracing::{
    extract_trace_headers, inject_trace_context, KafpySpanExt,
};
