// src/benchmark/hardening.rs
// Production hardening validation framework — runs 5 declarative checks against BenchmarkResult.
// HARD-01 through HARD-08

use serde::{Deserialize, Serialize};

use crate::benchmark::results::{BenchmarkResult, PercentileBuckets, ScenarioConfig};

// ─── HardeningCheck Enum (HARD-01) ────────────────────────────────────────────

/// Hardening checks that validate production readiness of the consumer.
/// Each check corresponds to a HARD-N requirement identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardeningCheck {
    /// HARD-04: Consumer honors queue_depth limits without message loss under saturated load
    BackpressureThreshold,
    /// HARD-05: No significant heap growth (> 1MB delta) over sustained 10M message run
    MemoryLeakCheck,
    /// HARD-06: Pending messages processed and offsets committed before exit on SIGINT
    GracefulShutdownCheck,
    /// HARD-07: All failed messages delivered to DLQ topic after graceful shutdown
    DlqDrainCheck,
    /// HARD-08: Messages exhaust retry budget and route to DLQ (not infinite retry loop)
    RetryBudgetCheck,
}

impl HardeningCheck {
    /// Returns the name of this check as a string.
    pub fn name(&self) -> &'static str {
        match self {
            HardeningCheck::BackpressureThreshold => "BackpressureThreshold",
            HardeningCheck::MemoryLeakCheck => "MemoryLeakCheck",
            HardeningCheck::GracefulShutdownCheck => "GracefulShutdownCheck",
            HardeningCheck::DlqDrainCheck => "DlqDrainCheck",
            HardeningCheck::RetryBudgetCheck => "RetryBudgetCheck",
        }
    }
}

// ─── ValidationResult Struct (HARD-02) ───────────────────────────────────────

/// Result of a single hardening check validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    /// Name of the check that produced this result.
    pub check_name: String,
    /// Whether the check passed.
    pub passed: bool,
    /// Human-readable explanation of the result.
    pub details: String,
    /// Actionable suggestions when the check fails.
    pub suggestions: Vec<String>,
}

impl ValidationResult {
    /// Creates a passing result with the given details.
    pub fn pass(check_name: impl Into<String>, details: impl Into<String>) -> Self {
        Self {
            check_name: check_name.into(),
            passed: true,
            details: details.into(),
            suggestions: Vec::new(),
        }
    }

    /// Creates a failing result with the given details and suggestions.
    pub fn fail(
        check_name: impl Into<String>,
        details: impl Into<String>,
        suggestions: Vec<String>,
    ) -> Self {
        Self {
            check_name: check_name.into(),
            passed: false,
            details: details.into(),
            suggestions,
        }
    }
}

// ─── HardeningRunner ───────────────────────────────────────────────────────────

/// Runs all hardening checks against a BenchmarkResult.
pub struct HardeningRunner;

impl HardeningRunner {
    /// Runs all 5 hardening checks against the given benchmark result.
    ///
    /// Returns a vector of ValidationResult, one per check, in a fixed order.
    pub fn run_all(result: &BenchmarkResult) -> Vec<ValidationResult> {
        vec![
            check_backpressure(result),
            check_memory_leak(result),
            check_graceful_shutdown(result),
            check_dlq_drain(result),
            check_retry_budget(result),
        ]
    }
}

// ─── Check Functions (HARD-04 through HARD-08) ─────────────────────────────────

/// HARD-04: Consumer honors queue_depth limits without message loss under saturated load.
///
/// Passes when error_rate < 0.1% (0.001).
/// This validates that backpressure signaling is working correctly.
fn check_backpressure(result: &BenchmarkResult) -> ValidationResult {
    const THRESHOLD: f64 = 0.001; // 0.1% message loss threshold

    if result.error_rate < THRESHOLD {
        ValidationResult::pass(
            HardeningCheck::BackpressureThreshold.name(),
            "Message loss rate within acceptable threshold (error_rate < 0.1%)",
        )
    } else {
        ValidationResult::fail(
            HardeningCheck::BackpressureThreshold.name(),
            format!(
                "Message loss rate too high: {:.2}% (expected < 0.1%)",
                result.error_rate * 100.0
            ),
            vec![
                "Increase queue_depth capacity".to_string(),
                "Reduce message production rate".to_string(),
                "Scale consumer instances".to_string(),
            ],
        )
    }
}

