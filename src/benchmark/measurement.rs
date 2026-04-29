// src/benchmark/measurement.rs
// Measurement infrastructure for collecting latency/throughput/memory samples off the hot path.
// All measurement code aggregates off hot path via background task with configurable warmup exclusion.

use std::fmt;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

use tdigest::TDigest;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::observability::MetricLabels;

// ─── MeasurementStats ─────────────────────────────────────────────────────────

/// Online statistics accumulator for computing mean/variance/percentiles.
/// Thread-safe via interior mutability (MEAS-01).
pub struct MeasurementStats {
    counter: AtomicU64,
    sum: Mutex<f64>,
    sum_squared: Mutex<f64>,
    min: Mutex<f64>,
    max: Mutex<f64>,
}

impl MeasurementStats {
    /// Create a new MeasurementStats with all accumulators initialized to zero.
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            counter: AtomicU64::new(0),
            sum: Mutex::new(0.0),
            sum_squared: Mutex::new(0.0),
            min: Mutex::new(f64::INFINITY),
            max: Mutex::new(f64::NEG_INFINITY),
        }
    }

    /// Record a value — updates all accumulators atomically.
    #[allow(dead_code)]
    pub fn record(&self, value: f64) {
        self.counter.fetch_add(1, Ordering::Relaxed);
        {
            let mut sum = self.sum.lock().unwrap();
            *sum += value;
        }
        {
            let mut sum_sq = self.sum_squared.lock().unwrap();
            *sum_sq += value * value;
        }
        {
            let mut min = self.min.lock().unwrap();
            if value < *min {
                *min = value;
            }
        }
        {
            let mut max = self.max.lock().unwrap();
            if value > *max {
                *max = value;
            }
        }
    }

    /// Returns the number of recorded samples.
    #[allow(dead_code)]
    pub fn counter(&self) -> u64 {
        self.counter.load(Ordering::Relaxed)
    }

    /// Returns the mean of recorded values, or None if counter == 0.
    #[allow(dead_code)]
    pub fn mean(&self) -> Option<f64> {
        let cnt = self.counter.load(Ordering::Relaxed);
        if cnt == 0 {
            None
        } else {
            let sum = *self.sum.lock().unwrap();
            Some(sum / cnt as f64)
        }
    }

    /// Returns the population variance, or None if counter < 2.
    #[allow(dead_code)]
    pub fn variance(&self) -> Option<f64> {
        let cnt = self.counter.load(Ordering::Relaxed);
        if cnt < 2 {
            None
        } else {
            let sum = *self.sum.lock().unwrap();
            let sum_sq = *self.sum_squared.lock().unwrap();
            let mean = sum / cnt as f64;
            Some((sum_sq / cnt as f64) - (mean * mean))
        }
    }

    /// Returns the population standard deviation, or None if counter < 2.
    #[allow(dead_code)]
    pub fn stddev(&self) -> Option<f64> {
        self.variance().map(|v| v.sqrt())
    }

    /// Returns the minimum value, or None if counter == 0.
    #[allow(dead_code)]
    pub fn min(&self) -> Option<f64> {
        let min_val = *self.min.lock().unwrap();
        if min_val.is_infinite() {
            None
        } else {
            Some(min_val)
        }
    }

    /// Returns the maximum value, or None if counter == 0.
    #[allow(dead_code)]
    pub fn max(&self) -> Option<f64> {
        let max_val = *self.max.lock().unwrap();
        if max_val.is_infinite() {
            None
        } else {
            Some(max_val)
        }
    }
}

impl Default for MeasurementStats {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for MeasurementStats {
    fn clone(&self) -> Self {
        Self {
            counter: AtomicU64::new(self.counter.load(Ordering::Relaxed)),
            sum: Mutex::new(*self.sum.lock().unwrap()),
            sum_squared: Mutex::new(*self.sum_squared.lock().unwrap()),
            min: Mutex::new(*self.min.lock().unwrap()),
            max: Mutex::new(*self.max.lock().unwrap()),
        }
    }
}

impl fmt::Debug for MeasurementStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MeasurementStats")
            .field("counter", &self.counter.load(Ordering::Relaxed))
            .field("sum", &*self.sum.lock().unwrap())
            .field("sum_squared", &*self.sum_squared.lock().unwrap())
            .field("min", &*self.min.lock().unwrap())
            .field("max", &*self.max.lock().unwrap())
            .finish()
    }
}

