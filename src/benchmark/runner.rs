// src/benchmark/runner.rs
// BenchmarkRunner — orchestrates scenario setup, warmup, measurement, teardown.

use std::fmt;
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::benchmark::measurement::{
    AggregatedStatsSnapshot, BackgroundAggregator,
};
use crate::benchmark::results::{BenchmarkResult, ScenarioConfig};
use crate::benchmark::scenarios::Scenario;
use crate::observability::metrics::MetricsSink;

/// Context passed to a Scenario during benchmark execution.
///
/// Provides the scenario with access to:
/// - Configuration parameters
/// - A MetricsSink for emitting benchmark metrics
/// - Latency recording infrastructure
pub struct BenchmarkContext {
    /// The scenario configuration for this run.
    pub config: ScenarioConfig,
    /// Optional MetricsSink for metric emission (can be null/noop).
    pub metrics_sink: Option<Arc<dyn MetricsSink>>,
    /// Background aggregator for collecting latency/throughput samples off the hot path.
    aggregator: Arc<BackgroundAggregator>,
    /// Cancellation token to signal shutdown.
    shutdown_token: CancellationToken,
}

impl BenchmarkContext {
    /// Create a new BenchmarkContext for a scenario.
    ///
    /// Sets up BackgroundAggregator with warmup exclusion based on scenario's default_warmup_messages.
    /// The metrics_sink can be None (no external metrics emitted) or Some (user-provided sink).
    pub fn new(
        scenario: &dyn Scenario,
        metrics_sink: Option<Arc<dyn MetricsSink>>,
    ) -> Self {
        let config = scenario.build_config();
        let warmup_messages = scenario.default_warmup_messages();
        let (aggregator, shutdown_token) = BackgroundAggregator::spawn(warmup_messages);

        Self {
            config,
            metrics_sink,
            aggregator,
            shutdown_token,
        }
    }

    /// Returns a sender clone for sending Sample::LatencyNs to the aggregator.
    pub fn latency_sender(&self) -> mpsc::Sender<crate::benchmark::measurement::Sample> {
        self.aggregator.sender()
    }

    /// Record a latency sample (nanoseconds).
    pub fn record_latency(&self, ns: u64) {
        let sender = self.latency_sender();
        // Non-blocking send on the hot path — best-effort, sample dropped if channel full
        let _ = sender.try_send(crate::benchmark::measurement::Sample::LatencyNs(ns));
    }

    /// Record a message throughput sample (payload bytes).
    pub fn record_message(&self, payload_bytes: usize) {
        let sender = self.latency_sender();
        let _ = sender.try_send(crate::benchmark::measurement::Sample::MessageRecorded(payload_bytes));
    }

    /// Take a snapshot of current aggregated stats (for interim reporting or final results).
    pub fn snapshot(&self) -> AggregatedStatsSnapshot {
        self.aggregator.snapshot()
    }

    /// Signal graceful shutdown — cancel the aggregator's background task.
    pub fn shutdown(&self) {
        self.shutdown_token.cancel();
    }

    /// Gracefully terminate — drain inflight messages and commit offsets before shutdown (RUN-05).
    ///
    /// This method is called during BenchmarkRunner teardown. In a full implementation,
    /// it would:
    /// 1. Wait for all in-flight messages to be acknowledged (drain the producer queue)
    /// 2. Commit consumer offsets on the Kafka consumer
    /// 3. Flush any buffered metrics to the sink
    ///
    /// For Phase 40, implement the method signature with a best-effort implementation.
    /// The actual drain-and-commit logic connects to the producer/consumer in Phase 41/42.
    pub async fn graceful_teardown(&self) -> anyhow::Result<()> {
        // Step 1: Signal shutdown to stop new message production
        self.shutdown();

        // Step 2: Give in-flight messages a bounded time to complete
        // For now, a simple yield — actual implementation needs producer.flush() in Phase 41
        tokio::task::yield_now().await;

        // Step 3: Flush metrics to sink if one is configured
        if let Some(sink) = &self.metrics_sink {
            // Emit final counter for total messages seen
            let snapshot = self.snapshot();
            sink.record_counter(
                "benchmark.messages.total",
                &[
                    ("scenario", self.config.scenario_name.as_str()),
                ],
            );
            // Emit measurement window duration
            sink.record_gauge(
                "benchmark.duration_ms",
                snapshot.throughput_meter.elapsed().as_millis() as f64,
                &[
                    ("scenario", self.config.scenario_name.as_str()),
                ],
            );
        }

        Ok(())
    }
}

impl fmt::Debug for BenchmarkContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BenchmarkContext")
            .field("config", &self.config.scenario_name)
            .finish()
    }
}

