// src/observability/prometheus.rs
// PrometheusMetricsSink implementation using prometheus-client 0.24

use crate::observability::metrics::MetricsSink;
use parking_lot::RwLock;
use prometheus_client::encoding::text::encode;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::metrics::histogram::Histogram;
use prometheus_client::registry::Registry;
use std::sync::Arc;

/// Histogram buckets for latency: [1ms, 5ms, 10ms, 50ms, 100ms, 500ms, 1s, 5s] in seconds.
const LATENCY_BUCKETS: &[f64] = &[0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0];

/// Prometheus metrics sink implementing the MetricsSink trait.
/// Thread-safe via Arc<RwLock<Registry>>.
pub struct PrometheusMetricsSink {
    registry: Arc<RwLock<Registry>>,
    // Per-metric handles registered with the registry
    invocation_counter: Counter,
    latency_histogram: Histogram,
    error_counter: Counter,
    batch_size_histogram: Histogram,
    queue_depth_gauge: Gauge,
    inflight_gauge: Gauge,
}

impl PrometheusMetricsSink {
    pub fn new() -> Self {
        let registry = Arc::new(RwLock::new(Registry::default()));
        Self {
            registry: Arc::clone(&registry),
            invocation_counter: Counter::default(),
            latency_histogram: Histogram::new(LATENCY_BUCKETS.iter().copied()),
            error_counter: Counter::default(),
            batch_size_histogram: Histogram::new(LATENCY_BUCKETS.iter().copied()),
            queue_depth_gauge: Gauge::default(),
            inflight_gauge: Gauge::default(),
        }
    }

    /// Register all metrics with the internal registry.
    pub fn register(&self) {
        let mut reg = self.registry.write();
        reg.register("kafpy_handler_invocation", "", self.invocation_counter.clone());
        reg.register("kafpy_handler_latency", "", self.latency_histogram.clone());
        reg.register("kafpy_handler_error", "", self.error_counter.clone());
        reg.register("kafpy_handler_batch_size", "", self.batch_size_histogram.clone());
        reg.register("kafpy_queue_depth", "", self.queue_depth_gauge.clone());
        reg.register("kafpy_queue_inflight", "", self.inflight_gauge.clone());
    }

    /// Encode all metrics to Prometheus exposition text format.
    pub fn encode(&self) -> String {
        let reg = self.registry.read();
        let mut output = String::new();
        encode(&mut output, &reg).expect("Prometheus encoding should not fail");
        output
    }
}

impl Default for PrometheusMetricsSink {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsSink for PrometheusMetricsSink {
    fn record_counter(&self, name: &str, _labels: &[(&str, &str)]) {
        let _reg = self.registry.read();
        // Match on name to update correct metric
        match name {
            "kafpy.handler.invocation" => {
                self.invocation_counter.inc();
            }
            "kafpy.handler.error" => {
                self.error_counter.inc();
            }
            _ => {}
        }
    }

    fn record_histogram(&self, name: &str, value: f64, _labels: &[(&str, &str)]) {
        match name {
            "kafpy.handler.latency" => {
                self.latency_histogram.observe(value);
            }
            "kafpy.handler.batch_size" => {
                self.batch_size_histogram.observe(value);
            }
            _ => {}
        }
    }

    fn record_gauge(&self, name: &str, value: f64, _labels: &[(&str, &str)]) {
        match name {
            "kafpy.queue.depth" => {
                self.queue_depth_gauge.set(value as i64);
            }
            "kafpy.queue.inflight" => {
                self.inflight_gauge.set(value as i64);
            }
            _ => {}
        }
    }
}

unsafe impl Send for PrometheusMetricsSink {}
unsafe impl Sync for PrometheusMetricsSink {}
