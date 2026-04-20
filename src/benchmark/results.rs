// src/benchmark/results.rs
// Result model data structures for benchmark results.
// These are pure data structures with serde serialization,
// consumed by later phases (39-42) for scenario definitions,
// benchmark runner, result output, and hardening checks.

use serde::{Deserialize, Serialize};

// ─── ScenarioConfig ────────────────────────────────────────────────────────────

/// Configuration parameters for a benchmark scenario (RES-04).
/// Echoed in result for reproducibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioConfig {
    pub scenario_name: String,
    pub num_messages: usize,
    pub payload_bytes: usize,
    pub rate: Option<usize>, // messages per second, None = unlimited
    pub warmup_messages: usize,
    pub failure_rate: f64, // 0.0 to 1.0
}

// ─── PercentileBuckets ──────────────────────────────────────────────────────────

/// Configurable percentile bucket definitions (RES-03).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PercentileBuckets {
    pub percentiles: Vec<f64>, // e.g., [50.0, 95.0, 99.0, 99.9]
}

impl Default for PercentileBuckets {
    fn default() -> Self {
        Self {
            percentiles: vec![50.0, 95.0, 99.0, 99.9],
        }
    }
}

// ─── BenchmarkResult ──────────────────────────────────────────────────────────

/// Per-run benchmark result (RES-01).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub scenario_config: ScenarioConfig,
    pub total_messages: u64,
    pub duration_ms: u64,
    pub throughput_msg_s: f64,
    pub latency_p50_ms: f64,
    pub latency_p95_ms: f64,
    pub latency_p99_ms: f64,
    pub error_rate: f64, // 0.0 to 1.0
    pub memory_delta_bytes: i64, // heap_allocated delta (can be negative)
    pub percentile_buckets: PercentileBuckets,
    /// Unix timestamp in milliseconds at end of benchmark run.
    pub timestamp_ms: i64,
}

// CsvSerializable, AggregatedResult, and escape_csv deleted — all had zero internal references.
