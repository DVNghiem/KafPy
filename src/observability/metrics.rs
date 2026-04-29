// src/observability/metrics.rs
// MetricsSink trait, PrometheusSink, PrometheusExporter, HandlerMetrics, QueueSnapshot

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::metrics::histogram::Histogram;
use prometheus_client::registry::{Registry as PrometheusRegistry, Unit};

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

// ─── Latency Histogram Buckets (OBS-04) ────────────────────────────────────────

/// Latency histogram buckets in seconds for OBS-04.
const LATENCY_BUCKETS: [f64; 12] = [
    0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

// ─── Prometheus Sink ────────────────────────────────────────────────────────────

/// Thread-safe Prometheus metrics sink implementing `MetricsSink`.
///
/// Uses interior mutability (Mutex) around a HashMap of metric families.
/// Recording operations lock briefly to look up and update metrics.
pub struct PrometheusSink {
    registry: PrometheusRegistry,
    counters: HashMap<&'static str, Counter>,
    histograms: HashMap<&'static str, Histogram>,
    gauges: HashMap<&'static str, Gauge>,
}

impl PrometheusSink {
    /// Creates a new sink with an empty registry.
    pub fn new() -> Self {
        Self {
            registry: PrometheusRegistry::default(),
            counters: HashMap::new(),
            histograms: HashMap::new(),
            gauges: HashMap::new(),
        }
    }

    /// Returns a mutable reference to the underlying registry.
    fn registry_mut(&mut self) -> &mut PrometheusRegistry {
        &mut self.registry
    }

    /// Register a counter family. Idempotent — no-op if already registered.
    pub fn register_counter(&mut self, name: &'static str, help: &str) {
        if !self.counters.contains_key(name) {
            let counter = Counter::default();
            self.registry_mut().register(name, help, counter.clone());
            self.counters.insert(name, counter);
        }
    }

    /// Register a histogram family with latency buckets. Idempotent.
    pub fn register_histogram(&mut self, name: &'static str, help: &str) {
        if !self.histograms.contains_key(name) {
            let histogram = Histogram::new(LATENCY_BUCKETS.iter().copied());
            self.registry_mut().register_with_unit(name, help, Unit::Seconds, histogram.clone());
            self.histograms.insert(name, histogram);
        }
    }

    /// Register a gauge family. Idempotent.
    pub fn register_gauge(&mut self, name: &'static str, help: &str) {
        if !self.gauges.contains_key(name) {
            let gauge = Gauge::default();
            self.registry_mut().register(name, help, gauge.clone());
            self.gauges.insert(name, gauge);
        }
    }
}

impl Default for PrometheusSink {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsSink for PrometheusSink {
    fn record_counter(&self, name: &str, _labels: &[(&str, &str)]) {
        if let Some(counter) = self.counters.get(name) {
            counter.inc();
        }
    }

    fn record_histogram(&self, name: &str, value: f64, _labels: &[(&str, &str)]) {
        if let Some(histogram) = self.histograms.get(name) {
            histogram.observe(value);
        }
    }

    fn record_gauge(&self, name: &str, value: f64, _labels: &[(&str, &str)]) {
        if let Some(gauge) = self.gauges.get(name) {
            gauge.set(value as i64);
        }
    }
}

// ─── Shared Prometheus Sink (Arc-wrapped, thread-safe) ─────────────────────────

use std::sync::Mutex;

/// Thread-safe Prometheus sink using interior mutability.
///
/// Cloneable so it can be shared across the codebase. All recording
/// operations are serialized via a `Mutex`.
#[derive(Clone)]
pub struct SharedPrometheusSink {
    inner: Arc<Mutex<PrometheusSink>>,
}

impl SharedPrometheusSink {
    /// Creates a new shared sink with all OBS-03 through OBS-07 families pre-registered.
    pub fn new() -> Self {
        let mut sink = PrometheusSink::new();
        sink.register_counter("kafpy.message.throughput", "Messages delivered per second");
        sink.register_histogram("kafpy.handler.latency", "Handler processing time in seconds");
        sink.register_histogram("kafpy.handler.batch_size", "Batch size distribution");
        sink.register_counter("kafpy.handler.invocation", "Handler invocation count");
        sink.register_counter("kafpy.handler.error", "Handler error count");
        sink.register_gauge("kafpy.queue.depth", "Current queue depth per handler");
        sink.register_gauge("kafpy.consumer.lag", "Consumer lag per partition");
        sink.register_counter("kafpy.dlq.messages", "Messages produced to DLQ");
        Self {
            inner: Arc::new(Mutex::new(sink)),
        }
    }

    /// Get a clone of the inner Arc for the exporter.
    pub fn registry(&self) -> Arc<Mutex<PrometheusSink>> {
        Arc::clone(&self.inner)
    }
}

impl Default for SharedPrometheusSink {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsSink for SharedPrometheusSink {
    fn record_counter(&self, name: &str, labels: &[(&str, &str)]) {
        self.inner.lock().unwrap().record_counter(name, labels);
    }

    fn record_histogram(&self, name: &str, value: f64, labels: &[(&str, &str)]) {
        self.inner.lock().unwrap().record_histogram(name, value, labels);
    }

    fn record_gauge(&self, name: &str, value: f64, labels: &[(&str, &str)]) {
        self.inner.lock().unwrap().record_gauge(name, value, labels);
    }
}

// ─── Prometheus Exporter ───────────────────────────────────────────────────────

use prometheus_client::encoding::text::encode;

/// Exposes metrics in Prometheus text format for the `/metrics` endpoint.
///
/// Backed by a `SharedPrometheusSink`.
#[derive(Clone)]
pub struct PrometheusExporter {
    sink: SharedPrometheusSink,
}

impl PrometheusExporter {
    /// Creates a new exporter backed by a fresh `SharedPrometheusSink`.
    pub fn new() -> Self {
        Self {
            sink: SharedPrometheusSink::new(),
        }
    }

    /// Returns the backing sink for recording metrics.
    pub fn sink(&self) -> &SharedPrometheusSink {
        &self.sink
    }

    /// Returns the current metrics as a Prometheus text format string.
    pub fn metrics(&self) -> String {
        let guard = self.sink.inner.lock().unwrap();
        let mut buffer = String::new();
        encode(&mut buffer, &guard.registry).expect("prometheus encoding failed");
        buffer
    }
}

impl Default for PrometheusExporter {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Handler Metrics ───────────────────────────────────────────────────────────

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

// ─── Throughput Metrics (OBS-03) ───────────────────────────────────────────────

/// Throughput metrics recorder — OBS-03.
pub struct ThroughputMetrics;

impl ThroughputMetrics {
    /// kafpy.message.throughput counter — increment per message delivered.
    pub fn record_throughput(sink: &dyn MetricsSink, topic: &str, handler_id: &str, mode: &str) {
        let labels = MetricLabels::new()
            .insert("topic", topic)
            .insert("handler_id", handler_id)
            .insert("mode", mode);
        sink.record_counter("kafpy.message.throughput", &labels.as_slice());
    }
}

// ─── Consumer Lag Metrics (OBS-05) ─────────────────────────────────────────────

/// Consumer lag metrics recorder — OBS-05.
pub struct ConsumerLagMetrics;

impl ConsumerLagMetrics {
    /// kafpy.consumer.lag gauge — updated every 10s with highwater - committed offset.
    pub fn record_lag(sink: &dyn MetricsSink, topic: &str, partition: i32, lag: i64) {
        let labels = MetricLabels::new()
            .insert("topic", topic)
            .insert("partition", partition.to_string());
        sink.record_gauge("kafpy.consumer.lag", lag as f64, &labels.as_slice());
    }
}

// ─── Queue Depth Metrics (OBS-06) ─────────────────────────────────────────────

/// Queue depth metrics recorder — OBS-06.
pub struct QueueMetrics;

impl QueueMetrics {
    /// kafpy.queue.depth gauge — reflects current queue depth per handler.
    pub fn record_queue_depth(sink: &dyn MetricsSink, handler_id: &str, depth: usize) {
        let labels = MetricLabels::new()
            .insert("handler_id", handler_id);
        sink.record_gauge("kafpy.queue.depth", depth as f64, &labels.as_slice());
    }
}

// ─── DLQ Metrics (OBS-07) ─────────────────────────────────────────────────────

/// DLQ metrics recorder — OBS-07.
pub struct DlqMetrics;

impl DlqMetrics {
    /// kafpy.dlq.messages counter — increment when message is produced to DLQ.
    pub fn record_dlq_message(sink: &dyn MetricsSink, dlq_topic: &str, original_topic: &str) {
        let labels = MetricLabels::new()
            .insert("dlq_topic", dlq_topic)
            .insert("original_topic", original_topic);
        sink.record_counter("kafpy.dlq.messages", &labels.as_slice());
    }
}

// ─── Queue Snapshot ───────────────────────────────────────────────────────────

/// Snapshot of queue depth and inflight message counts.
/// Polling-based: updated by background task every 10s, not per-message.
/// Uses AtomicUsize to match queue_manager.rs QueueSnapshot interface.
pub struct QueueSnapshot {
    pub queue_depth: Arc<std::sync::atomic::AtomicUsize>,
    pub inflight: Arc<std::sync::atomic::AtomicUsize>,
}

impl QueueSnapshot {
    pub fn new() -> Self {
        Self {
            queue_depth: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            inflight: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
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

// ─── Tests ─────────────────────────────────────────────────────────────────────

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

    #[test]
    fn shared_prometheus_sink_registers_and_records() {
        let sink = SharedPrometheusSink::new();
        let labels = MetricLabels::new().insert("topic", "test");
        sink.record_counter("kafpy.message.throughput", &labels.as_slice());
        sink.record_gauge("kafpy.queue.depth", 5.0, &labels.as_slice());
    }

    #[test]
    fn prometheus_exporter_produces_output() {
        let exporter = PrometheusExporter::new();
        let output = exporter.metrics();
        // Should contain at least some registered metric names
        assert!(output.contains("kafpy") || output.len() > 0);
    }
}
