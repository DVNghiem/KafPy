// src/benchmark/mod.rs
// Benchmark infrastructure — pub(crate) so it's invisible to Python API

pub(crate) mod results;

pub mod measurement;

pub use measurement::{
    BackgroundAggregator, HistogramRecorder, LatencyTimer, MeasurementStats,
    MemorySnapshot, ThroughputMeter, benchmark_labels,
    AggregatedStatsSnapshot, Sample,
};

// Re-export MetricLabels from observability for benchmark label construction
pub use crate::observability::MetricLabels;

// re-export result types andCsvSerializable trait for convenience
pub use results::{AggregatedResult, BenchmarkResult, CsvSerializable, PercentileBuckets, ScenarioConfig};