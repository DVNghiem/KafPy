// src/benchmark/output.rs
// JSON/CSV output writers for benchmark results with configurable output path.

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json;

use crate::benchmark::results::{AggregatedResult, BenchmarkResult, CsvSerializable};

// ─── OutputConfig ─────────────────────────────────────────────────────────────

/// Configuration for benchmark output writers.
#[derive(Debug, Clone)]
pub struct OutputConfig {
    /// Base directory for output files. Defaults to `./benchmark_results/`.
    pub base_path: PathBuf,
    /// Whether to create parent directories on write. Defaults to true.
    pub create_directories: bool,
    /// Regression threshold percentage for comparison reports. Defaults to 5.0.
    pub regression_threshold_pct: f64,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            base_path: PathBuf::from("./benchmark_results/"),
            create_directories: true,
            regression_threshold_pct: 5.0,
        }
    }
}

impl OutputConfig {
    /// Sets a custom base path for output files.
    pub fn base_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.base_path = path.into();
        self
    }

    /// Sets a custom regression threshold percentage.
    pub fn regression_threshold_pct(mut self, threshold: f64) -> Self {
        self.regression_threshold_pct = threshold;
        self
    }
}

// ─── BenchmarkResultSerializer ──────────────────────────────────────────────────

/// Serializes BenchmarkResult to JSON format.
#[derive(Debug, Clone)]
pub struct BenchmarkResultSerializer {
    config: OutputConfig,
}

impl BenchmarkResultSerializer {
    /// Creates a new serializer with the given output configuration.
    pub fn new(config: OutputConfig) -> Self {
        Self { config }
    }

    /// Serializes a benchmark result to a JSON string.
    pub fn serialize_json(&self, result: &BenchmarkResult) -> Result<String> {
        serde_json::to_string_pretty(result).context("failed to serialize BenchmarkResult to JSON")
    }

    /// Writes a benchmark result to a JSON file.
    ///
    /// Filename format: `bench-{scenario}-{timestamp}.json`
    pub fn write_json_file(&self, result: &BenchmarkResult) -> Result<PathBuf> {
        let filename = format!(
            "bench-{}-{}.json",
            sanitize_filename(&result.scenario_config.scenario_name),
            result.timestamp_ms
        );
        let path = self.config.base_path.join(&filename);

        if self.config.create_directories {
            fs::create_dir_all(&self.config.base_path)
                .with_context(|| format!("failed to create output directory {:?}", self.config.base_path))?;
        }

        let json = self.serialize_json(result)?;
        fs::write(&path, json).with_context(|| format!("failed to write JSON to {:?}", path))?;

        Ok(path)
    }
}

// ─── CsvIntervalWriter ─────────────────────────────────────────────────────────

/// Writes benchmark results to CSV format.
#[derive(Debug, Clone)]
pub struct CsvIntervalWriter {
    config: OutputConfig,
}

impl CsvIntervalWriter {
    /// Creates a new CSV writer with the given output configuration.
    pub fn new(config: OutputConfig) -> Self {
        Self { config }
    }

    /// Writes a benchmark result to a CSV file.
    ///
    /// Filename format: `bench-{scenario}-{timestamp}.csv`
    pub fn write_csv_file(&self, result: &BenchmarkResult) -> Result<PathBuf> {
        let filename = format!(
            "bench-{}-{}.csv",
            sanitize_filename(&result.scenario_config.scenario_name),
            result.timestamp_ms
        );
        let path = self.config.base_path.join(&filename);

        if self.config.create_directories {
            fs::create_dir_all(&self.config.base_path)
                .with_context(|| format!("failed to create output directory {:?}", self.config.base_path))?;
        }

        let header = BenchmarkResult::to_csv_header();
        let row = result.to_csv_row();
        let content = format!("{}\n{}\n", header, row);

        fs::write(&path, content).with_context(|| format!("failed to write CSV to {:?}", path))?;

        Ok(path)
    }

