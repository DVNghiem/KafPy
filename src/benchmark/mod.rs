// src/benchmark/mod.rs
// Benchmark infrastructure — pub(crate) so it's invisible to Python API

pub(crate) mod results;

// Benchmark infrastructure — measurement types, latency/throughput tracking
// (output.rs deleted — all items had zero internal references)

// Hardening validation checks (HARD-01 through HARD-08)
pub(crate) mod hardening;

pub mod measurement;


// Re-export MetricLabels from observability for benchmark label construction

// re-export result types and CsvSerializable trait for convenience

// re-export hardening validation types

// Scenario definitions — WHAT to benchmark (consumed by BenchmarkRunner in Phase 40)
pub(crate) mod scenarios;


// BenchmarkRunner — orchestrates scenario setup, warmup, measurement, teardown (Phase 40)
pub(crate) mod runner;

// Re-export BenchmarkRunner and BenchmarkContext for consumers
