// src/observability/mod.rs
// Module re-exports for observability infrastructure

pub mod config;
pub mod metrics;
pub mod runtime_snapshot;
pub mod tracing;

pub use config::{LogFormat, ObservabilityConfig};
pub use metrics::MetricLabels;
pub use metrics::noop_sink::NoopSink;