/// BenchmarkRunner orchestrates benchmark scenario execution.
///
/// ## Lifecycle (per RUN-01)
/// 1. **Setup**: Create BenchmarkContext, spawn background aggregator
/// 2. **Warmup**: Send warmup_messages (excluded from measurement by BackgroundAggregator)
/// 3. **Measurement window**: Record latency and throughput for remaining messages
/// 4. **Teardown**: Shutdown aggregator, drain samples, produce BenchmarkResult
///
/// ## Send + Sync (RUN-06)
/// BenchmarkRunner itself is Send + Sync so it can be used from Python async context
/// via PyO3. The Scenario is dyn Scenario + Send + Sync. All internal state is
/// thread-safe via Arc<...> wrappers.
pub struct BenchmarkRunner {
    /// Optional MetricsSink for emitting framework-level benchmark metrics.
    metrics_sink: Option<Arc<dyn MetricsSink>>,
}

impl BenchmarkRunner {
    /// Create a new BenchmarkRunner.
    ///
    /// metrics_sink: Optional sink for framework-level metrics (can be None for noop).
    pub fn new(metrics_sink: Option<Arc<dyn MetricsSink>>) -> Self {
        Self { metrics_sink }
    }

    /// Primary entry point — run a benchmark scenario and return the result (RUN-02).
    ///
    /// Takes ownership of the scenario (boxed to handle different sizes). The scenario
    /// is dyn Scenario + Send + Sync as required by trait bounds.
    ///
    /// Returns BenchmarkResult with latency percentiles, throughput, error rate, and
    /// memory delta. This method is async and will be called from Python via PyO3's
    /// Python::with_gil || Python::spawn.
    pub async fn run_scenario(
        &self,
        scenario: Box<dyn Scenario>,
    ) -> anyhow::Result<BenchmarkResult> {
        // Phase 1: Setup — create BenchmarkContext
        let context = BenchmarkContext::new(scenario.as_ref(), self.metrics_sink.clone());
        let config = context.config.clone();
        let warmup_messages = scenario.default_warmup_messages();

        // Phase 2: Warmup — send warmup_messages (excluded from measurement)
        self.run_warmup(scenario.as_ref(), &context, warmup_messages).await?;

        // Phase 3: Measurement window — record latency and throughput
        let start = Instant::now();
        self.run_measurement(scenario.as_ref(), &context).await?;
        let duration = start.elapsed();

        // Phase 4: Teardown — graceful drain then shutdown
        context.graceful_teardown().await?;
        let stats = context.snapshot();

        // Phase 5: Build BenchmarkResult
        let result = self.build_result(&config, duration, &stats);

        // Emit framework-level benchmark metrics to user-provided sink (RUN-03)
        if let Some(sink) = &self.metrics_sink {
            let labels: Vec<(&str, &str)> = vec![
                ("scenario", config.scenario_name.as_str()),
            ];

            // Emit throughput metrics
            sink.record_counter("benchmark.messages.total", &labels);

            sink.record_gauge(
                "benchmark.throughput_msg_s",
                result.throughput_msg_s,
                &labels,
            );

            // Emit latency percentiles
            sink.record_gauge("benchmark.latency.p50_ms", result.latency_p50_ms, &labels);
            sink.record_gauge("benchmark.latency.p95_ms", result.latency_p95_ms, &labels);
            sink.record_gauge("benchmark.latency.p99_ms", result.latency_p99_ms, &labels);

            // Emit error rate
            sink.record_gauge("benchmark.error_rate", result.error_rate, &labels);

            // Emit memory delta (will be 0 until Phase 41 connects RuntimeSnapshot)
            sink.record_gauge(
                "benchmark.memory_delta_bytes",
                result.memory_delta_bytes as f64,
                &labels,
            );
        }

        Ok(result)
    }

    async fn run_warmup(
        &self,
        scenario: &dyn Scenario,
        context: &BenchmarkContext,
        warmup_messages: usize,
    ) -> anyhow::Result<()> {
        // Warmup sends messages without recording latency/throughput.
        // The BackgroundAggregator drops samples until warmup_messages are seen.
        // For now, implement a simple warmup loop — will be connected to producer in later phases.
        // TODO: Connect to actual producer (Phase 41/42)
        let _ = scenario;
        let _ = context;
        let _ = warmup_messages;
        Ok(())
    }

    async fn run_measurement(
        &self,
        scenario: &dyn Scenario,
        context: &BenchmarkContext,
    ) -> anyhow::Result<()> {
        // Measurement window — record latency for each message.
        // The BackgroundAggregator collects stats off the hot path.
        // For now, implement a placeholder — will be connected to producer in later phases.
        // TODO: Connect to actual producer (Phase 41/42)
        let _ = scenario;
        let _ = context;
        Ok(())
    }

