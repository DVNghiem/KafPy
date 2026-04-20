// src/observability/metrics.rs
// MetricsSink trait, MetricLabels, HandlerMetrics, QueueSnapshot

use std::time::Duration;

/// User-implemented trait for plugging in custom metrics backends.
/// All methods are sync-only; the metrics crate facade handles zero-cost noop
/// when no recorder is installed.
pub trait MetricsSink: Send + Sync {
    fn record_counter(&self, name: &str, labels: &[(&str, &str)]);
    fn record_histogram(&self, name: &str, value: f64, labels: &[(&str, &str)]);
    fn record_gauge(&self, name: &str, value: f64, labels: &[(&str, &str)]);
}

/// Labels for metrics emissions, enforcing lexicographically sorted key order
/// to prevent cardinality explosion (Pitfall 5).
#[derive(Default)]
pub struct MetricLabels(Vec<(&'static str, String)>);

impl MetricLabels {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Insert a label, sorts lexicographically by key before returning.
    pub fn insert(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.0.push((key, value.into()));
        self.0.sort_by_key(|(k, _)| *k);
        self
    }

    /// Returns owned Vec of borrowed pairs for metrics crate macro consumption.
    pub fn as_slice(&self) -> Vec<(&str, &str)> {
        self.0
            .iter()
            .map(|(k, v)| (*k, v.as_str()))
            .collect()
    }
}

/// Wraps handler invocation counter, latency histogram, error counter, and batch size histogram.
pub struct HandlerMetrics;

impl HandlerMetrics {
    /// Record invocation counter: kafpy.handler.invocation
    pub fn record_invocation(&self, sink: &dyn MetricsSink, labels: &MetricLabels) {
        sink.record_counter("kafpy.handler.invocation", &labels.as_slice());
    }

    /// Record latency histogram: kafpy.handler.latency (seconds)
    pub fn record_latency(&self, sink: &dyn MetricsSink, labels: &MetricLabels, elapsed: Duration) {
        sink.record_histogram("kafpy.handler.latency", elapsed.as_secs_f64(), &labels.as_slice());
    }

    /// Record error counter: kafpy.handler.error
    pub fn record_error(&self, sink: &dyn MetricsSink, labels: &MetricLabels) {
        sink.record_counter("kafpy.handler.error", &labels.as_slice());
    }

    /// Record batch size histogram: kafpy.handler.batch_size
    pub fn record_batch_size(&self, sink: &dyn MetricsSink, labels: &MetricLabels, size: usize) {
        sink.record_histogram("kafpy.handler.batch_size", size as f64, &labels.as_slice());
    }
}

/// Snapshot of queue depth and inflight message counts.
/// Polling-based: updated by background task every 10s, not per-message.
pub struct QueueSnapshot {
    pub queue_depth: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    pub inflight: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl QueueSnapshot {
    pub fn new() -> Self {
        Self {
            queue_depth: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            inflight: std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }
}

impl Default for QueueSnapshot {
    fn default() -> Self {
        Self::new()
    }
}

/// No-op metrics sink used when no metrics backend is configured.
///
/// This is a zero-cost abstraction — all methods are no-ops.
pub mod noop_sink {
    /// A metrics sink that discards all recordings.
    pub struct NoopSink;

    impl super::MetricsSink for NoopSink {
        fn record_counter(&self, _name: &str, _labels: &[(&str, &str)]) {}
        fn record_histogram(&self, _name: &str, _value: f64, _labels: &[(&str, &str)]) {}
        fn record_gauge(&self, _name: &str, _value: f64, _labels: &[(&str, &str)]) {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metric_labels_sorts_lexicographically() {
        let labels = MetricLabels::new()
            .insert("topic", "my-topic")
            .insert("handler_id", "h1")
            .insert("mode", "BatchSync");

        let slice = labels.as_slice();
        let keys: Vec<&str> = slice.iter().map(|(k, _)| *k).collect();
        assert_eq!(keys, vec!["handler_id", "mode", "topic"]);
    }

    #[test]
    fn metric_labels_empty() {
        let labels = MetricLabels::new();
        assert!(labels.as_slice().is_empty());
    }
}