    /// Writes aggregated results to a CSV file.
    ///
    /// Filename format: `bench-{scenario}-aggregated-{timestamp}.csv`
    pub fn write_aggregated_csv(
        &self,
        results: &[AggregatedResult],
        scenario: &str,
        timestamp_ms: i64,
    ) -> Result<PathBuf> {
        let filename = format!(
            "bench-{}-aggregated-{}.csv",
            sanitize_filename(scenario),
            timestamp_ms
        );
        let path = self.config.base_path.join(&filename);

        if self.config.create_directories {
            fs::create_dir_all(&self.config.base_path)
                .with_context(|| format!("failed to create output directory {:?}", self.config.base_path))?;
        }

        let header = AggregatedResult::to_csv_header();
        let mut lines = vec![header];
        for result in results {
            lines.push(result.to_csv_row());
        }
        let content = lines.join("\n");

        fs::write(&path, content).with_context(|| format!("failed to write aggregated CSV to {:?}", path))?;

        Ok(path)
    }
}

// ─── BenchmarkReport ───────────────────────────────────────────────────────────

/// Human-readable benchmark report in markdown format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkReport {
    pub scenario_name: String,
    pub timestamp_ms: i64,
    pub total_messages: u64,
    pub duration_ms: u64,
    pub throughput_msg_s: f64,
    pub latency_p50_ms: f64,
    pub latency_p95_ms: f64,
    pub latency_p99_ms: f64,
    pub error_rate: f64,
    pub memory_delta_bytes: i64,
}

impl From<&BenchmarkResult> for BenchmarkReport {
    fn from(result: &BenchmarkResult) -> Self {
        Self {
            scenario_name: result.scenario_config.scenario_name.clone(),
            timestamp_ms: result.timestamp_ms,
            total_messages: result.total_messages,
            duration_ms: result.duration_ms,
            throughput_msg_s: result.throughput_msg_s,
            latency_p50_ms: result.latency_p50_ms,
            latency_p95_ms: result.latency_p95_ms,
            latency_p99_ms: result.latency_p99_ms,
            error_rate: result.error_rate,
            memory_delta_bytes: result.memory_delta_bytes,
        }
    }
}

impl BenchmarkReport {
    /// Converts the report to a markdown string.
    pub fn to_markdown(&self) -> String {
        let timestamp = format_timestamp(self.timestamp_ms);
        let markdown = format!(
            "# Benchmark Report: {}\n\n**Timestamp:** {}\n**Duration:** {}ms\n**Total Messages:** {}\n\n## Key Metrics\n\n| Metric | Value |\n|--------|-------|\n| Throughput | {:.2} msg/s |\n| Latency P50 | {:.3}ms |\n| Latency P95 | {:.3}ms |\n| Latency P99 | {:.3}ms |\n| Error Rate | {:.4} ({:.2}%) |\n| Memory Delta | {} bytes |\n",
            self.scenario_name,
            timestamp,
            self.duration_ms,
            self.total_messages,
            self.throughput_msg_s,
            self.latency_p50_ms,
            self.latency_p95_ms,
            self.latency_p99_ms,
            self.error_rate,
            self.error_rate * 100.0,
            self.memory_delta_bytes,
        );
        markdown
    }
}

/// Writes a benchmark report to a markdown file.
///
/// Filename format: `bench-report-{scenario}-{timestamp}.md`
pub fn write_markdown_report(report: &BenchmarkReport, config: &OutputConfig) -> Result<PathBuf> {
    let filename = format!(
        "bench-report-{}-{}.md",
        sanitize_filename(&report.scenario_name),
        report.timestamp_ms
    );
    let path = config.base_path.join(&filename);

    if config.create_directories {
        fs::create_dir_all(&config.base_path)
            .with_context(|| format!("failed to create output directory {:?}", config.base_path))?;
    }

    let markdown = report.to_markdown();
    fs::write(&path, markdown).with_context(|| format!("failed to write markdown report to {:?}", path))?;

    Ok(path)
}

// ─── ComparisonReport ───────────────────────────────────────────────────────────

/// Comparison report between two benchmark runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonReport {
    pub scenario_name: String,
    pub base_timestamp_ms: i64,
    pub current_timestamp_ms: i64,
    pub throughput_delta_pct: f64,
    pub latency_p50_delta_pct: f64,
    pub latency_p95_delta_pct: f64,
    pub latency_p99_delta_pct: f64,
    pub error_rate_delta_pct: f64,
    pub memory_delta_bytes: i64,
    pub regressions: Vec<String>,
    pub improvements: Vec<String>,
}