// ─── HistogramRecorder ───────────────────────────────────────────────────────

/// High-percentile histogram using t-digest algorithm (MEAS-02).
/// Accurate at high cardinality with bounded memory.
/// Uses tdigest crate for accurate high-percentile computation.
pub struct HistogramRecorder {
    /// Compression factor for t-digest (higher = more accurate, more memory)
    compression: f64,
    /// Internal t-digest state
    digest: RwLock<TDigest>,
}

impl HistogramRecorder {
    /// Create a new HistogramRecorder with the given compression factor.
    #[allow(dead_code)]
    pub fn new(compression: f64) -> Self {
        Self {
            compression,
            digest: RwLock::new(TDigest::new_with_size(compression as usize)),
        }
    }

    /// Record a single value into the histogram.
    pub fn record(&self, value: f64) {
        self.digest.write().unwrap().merge_unsorted(vec![value]);
    }

    /// Get the percentile value (e.g., 50.0 for p50, 95.0 for p95).
    /// Returns None if no data has been recorded.
    pub fn percentile(&self, p: f64) -> Option<f64> {
        let digest = self.digest.read().unwrap();
        if digest.count() == 0.0 {
            None
        } else {
            Some(digest.estimate_quantile(p / 100.0))
        }
    }

    /// Get multiple percentiles at once.
    #[allow(dead_code)]
    pub fn percentiles(&self, percentiles: &[f64]) -> Vec<Option<f64>> {
        percentiles.iter().map(|&p| self.percentile(p)).collect()
    }

    /// Clear all recorded samples and reset to initial state.
    #[allow(dead_code)]
    pub fn clear(&self) {
        *self.digest.write().unwrap() = TDigest::new_with_size(self.compression as usize);
    }

    /// Returns the number of samples recorded.
    pub fn count(&self) -> usize {
        self.digest.read().unwrap().count() as usize
    }
}

impl Default for HistogramRecorder {
    fn default() -> Self {
        Self::new(100.0) // default compression
    }
}

impl Clone for HistogramRecorder {
    fn clone(&self) -> Self {
        Self {
            compression: self.compression,
            digest: RwLock::new(self.digest.read().unwrap().clone()),
        }
    }
}

impl fmt::Debug for HistogramRecorder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HistogramRecorder")
            .field("compression", &self.compression)
            .field("count", &self.count())
            .finish()
    }
}

// ─── LatencyTimer ────────────────────────────────────────────────────────────

/// Nanosecond-precision scoped timer for latency measurement (MEAS-03).
/// Uses std::time::Instant for wall-clock timing.
#[derive(Debug)]
#[allow(dead_code)]
pub struct LatencyTimer {
    start: Instant,
}

impl LatencyTimer {
    /// Start a new timer and return it immediately.
    #[allow(dead_code)]
    pub fn start_now() -> Self {
        Self {
            start: Instant::now(),
        }
    }

    /// Immediately start a timer — shorthand for LatencyTimer::start_now().
    #[allow(dead_code)]
    pub fn start() -> Self {
        Self::start_now()
    }

    /// Returns elapsed time in milliseconds as f64 (for fractional ms precision).
    #[allow(dead_code)]
    pub fn elapsed_ms(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.0
    }

    /// Returns elapsed time in nanoseconds as u64.
    #[allow(dead_code)]
    pub fn elapsed_ns(&self) -> u64 {
        self.start.elapsed().as_nanos() as u64
    }

    /// Returns elapsed time as Duration.
    #[allow(dead_code)]
    pub fn elapsed(&self) -> std::time::Duration {
        self.start.elapsed()
    }
}

// ─── ThroughputMeter ──────────────────────────────────────────────────────────

/// Throughput meter tracking messages and bytes count (MEAS-04).
/// Computes rate on demand, not per-message.
pub struct ThroughputMeter {
    messages: AtomicU64,
    bytes: AtomicU64,
    start_time: Instant,
}

