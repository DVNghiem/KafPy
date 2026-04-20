// src/benchmark/scenarios.rs
// Scenario definitions — WHAT to benchmark (consumed by BenchmarkRunner in Phase 40).

use serde::{Deserialize, Serialize};
use std::time::Duration;

pub(crate) use crate::benchmark::results::ScenarioConfig;

// ─── Scenario Trait ────────────────────────────────────────────────────────────

/// Defines the interface for benchmark scenarios.
///
/// Implementors describe WHAT is being benchmarked:
/// topic, rate, payload size, duration, warmup — while the BenchmarkRunner
/// (Phase 40) handles the HOW of actually running the benchmark.
pub trait Scenario: Send + Sync {
    /// Returns the scenario type name (e.g., "throughput", "latency").
    fn scenario_name(&self) -> &str;

    /// Returns the configuration for this scenario.
    fn build_config(&self) -> ScenarioConfig;

    /// Returns the default number of warmup messages.
    fn default_warmup_messages(&self) -> usize {
        1000
    }
}

// ─── WorkloadProfile Enum ──────────────────────────────────────────────────────

/// Discriminates five distinct workload types used to classify benchmarks (D-03).
///
/// Each variant tunes measurement behavior, reporting aggregation, and
/// scenario structure expectations in the BenchmarkRunner (Phase 40).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorkloadProfile {
    /// Throughput-oriented workload: producer goes as fast as possible
    /// or targets a fixed message rate.
    ThroughputFocused,
    /// Latency-oriented workload: steady background load with single-message
    /// round-trip latency measurements.
    LatencyFocused,
    /// Failure-oriented workload: injects failures at a configured rate
    /// and measures recovery behavior.
    FailureFocused,
    /// Batch comparison workload: measures throughput difference between
    /// batch-enabled and synchronous produce paths.
    BatchComparison,
    /// Handler mode comparison workload: measures throughput difference
    /// between async and sync handler execution modes.
    HandlerModeComparison,
}

// ─── ThroughputScenario ────────────────────────────────────────────────────────

/// Throughput-oriented benchmark scenario (SCEN-03).
///
/// Configurable messages_per_second, payload_bytes, num_messages or duration.
/// rate = None means unlimited (producer goes as fast as possible).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThroughputScenario {
    /// Target Kafka topic for producing messages.
    pub target_topic: String,
    /// Target message rate in messages per second.
    /// None = unlimited (producer goes as fast as possible).
    pub messages_per_second: Option<usize>,
    /// Size of each message payload in bytes.
    pub payload_bytes: usize,
    /// Total number of messages to send.
    /// None = use duration_secs instead.
    pub num_messages: Option<usize>,
    /// Total duration of the measurement window in seconds.
    /// None = use num_messages instead.
    pub duration_secs: Option<u64>,
    /// Number of warmup messages sent before measurement begins (D-12).
    pub warmup_messages: usize,
}

impl Scenario for ThroughputScenario {
    fn scenario_name(&self) -> &str {
        "throughput"
    }

    fn build_config(&self) -> ScenarioConfig {
        ScenarioConfig {
            scenario_name: self.scenario_name().to_string(),
            num_messages: self.num_messages.unwrap_or(0),
            payload_bytes: self.payload_bytes,
            rate: self.messages_per_second,
            warmup_messages: self.warmup_messages,
            failure_rate: 0.0, // throughput scenario assumes no failures
        }
    }

    fn default_warmup_messages(&self) -> usize {
        self.warmup_messages
    }
}

// ─── LatencyScenario ──────────────────────────────────────────────────────────

/// Latency-focused benchmark scenario (SCEN-04).
///
/// Measures single-message latency under steady-state load.
/// Uses a steady message rate (messages_per_second) to generate background load
/// while measuring individual message round-trip latency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyScenario {
    /// Target Kafka topic for producing messages.
    pub target_topic: String,
    /// Steady-state message rate for background load generation.
    /// Unlike ThroughputScenario, this is required (not Option) because
    /// a steady state is needed for meaningful latency measurement.
    pub messages_per_second: usize,
    /// Size of each message payload in bytes.
    pub payload_bytes: usize,
    /// Total messages to send in the measurement window.
    pub num_messages: usize,
    /// Number of warmup messages sent before measurement begins (D-12).
    pub warmup_messages: usize,
}

impl Scenario for LatencyScenario {
    fn scenario_name(&self) -> &str {
        "latency"
    }