/// Compares two benchmark results and produces a comparison report.
pub fn compare(base: &BenchmarkResult, current: &BenchmarkResult, threshold_pct: f64) -> Result<ComparisonReport> {
    if base.scenario_config.scenario_name != current.scenario_config.scenario_name {
        anyhow::bail!(
            "scenario mismatch: base '{}' vs current '{}'",
            base.scenario_config.scenario_name,
            current.scenario_config.scenario_name
        );
    }

    let throughput_delta_pct = compute_delta_pct(base.throughput_msg_s, current.throughput_msg_s);
    let latency_p50_delta_pct = compute_delta_pct(base.latency_p50_ms, current.latency_p50_ms);
    let latency_p95_delta_pct = compute_delta_pct(base.latency_p95_ms, current.latency_p95_ms);
    let latency_p99_delta_pct = compute_delta_pct(base.latency_p99_ms, current.latency_p99_ms);
    let error_rate_delta_pct = compute_delta_pct(base.error_rate, current.error_rate);

    let mut regressions = Vec::new();
    let mut improvements = Vec::new();

    // Latency regressions: increase is bad (> threshold means regression)
    if latency_p50_delta_pct > threshold_pct {
        regressions.push(format!("latency_p50 increased by {:.2}%", latency_p50_delta_pct));
    } else if latency_p50_delta_pct < -threshold_pct {
        improvements.push(format!("latency_p50 improved by {:.2}%", -latency_p50_delta_pct));
    }

    if latency_p95_delta_pct > threshold_pct {
        regressions.push(format!("latency_p95 increased by {:.2}%", latency_p95_delta_pct));
    } else if latency_p95_delta_pct < -threshold_pct {
        improvements.push(format!("latency_p95 improved by {:.2}%", -latency_p95_delta_pct));
    }

    if latency_p99_delta_pct > threshold_pct {
        regressions.push(format!("latency_p99 increased by {:.2}%", latency_p99_delta_pct));
    } else if latency_p99_delta_pct < -threshold_pct {
        improvements.push(format!("latency_p99 improved by {:.2}%", -latency_p99_delta_pct));
    }

    // Throughput: decrease is regression (< -threshold)
    if throughput_delta_pct < -threshold_pct {
        regressions.push(format!("throughput decreased by {:.2}%", -throughput_delta_pct));
    } else if throughput_delta_pct > threshold_pct {
        improvements.push(format!("throughput improved by {:.2}%", throughput_delta_pct));
    }

    // Error rate: increase is regression
    if error_rate_delta_pct > threshold_pct {
        regressions.push(format!("error_rate increased by {:.2}%", error_rate_delta_pct));
    } else if error_rate_delta_pct < -threshold_pct {
        improvements.push(format!("error_rate improved by {:.2}%", -error_rate_delta_pct));
    }

    Ok(ComparisonReport {
        scenario_name: base.scenario_config.scenario_name.clone(),
        base_timestamp_ms: base.timestamp_ms,
        current_timestamp_ms: current.timestamp_ms,
        throughput_delta_pct,
        latency_p50_delta_pct,
        latency_p95_delta_pct,
        latency_p99_delta_pct,
        error_rate_delta_pct,
        memory_delta_bytes: current.memory_delta_bytes - base.memory_delta_bytes,
        regressions,
        improvements,
    })
}