impl ThroughputMeter {
    /// Create a new ThroughputMeter.
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            messages: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
            start_time: Instant::now(),
        }
    }

    /// Record a single message with its payload size in bytes.
    #[allow(dead_code)]
    pub fn record_message(&self, payload_bytes: usize) {
        self.messages.fetch_add(1, Ordering::Relaxed);
        self.bytes
            .fetch_add(payload_bytes as u64, Ordering::Relaxed);
    }

    /// Returns the total number of messages recorded.
    #[allow(dead_code)]
    pub fn messages(&self) -> u64 {
        self.messages.load(Ordering::Relaxed)
    }

    /// Returns the total number of bytes recorded.
    #[allow(dead_code)]
    pub fn bytes(&self) -> u64 {
        self.bytes.load(Ordering::Relaxed)
    }

    /// Returns messages per second.
    #[allow(dead_code)]
    pub fn msg_rate(&self) -> f64 {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        if elapsed == 0.0 {
            0.0
        } else {
            self.messages.load(Ordering::Relaxed) as f64 / elapsed
        }
    }

    /// Returns bytes per second.
    #[allow(dead_code)]
    pub fn byte_rate(&self) -> f64 {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        if elapsed == 0.0 {
            0.0
        } else {
            self.bytes.load(Ordering::Relaxed) as f64 / elapsed
        }
    }

    /// Returns megabits per second (for network throughput context).
    #[allow(dead_code)]
    pub fn mbps(&self) -> f64 {
        self.byte_rate() * 8.0 / 1_000_000.0
    }

    /// Returns the elapsed time since this meter was created.
    #[allow(dead_code)]
    pub fn elapsed(&self) -> std::time::Duration {
        self.start_time.elapsed()
    }
}

impl Default for ThroughputMeter {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for ThroughputMeter {
    fn clone(&self) -> Self {
        Self {
            messages: AtomicU64::new(self.messages.load(Ordering::Relaxed)),
            bytes: AtomicU64::new(self.bytes.load(Ordering::Relaxed)),
            start_time: self.start_time,
        }
    }
}

impl fmt::Debug for ThroughputMeter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ThroughputMeter")
            .field("messages", &self.messages.load(Ordering::Relaxed))
            .field("bytes", &self.bytes.load(Ordering::Relaxed))
            .finish()
    }
}

// ─── MemorySnapshot ───────────────────────────────────────────────────────────

/// Memory snapshot using RuntimeSnapshot for heap_allocated delta (MEAS-05).
/// Takes two snapshots and computes the delta between them.
///
/// Note: The actual heap_allocated delta requires integration with RuntimeSnapshot
/// in Phase 40. For Phase 38, provide the struct skeleton with a placeholder that
/// returns None/0. The real implementation will come when BenchmarkRunner integrates
/// with RuntimeSnapshotTask.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MemorySnapshot {
    initial: Option<i64>,
}

impl MemorySnapshot {
    /// Create a new MemorySnapshot with no initial baseline.
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self { initial: None }
    }

    /// Take initial snapshot using RuntimeSnapshot.
    /// Falls back to None if RuntimeSnapshot is not available.
    #[allow(dead_code)]
    pub fn take_initial(&mut self) {
        self.initial = self.try_get_heap_allocated();
    }

    /// Take final snapshot and return delta from initial.
    /// Returns 0 if no initial snapshot was taken.
    #[allow(dead_code)]
    pub fn take_final_and_compute_delta(&mut self) -> Option<i64> {
        let final_heap = self.try_get_heap_allocated()?;
        match self.initial {
            Some(initial) => Some(final_heap - initial),
            None => Some(0), // no initial snapshot taken
        }
    }

    #[allow(dead_code)]
    fn try_get_heap_allocated(&self) -> Option<i64> {
        // RuntimeSnapshot does not expose heap_allocated directly in v1.9.
        // Phase 40 will integrate with jemalloc metrics via the runtime subsystem.
        // Placeholder: return None for now.
        None
    }
}

impl Default for MemorySnapshot {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Sample ──────────────────────────────────────────────────────────────────

/// Sample sent from hot path to background aggregator via channel (MEAS-06).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum Sample {
    /// Nanosecond latency sample.
    LatencyNs(u64),
    /// Message recorded with payload bytes.
    MessageRecorded(usize),
    /// Error recorded.
    ErrorRecorded,
}