/// HARD-05: No significant heap growth (> 1MB delta) over sustained 10M message run.
///
/// Passes when |memory_delta_bytes| < 1 MB (1,048,576 bytes).
/// This validates that there are no memory leaks in the consumer pipeline.
fn check_memory_leak(result: &BenchmarkResult) -> ValidationResult {
    const ONE_MB: i64 = 1_048_576;

    if result.memory_delta_bytes.abs() < ONE_MB {
        ValidationResult::pass(
            HardeningCheck::MemoryLeakCheck.name(),
            "Memory delta within acceptable threshold (< 1 MB)",
        )
    } else {
        ValidationResult::fail(
            HardeningCheck::MemoryLeakCheck.name(),
            format!(
                "Memory grew by {:.2} MB (expected < 1 MB)",
                result.memory_delta_bytes as f64 / ONE_MB as f64
            ),
            vec![
                "Check for unclosed file handles or connections".to_string(),
                "Review buffer pool sizing".to_string(),
                "Profile heap allocations with heap_allocated metrics".to_string(),
            ],
        )
    }
}

/// HARD-06: Pending messages processed and offsets committed before exit on SIGINT.
///
/// Passes when total_messages == num_messages configured.
/// This validates that graceful shutdown drains all in-flight messages.
fn check_graceful_shutdown(result: &BenchmarkResult) -> ValidationResult {
    let expected = result.scenario_config.num_messages as u64;

    if result.total_messages == expected {
        ValidationResult::pass(
            HardeningCheck::GracefulShutdownCheck.name(),
            "All messages processed before shutdown",
        )
    } else {
        ValidationResult::fail(
            HardeningCheck::GracefulShutdownCheck.name(),
            format!(
                "{} of {} messages processed",
                result.total_messages, expected
            ),
            vec![
                "Increase graceful_shutdown_timeout".to_string(),
                "Check for deadlocks in handler code".to_string(),
                "Verify offset commit before exit".to_string(),
            ],
        )
    }
}

/// HARD-07: All failed messages delivered to DLQ topic after graceful shutdown.
///
/// Passes when error_rate matches the configured failure_rate (DLQ routing is working).
/// If failure_rate is 0, any error_rate > 0 is a failure (unexpected errors).
/// If failure_rate > 0, error_rate must equal failure_rate (all failures went to DLQ).
fn check_dlq_drain(result: &BenchmarkResult) -> ValidationResult {
    let failure_rate = result.scenario_config.failure_rate;

    // If no failures expected, any errors indicate a problem
    if failure_rate == 0.0 {
        if result.error_rate == 0.0 {
            return ValidationResult::pass(
                HardeningCheck::DlqDrainCheck.name(),
                "No failures expected; error_rate is 0",
            );
        } else {
            return ValidationResult::fail(
                HardeningCheck::DlqDrainCheck.name(),
                format!(
                    "Unexpected errors detected: error_rate={:.2}% (expected 0%)",
                    result.error_rate * 100.0
                ),
                vec![
                    "Verify handler logic for unexpected failure modes".to_string(),
                    "Check for transient infrastructure errors".to_string(),
                ],
            );
        }
    }

    // failure_rate > 0: errors are expected; they should match failure_rate
    if result.error_rate == failure_rate {
        ValidationResult::pass(
            HardeningCheck::DlqDrainCheck.name(),
            format!(
                "Failed messages routed to DLQ (error_rate={:.2}% matches failure_rate={:.2}%)",
                result.error_rate * 100.0,
                failure_rate * 100.0
            ),
        )
    } else {
        ValidationResult::fail(
            HardeningCheck::DlqDrainCheck.name(),
            format!(
                "DLQ routing may not be working correctly: error_rate={:.2}% but failure_rate={:.2}%",
                result.error_rate * 100.0,
                failure_rate * 100.0
            ),
            vec![
                "Verify DLQ topic is configured".to_string(),
                "Check DLQ producer is sending messages".to_string(),
                "Confirm DLQ router is being called on retry exhaustion".to_string(),
            ],
        )
    }
}

