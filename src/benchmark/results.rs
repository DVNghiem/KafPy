// src/benchmark/results.rs
// Result model data structures for benchmark results.
// These are pure data structures with serde serialization,
// consumed by later phases (39-42) for scenario definitions,
// benchmark runner, result output, and hardening checks.

use serde::{Deserialize, Serialize};

// ─── CsvSerializable Trait ─────────────────────────────────────────────────────

/// Trait for types that can be serialized to CSV format.
///
/// Implementors produce a header row and a data row following RFC 4180.
pub trait CsvSerializable {
    /// Returns the comma-separated column headers for CSV output.
    fn to_csv_header() -> String
    where
        Self: Sized;

    /// Returns the comma-separated values for this instance as a CSV row.
    /// Values are escaped according to RFC 4180 (double-quote wrapping for
    /// fields containing commas, double-quotes, or newlines).
    fn to_csv_row(&self) -> String;
}

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

// ─── AggregatedResult ──────────────────────────────────────────────────────────

/// Aggregated statistics across multiple benchmark runs (RES-02).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregatedResult {
    pub scenario_name: String,
    pub metric_name: String, // e.g., "throughput_msg_s", "latency_p99_ms"
    pub mean: f64,
    pub stddev: f64,
    pub min: f64,
    pub max: f64,
    pub runs: usize,
}

// ─── CsvSerializable Implementations ──────────────────────────────────────────

impl CsvSerializable for BenchmarkResult {
    fn to_csv_header() -> String
    where
        Self: Sized,
    {
        "scenario,total_messages,duration_ms,throughput_msg_s,latency_p50_ms,latency_p95_ms,latency_p99_ms,error_rate,memory_delta_bytes,warmup_messages,payload_bytes,timestamp_ms".to_string()
    }

    fn to_csv_row(&self) -> String {
        format!(
            "{},{},{},{},{},{},{},{},{},{},{},{}",
            escape_csv(&self.scenario_config.scenario_name),
            self.total_messages,
            self.duration_ms,
            self.throughput_msg_s,
            self.latency_p50_ms,
            self.latency_p95_ms,
            self.latency_p99_ms,
            self.error_rate,
            self.memory_delta_bytes,
            self.scenario_config.warmup_messages,
            self.scenario_config.payload_bytes,
            self.timestamp_ms,
        )
    }
}

impl CsvSerializable for AggregatedResult {
    fn to_csv_header() -> String
    where
        Self: Sized,
    {
        "scenario_name,metric_name,mean,stddev,min,max,runs".to_string()
    }

    fn to_csv_row(&self) -> String {
        format!(
            "{},{},{},{},{},{},{}",
            escape_csv(&self.scenario_name),
            escape_csv(&self.metric_name),
            self.mean,
            self.stddev,
            self.min,
            self.max,
            self.runs,
        )
    }
}

// ─── Helper Functions ───────────────────────────────────────────────────────────

/// Escapes a string value for CSV output per RFC 4180.
///
/// If the string contains a comma, double-quote, or newline, it is wrapped
/// in double-quotes with internal double-quotes doubled.
fn escape_csv(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_csv_plain() {
        assert_eq!(escape_csv("hello"), "hello");
    }

    #[test]
    fn test_escape_csv_with_comma() {
        assert_eq!(escape_csv("hello,world"), "\"hello,world\"");
    }

    #[test]
    fn test_escape_csv_with_quote() {
        assert_eq!(escape_csv("say \"hello\""), "\"say \"\"hello\"\"\"");
    }

    #[test]
    fn test_escape_csv_with_newline() {
        assert_eq!(escape_csv("line1\nline2"), "\"line1\nline2\"");
    }

    #[test]
    fn test_benchmark_result_csv_roundtrip() {
        let result = BenchmarkResult {
            scenario_config: ScenarioConfig {
                scenario_name: "throughput-test".to_string(),
                num_messages: 10000,
                payload_bytes: 256,
                rate: None,
                warmup_messages: 1000,
                failure_rate: 0.0,
            },
            total_messages: 10000,
            duration_ms: 1500,
            throughput_msg_s: 6666.67,
            latency_p50_ms: 0.5,
            latency_p95_ms: 1.2,
            latency_p99_ms: 2.0,
            error_rate: 0.0,
            memory_delta_bytes: 1024,
            percentile_buckets: PercentileBuckets::default(),
            timestamp_ms: 1713000000000,
        };

        let header = BenchmarkResult::to_csv_header();
        let row = result.to_csv_row();

        assert!(header.contains("scenario"));
        assert!(row.contains("throughput-test"));
    }

    #[test]
    fn test_aggregated_result_csv_roundtrip() {
        let result = AggregatedResult {
            scenario_name: "latency-test".to_string(),
            metric_name: "latency_p99_ms".to_string(),
            mean: 1.5,
            stddev: 0.3,
            min: 1.0,
            max: 2.5,
            runs: 10,
        };

        let header = AggregatedResult::to_csv_header();
        let row = result.to_csv_row();

        assert!(header.contains("scenario_name"));
        assert!(row.contains("latency-test"));
    }
}
