// src/observability/mod.rs
// Module re-exports for observability metrics infrastructure

pub mod metrics;
pub mod prometheus;

pub use metrics::{HandlerMetrics, MetricLabels, MetricsSink, QueueSnapshot};
pub use prometheus::PrometheusMetricsSink;