    fn build_result(
        &self,
        config: &ScenarioConfig,
        duration: std::time::Duration,
        stats: &AggregatedStatsSnapshot,
    ) -> BenchmarkResult {
        let duration_ms = duration.as_millis() as u64;

        // Compute throughput from ThroughputMeter
        let total_messages = stats.throughput_meter.messages();
        let throughput_msg_s = if duration_ms > 0 {
            (total_messages as f64 / duration_ms as f64) * 1000.0
        } else {
            0.0
        };

        // Get latency percentiles from histogram
        let p50 = stats.histogram.percentile(50.0).unwrap_or(0.0);
        let p95 = stats.histogram.percentile(95.0).unwrap_or(0.0);
        let p99 = stats.histogram.percentile(99.0).unwrap_or(0.0);

        BenchmarkResult {
            scenario_config: config.clone(),
            total_messages,
            duration_ms,
            throughput_msg_s,
            latency_p50_ms: p50,
            latency_p95_ms: p95,
            latency_p99_ms: p99,
            error_rate: 0.0, // TODO: connect error tracking (Phase 41/42)
            memory_delta_bytes: 0, // TODO: connect RuntimeSnapshot (Phase 40-02)
            percentile_buckets: crate::benchmark::results::PercentileBuckets::default(),
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64,
        }
    }
}

// SAFETY: BenchmarkRunner contains only Send + Sync types:
// - Option<Arc<dyn MetricsSink>> — Arc is Send+Sync, dyn MetricsSink is Send+Sync
// - No interior mutability that could break Send+Sync invariants
unsafe impl Send for BenchmarkRunner {}
unsafe impl Sync for BenchmarkRunner {}

// ─── PyO3 Bindings ────────────────────────────────────────────────────────────

/// PyO3 bindings for BenchmarkRunner.
/// Allows Python code to call run_scenario from async context.
#[cfg(feature = "pyo3")]
mod pyo3_bindings {
    use super::*;
    use pyo3::prelude::*;
    use pyo3::Py;

    #[pymethods]
    impl BenchmarkRunner {
        /// Create a new BenchmarkRunner.
        ///
        /// Args:
        ///     metrics_sink: Optional PyMetricsSink instance (None for noop)
        #[new]
        fn new(metrics_sink: Option<Py<pyo3::PyAny>>) -> Self {
            // metrics_sink is deserialized/cast by Python caller — store as None for now
            // Real implementation would use a PyObject-based sink wrapper
            let _ = metrics_sink;
            Self::new(None)
        }

        /// Run a benchmark scenario asynchronously.
        ///
        /// This method is `async` in Rust — Python calls it via `asyncio.to_thread`
        /// or similar to avoid blocking the event loop.
        ///
        /// Args:
        ///     scenario_name: String name of the scenario to run ("throughput", "latency", etc.)
        ///     config_json: JSON string with scenario configuration
        ///
        /// Returns:
        ///     BenchmarkResult as a Python dict
        pub async fn run_scenario_py(
            &self,
            py: pyo3::Python<'_>,
            scenario_name: String,
            config_json: String,
        ) -> Py<pyo3::PyAny> {
            // Deserialize scenario from JSON config
            let scenario: Box<dyn Scenario> = match scenario_name.as_str() {
                "throughput" => {
                    let cfg: crate::benchmark::scenarios::ThroughputScenario =
                        serde_json::from_str(&config_json).unwrap_or_default();
                    Box::new(cfg)
                }
                "latency" => {
                    let cfg: crate::benchmark::scenarios::LatencyScenario =
                        serde_json::from_str(&config_json).unwrap_or_default();
                    Box::new(cfg)
                }
                _ => {
                    // Fallback to throughput scenario for unknown names
                    let cfg = crate::benchmark::scenarios::ThroughputScenario::default();
                    Box::new(cfg)
                }
            };

            // Run the scenario
            let result = self.run_scenario(scenario).await;

            // Return result as Python dict
            match result {
                Ok(r) => {
                    let dict = pyo3::types::PyDict::new(py);
                    dict.set_item("scenario_name", r.scenario_config.scenario_name).ok();
                    dict.set_item("total_messages", r.total_messages).ok();
                    dict.set_item("duration_ms", r.duration_ms).ok();
                    dict.set_item("throughput_msg_s", r.throughput_msg_s).ok();
                    dict.set_item("latency_p50_ms", r.latency_p50_ms).ok();
                    dict.set_item("latency_p95_ms", r.latency_p95_ms).ok();
                    dict.set_item("latency_p99_ms", r.latency_p99_ms).ok();
                    dict.set_item("error_rate", r.error_rate).ok();
                    dict.set_item("memory_delta_bytes", r.memory_delta_bytes).ok();
                    dict.set_item("timestamp_ms", r.timestamp_ms).ok();
                    dict.into_py(py)
                }
                Err(e) => {
                    pyo3::exceptions::PyRuntimeError::new_err(format!("benchmark run failed: {}", e))
                        .into_py(py)
                }
            }
        }
    }
}