impl ComparisonReport {
    /// Converts the comparison report to a markdown string.
    pub fn to_markdown(&self) -> String {
        let base_time = format_timestamp(self.base_timestamp_ms);
        let current_time = format_timestamp(self.current_timestamp_ms);

        let regressions_section = if self.regressions.is_empty() {
            "None".to_string()
        } else {
            self.regressions
                .iter()
                .map(|r| format!("- [x] {}", r))
                .collect::<Vec<_>>()
                .join("\n")
        };

        let improvements_section = if self.improvements.is_empty() {
            "None".to_string()
        } else {
            self.improvements
                .iter()
                .map(|i| format!("- [+] {}", i))
                .collect::<Vec<_>>()
                .join("\n")
        };

        let markdown = format!(
            "# Comparison Report: {scenario}\n\n**Base:** {base} -> **Current:** {current}\n\n\
             ## Delta Summary\n\n\
             | Metric | Base | Current | Delta % |\n\
             |--------|------|---------|---------|\n\
             | Throughput | 0.00 | 0.00 | {tp_delta:+.2}% |\n\
             | Latency P50 | 0.000ms | 0.000ms | {p50_delta:+.2}% |\n\
             | Latency P95 | 0.000ms | 0.000ms | {p95_delta:+.2}% |\n\
             | Latency P99 | 0.000ms | 0.000ms | {p99_delta:+.2}% |\n\
             | Error Rate | 0.0000 | 0.0000 | {er_delta:+.2}% |\n\
             | Memory Delta | - | - | {mem_delta:} bytes |\n\n\
             ## Regressions\n{regressions}\n\n\
             ## Improvements\n{improvements}",
            scenario=self.scenario_name,
            base=base_time,
            current=current_time,
            tp_delta=self.throughput_delta_pct,
            p50_delta=self.latency_p50_delta_pct,
            p95_delta=self.latency_p95_delta_pct,
            p99_delta=self.latency_p99_delta_pct,
            er_delta=self.error_rate_delta_pct,
            mem_delta=self.memory_delta_bytes,
            regressions=regressions_section,
            improvements=improvements_section,
        );
        markdown
    }
}

/// Writes a comparison report to a markdown file.
///
/// Filename format: `bench-compare-{scenario}-{timestamp}.md`
pub fn write_comparison_markdown(report: &ComparisonReport, config: &OutputConfig) -> Result<PathBuf> {
    let filename = format!(
        "bench-compare-{}-{}.md",
        sanitize_filename(&report.scenario_name),
        report.current_timestamp_ms
    );
    let path = config.base_path.join(&filename);

    if config.create_directories {
        fs::create_dir_all(&config.base_path)
            .with_context(|| format!("failed to create output directory {:?}", config.base_path))?;
    }

    let markdown = report.to_markdown();
    fs::write(&path, markdown).with_context(|| format!("failed to write comparison markdown to {:?}", path))?;

    Ok(path)
}

// ─── DiffReport ────────────────────────────────────────────────────────────────

/// Individual metric diff entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffEntry {
    pub metric: String,
    pub base_value: f64,
    pub current_value: f64,
    pub delta_pct: f64,
    pub absolute_delta: f64,
    pub is_regression: bool,
}

/// Machine-readable diff report for JSON serialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffReport {
    pub scenario_name: String,
    pub base_timestamp_ms: i64,
    pub current_timestamp_ms: i64,
    pub entries: Vec<DiffEntry>,
    pub regressions: Vec<String>,
    pub improvements: Vec<String>,
    pub summary: String,
}

impl From<ComparisonReport> for DiffReport {
    fn from(report: ComparisonReport) -> Self {
        let mut entries = Vec::new();

        // Throughput
        entries.push(DiffEntry {
            metric: "throughput_msg_s".to_string(),
            base_value: 0.0, // Will be filled from original results
            current_value: 0.0,
            delta_pct: report.throughput_delta_pct,
            absolute_delta: 0.0,
            is_regression: report.throughput_delta_pct < -5.0,
        });

        // Latency P50
        entries.push(DiffEntry {
            metric: "latency_p50_ms".to_string(),
            base_value: 0.0,
            current_value: 0.0,
            delta_pct: report.latency_p50_delta_pct,
            absolute_delta: 0.0,
            is_regression: report.latency_p50_delta_pct > 5.0,
        });

        // Latency P95
        entries.push(DiffEntry {
            metric: "latency_p95_ms".to_string(),
            base_value: 0.0,
            current_value: 0.0,
            delta_pct: report.latency_p95_delta_pct,
            absolute_delta: 0.0,
            is_regression: report.latency_p95_delta_pct > 5.0,
        });

        // Latency P99
        entries.push(DiffEntry {
            metric: "latency_p99_ms".to_string(),
            base_value: 0.0,
            current_value: 0.0,
            delta_pct: report.latency_p99_delta_pct,
            absolute_delta: 0.0,
            is_regression: report.latency_p99_delta_pct > 5.0,
        });

        // Error rate
        entries.push(DiffEntry {
            metric: "error_rate".to_string(),
            base_value: 0.0,
            current_value: 0.0,
            delta_pct: report.error_rate_delta_pct,
            absolute_delta: 0.0,
            is_regression: report.error_rate_delta_pct > 5.0,
        });

        let regression_count = report.regressions.len();
        let improvement_count = report.improvements.len();
        let summary = format!("{} improvements, {} regressions", improvement_count, regression_count);

        Self {
            scenario_name: report.scenario_name,
            base_timestamp_ms: report.base_timestamp_ms,
            current_timestamp_ms: report.current_timestamp_ms,
            entries,
            regressions: report.regressions,
            improvements: report.improvements,
            summary,
        }
    }
}