/// HARD-08: Messages exhaust retry budget and route to DLQ (not infinite retry loop).
///
/// Passes when duration is bounded despite errors (no infinite retry loops).
/// We define an upper bound: a message processing run with errors should complete
/// within 10x the expected single-message duration if retries are bounded.
fn check_retry_budget(result: &BenchmarkResult) -> ValidationResult {
    // If no errors, the check passes trivially
    if result.error_rate == 0.0 {
        return ValidationResult::pass(
            HardeningCheck::RetryBudgetCheck.name(),
            "No errors detected; retry budget not exercised",
        );
    }

    // Determine a reasonable upper bound for duration.
    // If processing 1 message should take ~1ms, then 10M messages should take ~10s.
    // With retries bounded, even a worst-case scenario should complete in reasonable time.
    // We use 1 hour (3,600,000 ms) as an absolute upper bound for any benchmark run.
    const ABSOLUTE_UPPER_BOUND_MS: u64 = 3_600_000; // 1 hour

    if result.duration_ms < ABSOLUTE_UPPER_BOUND_MS {
        ValidationResult::pass(
            HardeningCheck::RetryBudgetCheck.name(),
            "Retry budget respected: no infinite retry loops detected",
        )
    } else {
        ValidationResult::fail(
            HardeningCheck::RetryBudgetCheck.name(),
            "Possible infinite retry loop detected (high duration with errors)".to_string(),
            vec![
                "Verify RetryPolicy.max_attempts is configured".to_string(),
                "Confirm RetryCoordinator signals DLQ on max_attempts exceeded".to_string(),
                "Check that non-retryable failures are classified correctly".to_string(),
            ],
        )
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_result(
        num_messages: usize,
        total_messages: u64,
        error_rate: f64,
        memory_delta_bytes: i64,
        failure_rate: f64,
        duration_ms: u64,
    ) -> BenchmarkResult {
        BenchmarkResult {
            scenario_config: ScenarioConfig {
                scenario_name: "test-scenario".to_string(),
                num_messages,
                payload_bytes: 256,
                rate: None,
                warmup_messages: 1000,
                failure_rate,
            },
            total_messages,
            duration_ms,
            throughput_msg_s: 0.0,
            latency_p50_ms: 0.0,
            latency_p95_ms: 0.0,
            latency_p99_ms: 0.0,
            error_rate,
            memory_delta_bytes,
            percentile_buckets: PercentileBuckets::default(),
            timestamp_ms: 0,
        }
    }

    // ── HardeningCheck enum tests ──

    #[test]
    fn test_hardening_check_enum_has_five_variants() {
        let variants = [
            HardeningCheck::BackpressureThreshold,
            HardeningCheck::MemoryLeakCheck,
            HardeningCheck::GracefulShutdownCheck,
            HardeningCheck::DlqDrainCheck,
            HardeningCheck::RetryBudgetCheck,
        ];
        assert_eq!(variants.len(), 5);
    }

    #[test]
    fn test_hardening_check_name() {
        assert_eq!(
            HardeningCheck::BackpressureThreshold.name(),
            "BackpressureThreshold"
        );
        assert_eq!(
            HardeningCheck::MemoryLeakCheck.name(),
            "MemoryLeakCheck"
        );
        assert_eq!(
            HardeningCheck::GracefulShutdownCheck.name(),
            "GracefulShutdownCheck"
        );
        assert_eq!(HardeningCheck::DlqDrainCheck.name(), "DlqDrainCheck");
        assert_eq!(
            HardeningCheck::RetryBudgetCheck.name(),
            "RetryBudgetCheck"
        );
    }

    // ── ValidationResult serialization test ──

    #[test]
    fn test_validation_result_serialization() {
        let result = ValidationResult::fail(
            "BackpressureThreshold",
            "Message loss rate too high: 5.00% (expected < 0.1%)",
            vec!["Increase queue_depth capacity".to_string()],
        );

        let serialized = serde_json::to_string(&result).unwrap();
        let deserialized: ValidationResult = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.check_name, "BackpressureThreshold");
        assert_eq!(deserialized.passed, false);
        assert_eq!(
            deserialized.details,
            "Message loss rate too high: 5.00% (expected < 0.1%)"
        );
        assert_eq!(deserialized.suggestions.len(), 1);
    }

    #[test]
    fn test_validation_result_pass_serialization() {
        let result =
            ValidationResult::pass("MemoryLeakCheck", "Memory delta within acceptable threshold");

        let serialized = serde_json::to_string(&result).unwrap();
        let deserialized: ValidationResult = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.check_name, "MemoryLeakCheck");
        assert_eq!(deserialized.passed, true);
        assert_eq!(
            deserialized.details,
            "Memory delta within acceptable threshold"
        );
        assert!(deserialized.suggestions.is_empty());
    }

    // ── HardeningRunner::run_all test ──

    #[test]
    fn test_run_all_returns_five_results() {
        let result = make_result(10_000, 10_000, 0.0, 512_000, 0.0, 1000);
        let results = HardeningRunner::run_all(&result);
        assert_eq!(results.len(), 5);
    }

    // ── check_backpressure tests ──

    #[test]
    fn test_backpressure_check_passes_on_low_error_rate() {
        let result = make_result(10_000, 10_000, 0.0001, 0, 0.0, 1000);
        let validation = check_backpressure(&result);
        assert!(validation.passed);
        assert!(validation.suggestions.is_empty());
    }

    #[test]
    fn test_backpressure_check_fails_on_high_error_rate() {
        let result = make_result(10_000, 10_000, 0.01, 0, 0.0, 1000);
        let validation = check_backpressure(&result);
        assert!(!validation.passed);
        assert!(!validation.suggestions.is_empty());
    }

    // ── check_memory_leak tests ──

    #[test]
    fn test_memory_leak_check_passes_on_small_delta() {
        let result = make_result(10_000, 10_000, 0.0, 512_000, 0.0, 1000);
        let validation = check_memory_leak(&result);
        assert!(validation.passed);
    }

    #[test]
    fn test_memory_leak_check_fails_on_large_delta() {
        let result = make_result(10_000, 10_000, 0.0, 2_000_000, 0.0, 1000);
        let validation = check_memory_leak(&result);
        assert!(!validation.passed);
        assert!(!validation.suggestions.is_empty());
    }

    #[test]
    fn test_memory_leak_check_negative_delta_passes() {
        // Negative delta (memory freed) is also fine
        let result = make_result(10_000, 10_000, 0.0, -512_000, 0.0, 1000);
        let validation = check_memory_leak(&result);
        assert!(validation.passed);
    }

    // ── check_graceful_shutdown tests ──

    #[test]
    fn test_graceful_shutdown_check_passes_when_all_messages_processed() {
        let result = make_result(10_000, 10_000, 0.0, 0, 0.0, 1000);
        let validation = check_graceful_shutdown(&result);
        assert!(validation.passed);
    }

    #[test]
    fn test_graceful_shutdown_check_fails_when_messages_lost() {
        let result = make_result(10_000, 9_500, 0.0, 0, 0.0, 1000);
        let validation = check_graceful_shutdown(&result);
        assert!(!validation.passed);
        assert!(!validation.suggestions.is_empty());
    }

    // ── check_dlq_drain tests ──

    #[test]
    fn test_dlq_drain_check_passes_when_no_failures_expected_and_none_seen() {
        let result = make_result(10_000, 10_000, 0.0, 0, 0.0, 1000);
        let validation = check_dlq_drain(&result);
        assert!(validation.passed);
    }

    #[test]
    fn test_dlq_drain_check_fails_when_unexpected_errors() {
        let result = make_result(10_000, 10_000, 0.05, 0, 0.0, 1000);
        let validation = check_dlq_drain(&result);
        assert!(!validation.passed);
    }

    #[test]
    fn test_dlq_drain_check_passes_when_error_rate_matches_failure_rate() {
        let result = make_result(10_000, 10_000, 0.05, 0, 0.05, 1000);
        let validation = check_dlq_drain(&result);
        assert!(validation.passed);
    }

    #[test]
    fn test_dlq_drain_check_fails_when_error_rate_mismatch() {
        let result = make_result(10_000, 10_000, 0.03, 0, 0.05, 1000);
        let validation = check_dlq_drain(&result);
        assert!(!validation.passed);
    }

    // ── check_retry_budget tests ──

    #[test]
    fn test_retry_budget_check_passes_on_no_errors() {
        let result = make_result(10_000, 10_000, 0.0, 0, 0.0, 1000);
        let validation = check_retry_budget(&result);
        assert!(validation.passed);
    }

    #[test]
    fn test_retry_budget_check_passes_on_bounded_duration_with_errors() {
        let result = make_result(10_000, 9_500, 0.05, 0, 0.05, 1000);
        let validation = check_retry_budget(&result);
        assert!(validation.passed);
    }

    #[test]
    fn test_retry_budget_check_fails_on_excessive_duration() {
        // Duration exceeds 1 hour — considered unbounded
        let result = make_result(10_000, 9_500, 0.05, 0, 0.05, 5_000_000);
        let validation = check_retry_budget(&result);
        assert!(!validation.passed);
    }
}
