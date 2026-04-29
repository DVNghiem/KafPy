// src/benchmark/scenarios.rs
// Scenario definitions — WHAT to benchmark (consumed by BenchmarkRunner in Phase 40).

use serde::{Deserialize, Serialize};

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

// WorkloadProfile, RetryPolicy, FailureScenario, BatchVsSyncScenario, AsyncVsSyncScenario
// deleted — all had zero internal references.

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
}