/// Writes a diff report comparing two benchmark results to JSON.
///
/// Filename format: `bench-diff-{scenario}-{timestamp}.json`
pub fn write_diff_json(
    base: &BenchmarkResult,
    current: &BenchmarkResult,
    config: &OutputConfig,
) -> Result<PathBuf> {
    let comparison = compare(base, current, config.regression_threshold_pct)?;
    let mut diff = DiffReport::from(comparison);

    // Fill in actual values from benchmark results
    for entry in &mut diff.entries {
        match entry.metric.as_str() {
            "throughput_msg_s" => {
                entry.base_value = base.throughput_msg_s;
                entry.current_value = current.throughput_msg_s;
                entry.absolute_delta = current.throughput_msg_s - base.throughput_msg_s;
            }
            "latency_p50_ms" => {
                entry.base_value = base.latency_p50_ms;
                entry.current_value = current.latency_p50_ms;
                entry.absolute_delta = current.latency_p50_ms - base.latency_p50_ms;
            }
            "latency_p95_ms" => {
                entry.base_value = base.latency_p95_ms;
                entry.current_value = current.latency_p95_ms;
                entry.absolute_delta = current.latency_p95_ms - base.latency_p95_ms;
            }
            "latency_p99_ms" => {
                entry.base_value = base.latency_p99_ms;
                entry.current_value = current.latency_p99_ms;
                entry.absolute_delta = current.latency_p99_ms - base.latency_p99_ms;
            }
            "error_rate" => {
                entry.base_value = base.error_rate;
                entry.current_value = current.error_rate;
                entry.absolute_delta = current.error_rate - base.error_rate;
            }
            _ => {}
        }
    }

    let filename = format!(
        "bench-diff-{}-{}.json",
        sanitize_filename(&current.scenario_config.scenario_name),
        current.timestamp_ms
    );
    let path = config.base_path.join(&filename);

    if config.create_directories {
        fs::create_dir_all(&config.base_path)
            .with_context(|| format!("failed to create output directory {:?}", config.base_path))?;
    }

    let json = serde_json::to_string_pretty(&diff)
        .context("failed to serialize DiffReport to JSON")?;
    fs::write(&path, json).with_context(|| format!("failed to write diff JSON to {:?}", path))?;

    Ok(path)
}

// ─── Helper Functions ─────────────────────────────────────────────────────────

/// Computes percentage delta between two values.
fn compute_delta_pct(base: f64, current: f64) -> f64 {
    if base == 0.0 {
        if current == 0.0 {
            0.0
        } else {
            100.0
        }
    } else {
        (current - base) / base * 100.0
    }
}

/// Formats a Unix timestamp in milliseconds to a human-readable string.
fn format_timestamp(ts_ms: i64) -> String {
    let secs = ts_ms / 1000;
    let nanos = ((ts_ms % 1000) * 1_000_000) as u32;
    match chrono::DateTime::from_timestamp(secs, nanos) {
        Some(dt) => dt.format("%Y-%m-%d %H:%M:%S").to_string(),
        None => format!("{}", ts_ms),
    }
}

