// src/observability/mod.rs
// Module re-exports for observability infrastructure

pub mod config;
pub mod kafka_metrics;
pub mod metrics;
pub mod prometheus;
pub mod runtime_snapshot;
pub mod tracing;

pub use config::{LogFormat, ObservabilityConfig};
pub use metrics::MetricLabels;
pub use runtime_snapshot::RuntimeSnapshotTask;