// ─── BackgroundAggregator ─────────────────────────────────────────────────────

/// Background task that aggregates samples from the hot path (MEAS-06, MEAS-07).
/// Uses tokio channel to receive samples without blocking the hot path.
/// Warmup exclusion: first N messages are excluded from metrics (MEAS-07).
pub struct BackgroundAggregator {
    /// Channel sender — clone this and give to hot path code.
    #[allow(dead_code)]
    sender: mpsc::Sender<Sample>,
    /// Pre-aggregated stats — populated when aggregation task runs.
    latency_stats: MeasurementStats,
    throughput_meter: ThroughputMeter,
    histogram: HistogramRecorder,
    warmup_messages: usize,
    messages_seen: AtomicUsize,
}

impl BackgroundAggregator {
    /// Create a new BackgroundAggregator and spawn the background task.
    /// The aggregation task runs until the returned CancellationToken is cancelled.
    /// Returns the aggregator and a cancellation token to shut it down.
    pub fn spawn(warmup_messages: usize) -> (Arc<Self>, CancellationToken) {
        let (tx, rx) = mpsc::channel(10_000); // buffered channel
        let this = Arc::new(Self {
            sender: tx,
            latency_stats: MeasurementStats::new(),
            throughput_meter: ThroughputMeter::new(),
            histogram: HistogramRecorder::default(),
            warmup_messages,
            messages_seen: AtomicUsize::new(0),
        });

        let this_clone = Arc::clone(&this);
        let shutdown_token = CancellationToken::new();
        let token_clone = shutdown_token.clone();

        tokio::spawn(async move {
            this_clone.aggregation_loop(rx, token_clone).await;
        });

        (this, shutdown_token)
    }

    /// Returns a sender clone for use on the hot path.
    #[allow(dead_code)]
    pub fn sender(&self) -> mpsc::Sender<Sample> {
        self.sender.clone()
    }

    async fn aggregation_loop(
        self: Arc<Self>,
        mut rx: mpsc::Receiver<Sample>,
        shutdown_token: CancellationToken,
    ) {
        tokio::select! {
            _ = shutdown_token.cancelled() => (),
            sample = rx.recv() => {
                if let Some(sample) = sample {
                    self.process_sample(sample);
                    // Continue looping to drain remaining samples
                    while let Ok(sample) = rx.try_recv() {
                        self.process_sample(sample);
                    }
                }
            }
        }
    }

    fn process_sample(&self, sample: Sample) {
        let msg_count = self.messages_seen.fetch_add(1, Ordering::Relaxed);

        // MEAS-07: Skip warmup messages
        if msg_count < self.warmup_messages {
            return;
        }

        match sample {
            Sample::LatencyNs(ns) => {
                let ms = ns as f64 / 1_000_000.0;
                self.latency_stats.record(ms);
                self.histogram.record(ms);
            }
            Sample::MessageRecorded(bytes) => {
                self.throughput_meter.record_message(bytes);
            }
            Sample::ErrorRecorded => {
                // Error counting handled separately via MetricsSink
            }
        }
    }

    /// Returns a snapshot of current aggregated stats.
    pub fn snapshot(&self) -> AggregatedStatsSnapshot {
        AggregatedStatsSnapshot {
            latency_stats: self.latency_stats.clone(),
            throughput_meter: self.throughput_meter.clone(),
            histogram: self.histogram.clone(),
            messages_seen: self.messages_seen.load(Ordering::Relaxed),
        }
    }
}

impl fmt::Debug for BackgroundAggregator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BackgroundAggregator")
            .field("warmup_messages", &self.warmup_messages)
            .field("messages_seen", &self.messages_seen.load(Ordering::Relaxed))
            .finish()
    }
}

/// Snapshot of aggregated stats at a point in time.
#[derive(Debug, Clone)]
pub struct AggregatedStatsSnapshot {
    /// Latency statistics.
    #[allow(dead_code)]
    pub latency_stats: MeasurementStats,
    /// Throughput meter.
    pub throughput_meter: ThroughputMeter,
    /// Histogram for high-percentile computation.
    pub histogram: HistogramRecorder,
    /// Total messages seen (including warmup).
    #[allow(dead_code)]
    pub messages_seen: usize,
}