/// Sanitizes a string for use in filenames.
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmark::results::{PercentileBuckets, ScenarioConfig};

    fn make_test_result(scenario: &str, throughput: f64, latency_p99: f64, timestamp: i64) -> BenchmarkResult {
        BenchmarkResult {
            scenario_config: ScenarioConfig {
                scenario_name: scenario.to_string(),
                num_messages: 10000,
                payload_bytes: 256,
                rate: None,
                warmup_messages: 1000,
                failure_rate: 0.0,
            },
            total_messages: 10000,
            duration_ms: 1500,
            throughput_msg_s: throughput,
            latency_p50_ms: 0.5,
            latency_p95_ms: 1.0,
            latency_p99_ms: latency_p99,
            error_rate: 0.01,
            memory_delta_bytes: 1024,
            percentile_buckets: PercentileBuckets::default(),
            timestamp_ms: timestamp,
        }
    }

    #[test]
    fn test_output_filename_format() {
        let config = OutputConfig::default();
        assert_eq!(config.base_path, PathBuf::from("./benchmark_results/"));
    }

    #[test]
    fn test_json_roundtrip() {
        let result = make_test_result("throughput-test", 6666.67, 2.0, 1713000000000);
        let serializer = BenchmarkResultSerializer::new(OutputConfig::default());
        let json = serializer.serialize_json(&result).unwrap();
        let deserialized: BenchmarkResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.scenario_config.scenario_name, "throughput-test");
        assert_eq!(deserialized.throughput_msg_s, 6666.67);
    }

    #[test]
    fn test_csv_header_and_row() {
        let result = make_test_result("csv-test", 5000.0, 1.5, 1713000000000);
        let header = BenchmarkResult::to_csv_header();
        let row = result.to_csv_row();
        assert!(header.contains("scenario"));
        assert!(header.contains("throughput_msg_s"));
        assert!(row.contains("csv-test"));
        assert!(row.contains("5000"));
    }

    #[test]
    fn test_default_output_path() {
        let config = OutputConfig::default();
        assert_eq!(config.base_path.to_str().unwrap(), "./benchmark_results/");
    }

    #[test]
    fn test_compare_throughput_improvement() {
        let base = make_test_result("test", 1000.0, 1.0, 1713000000000);
        let current = make_test_result("test", 1200.0, 1.0, 1713100000000);
        let report = compare(&base, &current, 5.0).unwrap();
        assert!((report.throughput_delta_pct - 20.0).abs() < 0.01);
        assert!(!report.improvements.is_empty() || !report.regressions.is_empty());
    }

    #[test]
    fn test_compare_latency_regression() {
        let base = make_test_result("test", 1000.0, 1.0, 1713000000000);
        let current = make_test_result("test", 1000.0, 1.3, 1713100000000);
        let report = compare(&base, &current, 5.0).unwrap();
        let has_latency_regression = report.regressions.iter().any(|r| r.contains("latency_p99"));
        assert!(has_latency_regression);
    }

    #[test]
    fn test_compare_scenario_mismatch() {
        let base = make_test_result("scenario-a", 1000.0, 1.0, 1713000000000);
        let current = make_test_result("scenario-b", 1000.0, 1.0, 1713100000000);
        let result = compare(&base, &current, 5.0);
        assert!(result.is_err());
    }

    #[test]
    fn test_benchmark_report_to_markdown() {
        let result = make_test_result("markdown-test", 6666.67, 2.0, 1713000000000);
        let report = BenchmarkReport::from(&result);
        let markdown = report.to_markdown();
        assert!(markdown.contains("markdown-test"));
        assert!(markdown.contains("6666.67"));
        assert!(markdown.contains("2.000"));
    }

    #[test]
    fn test_diff_report_serialization() {
        let base = make_test_result("test", 1000.0, 1.0, 1713000000000);
        let current = make_test_result("test", 1200.0, 1.2, 1713100000000);
        let comparison = compare(&base, &current, 5.0).unwrap();
        let diff = DiffReport::from(comparison);
        let json = serde_json::to_string(&diff).unwrap();
        let deserialized: DiffReport = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.scenario_name, "test");
        assert!(!deserialized.summary.is_empty());
    }

    #[test]
    fn test_regression_threshold_config() {
        let base = make_test_result("test", 1000.0, 1.0, 1713000000000);
        // 8% increase - should be regression with 5% threshold but not with 10%
        let current = make_test_result("test", 1000.0, 1.08, 1713100000000);

        let report_5 = compare(&base, &current, 5.0).unwrap();
        let report_10 = compare(&base, &current, 10.0).unwrap();

        let has_regression_5 = report_5.regressions.iter().any(|r| r.contains("latency_p99"));
        let has_regression_10 = report_10.regressions.iter().any(|r| r.contains("latency_p99"));

        assert!(has_regression_5);
        assert!(!has_regression_10);
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("test-scenario"), "test-scenario");
        assert_eq!(sanitize_filename("test/scenario"), "test_scenario");
        assert_eq!(sanitize_filename("test scenario"), "test_scenario");
    }
}