    fn build_config(&self) -> ScenarioConfig {
        ScenarioConfig {
            scenario_name: self.scenario_name().to_string(),
            num_messages: self.num_messages,
            payload_bytes: self.payload_bytes,
            rate: Some(self.messages_per_second),
            warmup_messages: self.warmup_messages,
            failure_rate: 0.0, // latency scenario assumes no failures
        }
    }

    fn default_warmup_messages(&self) -> usize {
        self.warmup_messages
    }
}

// ─── RetryPolicy (local copy for serde) ───────────────────────────────────────

/// Retry policy configuration for message processing.
///
/// # Defaults
/// - max_attempts: 3
/// - base_delay: 100ms
/// - max_delay: 30s
/// - jitter_factor: 0.1
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_attempts: usize,
    pub base_delay: Duration,
    pub max_delay: Duration,
    pub jitter_factor: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            jitter_factor: 0.1,
        }
    }
}

// ─── FailureScenario ────────────────────────────────────────────────────────────

/// Failure scenario exercising retry behavior and DLQ routing (SCEN-05).
///
/// Configurable failure injection rate, retry policy, and DLQ topic routing.
/// Verifies that failed messages route to DLQ after retry budget exhaustion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureScenario {
    /// Target topic for the primary message stream.
    pub target_topic: String,
    /// DLQ topic for routing failed messages after retry exhaustion.
    pub dlq_topic: String,
    /// Percentage of messages to inject failures on (0.0 to 1.0).
    pub failure_rate: f64,
    /// Retry policy for transient failures.
    pub retry_policy: RetryPolicy,
    pub payload_bytes: usize,
    /// Total messages to send in this scenario.
    pub num_messages: usize,
    /// Warmup messages excluded from failure rate measurement.
    pub warmup_messages: usize,
}

impl Scenario for FailureScenario {
    fn scenario_name(&self) -> &str {
        "failure"
    }

    fn build_config(&self) -> ScenarioConfig {
        ScenarioConfig {
            scenario_name: self.scenario_name().to_string(),
            num_messages: self.num_messages,
            payload_bytes: self.payload_bytes,
            rate: None,
            warmup_messages: self.warmup_messages,
            failure_rate: self.failure_rate,
        }
    }

    fn default_warmup_messages(&self) -> usize {
        self.warmup_messages
    }
}

impl Default for FailureScenario {
    fn default() -> Self {
        Self {
            target_topic: "benchmark".to_string(),
            dlq_topic: "benchmark-dlq".to_string(),
            failure_rate: 0.05,
            retry_policy: RetryPolicy::default(),
            payload_bytes: 256,
            num_messages: 100_000,
            warmup_messages: 1000,
        }
    }
}

// ─── BatchVsSyncScenario ───────────────────────────────────────────────────────

/// Batch vs synchronous handler mode comparison scenario (SCEN-06).
///
/// Compares BatchSync vs SingleSync handler modes under identical workload
/// to measure the overhead/benefit of batch processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchVsSyncScenario {
    pub target_topic: String,
    /// Steady message rate (messages/second) for background load.
    pub messages_per_second: usize,
    pub payload_bytes: usize,
    /// Batch size for the BatchSync mode comparison.
    pub batch_size: usize,
    /// Total messages to send in the measurement window.
    pub num_messages: usize,
    pub warmup_messages: usize,
}

impl Scenario for BatchVsSyncScenario {
    fn scenario_name(&self) -> &str {
        "batch_vs_sync"
    }

    fn build_config(&self) -> ScenarioConfig {
        // Include batch_size in name so results distinguish runs
        ScenarioConfig {
            scenario_name: format!("{}_{}", self.scenario_name(), self.batch_size),
            num_messages: self.num_messages,
            payload_bytes: self.payload_bytes,
            rate: Some(self.messages_per_second),
            warmup_messages: self.warmup_messages,
            failure_rate: 0.0,
        }
    }

    fn default_warmup_messages(&self) -> usize {
        self.warmup_messages
    }
}

impl Default for BatchVsSyncScenario {
    fn default() -> Self {
        Self {
            target_topic: "benchmark".to_string(),
            messages_per_second: 10_000,
            payload_bytes: 256,
            batch_size: 100,
            num_messages: 100_000,
            warmup_messages: 1000,
        }
    }
}

// ─── AsyncVsSyncScenario ────────────────────────────────────────────────────────

