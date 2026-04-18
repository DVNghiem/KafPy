// src/observability/mod.rs
// Module re-exports for observability infrastructure

pub mod config;
pub mod kafka_metrics;
pub mod metrics;
pub mod prometheus;
pub mod runtime_snapshot;
pub mod tracing;

pub use config::{LogFormat, ObservabilityConfig};
pub use kafka_metrics::{KafkaMetrics, KafkaMetricsSnapshot, KafkaMetricsTask, OffsetCommitLatencyTracker};
pub use metrics::{HandlerMetrics, MetricLabels, MetricsSink, QueueSnapshot};
pub use prometheus::PrometheusMetricsSink;
pub use runtime_snapshot::{
    RuntimeSnapshot, RuntimeSnapshotTask, WorkerPoolState, StatusCallbackRegistry,
    WorkerState, WorkerStatus, QueueDepthInfo, AccumulatorInfo,
    ConsumerLagSummary, TopicLagInfo, PartitionLagInfo,
    get_current_snapshot, get_callback_registry,
};
pub use tracing::{
    extract_trace_headers, inject_trace_context, KafpySpanExt,
};