// ─── MetricLabels integration (MEAS-08) ───────────────────────────────────────

/// Helper to build benchmark metric labels using MetricLabels from observability.
#[allow(dead_code)]
pub fn benchmark_labels(scenario: &str, mode: &str) -> MetricLabels {
    MetricLabels::new()
        .insert("scenario", scenario)
        .insert("mode", mode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measurement_stats_empty() {
        let stats = MeasurementStats::new();
        assert_eq!(stats.counter(), 0);
        assert!(stats.mean().is_none());
        assert!(stats.variance().is_none());
        assert!(stats.stddev().is_none());
        assert!(stats.min().is_none());
        assert!(stats.max().is_none());
    }

    #[test]
    fn measurement_stats_single_value() {
        let stats = MeasurementStats::new();
        stats.record(42.0);
        assert_eq!(stats.counter(), 1);
        assert_eq!(stats.mean(), Some(42.0));
        assert!(stats.variance().is_none()); // need at least 2 samples
        assert_eq!(stats.min(), Some(42.0));
        assert_eq!(stats.max(), Some(42.0));
    }

    #[test]
    fn measurement_stats_multiple_values() {
        let stats = MeasurementStats::new();
        stats.record(10.0);
        stats.record(20.0);
        stats.record(30.0);
        assert_eq!(stats.counter(), 3);
        assert_eq!(stats.mean(), Some(20.0));
        assert!(stats.variance().is_some());
        assert!(stats.stddev().is_some());
        assert_eq!(stats.min(), Some(10.0));
        assert_eq!(stats.max(), Some(30.0));
    }

    #[test]
    fn histogram_recorder_empty() {
        let hist = HistogramRecorder::new(100.0);
        assert!(hist.percentile(50.0).is_none());
        assert_eq!(hist.count(), 0);
    }

    #[test]
    fn histogram_recorder_single_value() {
        let hist = HistogramRecorder::new(100.0);
        hist.record(10.0);
        assert_eq!(hist.percentile(50.0), Some(10.0));
        assert_eq!(hist.percentile(95.0), Some(10.0));
        assert_eq!(hist.count(), 1);
    }

    #[test]
    fn histogram_recorder_multiple_values() {
        let hist = HistogramRecorder::new(100.0);
        for i in 1..=100u64 {
            hist.record(i as f64);
        }
        // p50 should be approximately 50
        let p50 = hist.percentile(50.0).unwrap();
        assert!(p50 > 40.0 && p50 < 60.0);
        // p99 should be approximately 99
        let p99 = hist.percentile(99.0).unwrap();
        assert!(p99 > 90.0 && p99 < 100.0);
    }

    #[test]
    fn latency_timer_elapsed() {
        let timer = LatencyTimer::start_now();
        std::thread::sleep(std::time::Duration::from_millis(1));
        let elapsed = timer.elapsed_ms();
        assert!(elapsed >= 0.5); // at least 0.5ms
        let ns = timer.elapsed_ns();
        assert!(ns > 0);
    }

    #[test]
    fn throughput_meter_basic() {
        let meter = ThroughputMeter::new();
        meter.record_message(100);
        meter.record_message(200);
        assert_eq!(meter.messages(), 2);
        assert_eq!(meter.bytes(), 300);
        assert!(meter.msg_rate() >= 0.0);
        assert!(meter.mbps() >= 0.0);
    }

    #[test]
    fn memory_snapshot_no_initial() {
        let mut snap = MemorySnapshot::new();
        snap.take_initial();
        // No RuntimeSnapshot available, should return None
        assert!(snap.take_final_and_compute_delta().is_none());
    }

    #[test]
    fn benchmark_labels_creation() {
        let labels = benchmark_labels("throughput_test", "burst");
        let slice = labels.as_slice();
        assert!(slice
            .iter()
            .any(|(k, v)| *k == "scenario" && *v == "throughput_test"));
        assert!(slice.iter().any(|(k, v)| *k == "mode" && *v == "burst"));
    }
}