/// Async vs synchronous handler mode comparison scenario (SCEN-07).
///
/// Compares SingleAsync vs SingleSync handler modes under identical workload
/// to measure the overhead/benefit of async processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsyncVsSyncScenario {
    pub target_topic: String,
    /// Steady message rate (messages/second) for background load.
    pub messages_per_second: usize,
    pub payload_bytes: usize,
    /// Total messages to send in the measurement window.
    pub num_messages: usize,
    pub warmup_messages: usize,
}

impl Scenario for AsyncVsSyncScenario {
    fn scenario_name(&self) -> &str {
        "async_vs_sync"
    }

    fn build_config(&self) -> ScenarioConfig {
        ScenarioConfig {
            scenario_name: self.scenario_name().to_string(),
            num_messages: self.num_messages,
            payload_bytes: self.payload_bytes,
            rate: Some(self.messages_per_second),
            warmup_messages: self.warmup_messages,
            failure_rate: 0.0,
        }
    }

    fn default_warmup_messages(&self) -> usize {
        self.warmup_messages
    }
}

impl Default for AsyncVsSyncScenario {
    fn default() -> Self {
        Self {
            target_topic: "benchmark".to_string(),
            messages_per_second: 10_000,
            payload_bytes: 256,
            num_messages: 100_000,
            warmup_messages: 1000,
        }
    }
}

// ─── Scenario Configurations for common use cases ───────────────────────────────

impl Default for ThroughputScenario {
    fn default() -> Self {
        Self {
            target_topic: "benchmark".to_string(),
            messages_per_second: None,
            payload_bytes: 256,
            num_messages: Some(100_000),
            duration_secs: None,
            warmup_messages: 1000,
        }
    }
}

impl Default for LatencyScenario {
    fn default() -> Self {
        Self {
            target_topic: "benchmark".to_string(),
            messages_per_second: 10_000,
            payload_bytes: 256,
            num_messages: 100_000,
            warmup_messages: 1000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn throughput_scenario_build_config() {
        let scenario = ThroughputScenario {
            target_topic: "test-topic".to_string(),
            messages_per_second: Some(50_000),
            payload_bytes: 512,
            num_messages: Some(500_000),
            duration_secs: None,
            warmup_messages: 2000,
        };

        let config = scenario.build_config();
        assert_eq!(config.scenario_name, "throughput");
        assert_eq!(config.num_messages, 500_000);
        assert_eq!(config.payload_bytes, 512);
        assert_eq!(config.rate, Some(50_000));
        assert_eq!(config.warmup_messages, 2000);
        assert_eq!(config.failure_rate, 0.0);
    }

    #[test]
    fn throughput_scenario_unlimited_rate() {
        let scenario = ThroughputScenario {
            target_topic: "test-topic".to_string(),
            messages_per_second: None, // unlimited
            payload_bytes: 256,
            num_messages: Some(100_000),
            duration_secs: None,
            warmup_messages: 1000,
        };

        let config = scenario.build_config();
        assert_eq!(config.rate, None);
    }

    #[test]
    fn latency_scenario_build_config() {
        let scenario = LatencyScenario {
            target_topic: "latency-topic".to_string(),
            messages_per_second: 20_000,
            payload_bytes: 128,
            num_messages: 200_000,
            warmup_messages: 1500,
        };

        let config = scenario.build_config();
        assert_eq!(config.scenario_name, "latency");
        assert_eq!(config.num_messages, 200_000);
        assert_eq!(config.payload_bytes, 128);
        assert_eq!(config.rate, Some(20_000));
        assert_eq!(config.warmup_messages, 1500);
    }

    #[test]
    fn scenario_default_warmup() {
        let throughput = ThroughputScenario::default();
        let latency = LatencyScenario::default();
        assert_eq!(throughput.warmup_messages, 1000);
        assert_eq!(latency.warmup_messages, 1000);
    }

    #[test]
    fn workload_profile_serialize() {
        let profiles = [
            WorkloadProfile::ThroughputFocused,
            WorkloadProfile::LatencyFocused,
            WorkloadProfile::FailureFocused,
            WorkloadProfile::BatchComparison,
            WorkloadProfile::HandlerModeComparison,
        ];

        for profile in &profiles {
            let serialized = serde_json::to_string(profile).unwrap();
            let deserialized: WorkloadProfile = serde_json::from_str(&serialized).unwrap();
            assert_eq!(*profile, deserialized);
        }
    }
}