use pyo3::prelude::*;

pub mod config;
// Unified error re-exports (errors.rs remains internal as PyError)
pub(crate) mod errors;
pub mod error;
pub mod kafka_message;
pub mod produce;
pub mod pyconfig;
pub mod pyconsumer;

// Pure Rust Kafka consumer core — no PyO3 dependencies
pub mod consumer;

// Pure Rust Kafka dispatcher — routes OwnedMessage to per-handler bounded channels
pub mod dispatcher;

// Python execution lane — PythonHandler, Executor trait, ExecutionResult
pub mod python;

// Worker pool — N Tokio workers polling handler queues, invoking Python callbacks
pub mod worker_pool;

// Internal-only modules — not exposed to Python, used within Rust crate

// Offset commit coordinator — per-topic-partition ack tracking with highest-contiguous-offset
pub(crate) mod coordinator;

// Failure classification — structured failure taxonomy for retry/DLQ handling
pub(crate) mod failure;

// Retry scheduling — RetryPolicy, RetrySchedule for exponential backoff with jitter
pub(crate) mod retry;

// Shutdown coordination — 4-phase shutdown lifecycle
pub(crate) mod shutdown;

// Offset tracking — highest-contiguous-offset algorithm
pub(crate) mod offset;

// DLQ routing — DlqMetadata envelope, DlqRouter trait, fire-and-forget produce
pub(crate) mod dlq;

// Observability — metrics sink, metric labels, handler metrics, queue snapshots
pub(crate) mod observability;

// Benchmark infrastructure — measurement types, latency/throughput tracking
pub(crate) mod benchmark;

// Routing — zero-copy context, decision enum, router trait, and concrete routers
pub(crate) mod routing;

// Logging — internal logger initialization used by other modules
mod logging;

// Runtime assembly — RuntimeBuilder for composing pure-Rust consumer core
pub(crate) mod runtime;

use kafka_message::KafkaMessage;
// logging::Logger removed — using Python logging
use produce::PyProducer;
use pyconsumer::PyConsumer;

use benchmark::hardening::HardeningRunner;
use benchmark::results::BenchmarkResult;
use benchmark::runner::BenchmarkRunner;

#[pymodule]
fn _kafpy(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Initialize logging (no-op — Python logging is configured by user)
    crate::logging::init();

    m.add_class::<KafkaMessage>()?;
    m.add_class::<PyConsumer>()?;
    m.add_class::<PyProducer>()?;
    m.add_class::<config::ConsumerConfig>()?;
    m.add_class::<config::ProducerConfig>()?;
    m.add_class::<pyconfig::PyRetryPolicy>()?;
    m.add_class::<pyconfig::PyObservabilityConfig>()?;
    m.add_class::<pyconfig::PyFailureCategory>()?;
    m.add_class::<pyconfig::PyFailureReason>()?;

    // Benchmark PyO3 functions
    m.add_function(wrap_pyfunction!(run_scenario_py, m.py())?)?;
    m.add_function(wrap_pyfunction!(run_hardening_checks_py, m.py())?)?;

    Ok(())
}

// ─── PyO3 benchmark bindings ────────────────────────────────────────────────────

/// Runs a benchmark scenario and returns the result as a JSON string.
///
/// Args:
///     scenario_name: One of "throughput", "latency", "failure", "batch_vs_sync", "async_vs_sync"
///     config_json: JSON configuration for the scenario
///
/// Returns:
///     JSON string with BenchmarkResult fields
#[pyfunction]
fn run_scenario_py(_py: Python<'_>, scenario_name: &str, config_json: &str) -> PyResult<String> {
    // Deserialize scenario config
    let scenario: Box<dyn benchmark::scenarios::Scenario> = match scenario_name {
        "throughput" => {
            match serde_json::from_str::<benchmark::scenarios::ThroughputScenario>(config_json) {
                Ok(cfg) => Box::new(cfg),
                Err(e) => return Err(pyo3::exceptions::PyValueError::new_err(format!("invalid config for throughput scenario: {e}"))),
            }
        }
        "latency" => {
            match serde_json::from_str::<benchmark::scenarios::LatencyScenario>(config_json) {
                Ok(cfg) => Box::new(cfg),
                Err(e) => return Err(pyo3::exceptions::PyValueError::new_err(format!("invalid config for latency scenario: {e}"))),
            }
        }
        _ => {
            return Err(pyo3::exceptions::PyValueError::new_err(
                format!("unknown scenario '{scenario_name}', expected one of: throughput, latency, failure, batch_vs_sync, async_vs_sync")
            ));
        }
    };

    // Run benchmark synchronously on a tokio runtime
    let rt = tokio::runtime::Runtime::new().map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("failed to create tokio runtime: {e}")))?;
    let result = rt.block_on(async {
        let runner = BenchmarkRunner::new(None);
        runner.run_scenario(scenario).await
    });

    match result {
        Ok(r) => {
            // Build JSON dict manually to avoid extra serde overhead
            let json = serde_json::json!({
                "scenario_name": r.scenario_config.scenario_name,
                "total_messages": r.total_messages,
                "duration_ms": r.duration_ms,
                "throughput_msg_s": r.throughput_msg_s,
                "latency_p50_ms": r.latency_p50_ms,
                "latency_p95_ms": r.latency_p95_ms,
                "latency_p99_ms": r.latency_p99_ms,
                "error_rate": r.error_rate,
                "memory_delta_bytes": r.memory_delta_bytes,
                "timestamp_ms": r.timestamp_ms,
            });
            Ok(serde_json::to_string(&json).unwrap())
        }
        Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(format!("benchmark run failed: {e}")))
    }
}

/// Runs hardening checks against a benchmark result and returns validation results as JSON.
///
/// Args:
///     result_json: JSON string with BenchmarkResult fields
///
/// Returns:
///     JSON array of ValidationResult objects
#[pyfunction]
fn run_hardening_checks_py(_py: Python<'_>, result_json: &str) -> PyResult<String> {
    let result: BenchmarkResult = serde_json::from_str(result_json)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("invalid result JSON: {e}")))?;

    let validations = HardeningRunner::run_all(&result);

    let json_array: Vec<serde_json::Value> = validations
        .iter()
        .map(|v| {
            serde_json::json!({
                "check_name": v.check_name,
                "passed": v.passed,
                "details": v.details,
                "suggestions": v.suggestions,
            })
        })
        .collect();

    serde_json::to_string(&json_array)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("failed to serialize results: {e}")))
}

// ─── Compile-time Send+Sync guarantees ─────────────────────────────────────────

/// Compile-time assertion that all routing types are Send+Sync.
/// Breaking these guarantees would introduce data races in the dispatcher.
fn _assert_send_sync_routing()
where
    crate::routing::HandlerId: Send + Sync,
    crate::routing::context::RoutingContext<'static>: Send + Sync,
    crate::routing::decision::RoutingDecision: Send + Sync,
    crate::routing::key::KeyRouter: Send + Sync,
{}

#[cfg(test)]
mod send_sync_assertions {
    use super::*;

    #[test]
    fn routing_types_are_send_sync() {
        _assert_send_sync_routing();
    }
}

/// Compile-time assertion that Dispatcher types are Send+Sync.
fn _assert_send_sync_dispatcher()
where
    crate::dispatcher::Dispatcher: Send + Sync,
    crate::dispatcher::DispatchOutcome: Send + Sync,
    crate::dispatcher::error::DispatchError: Send + Sync,
{}

#[cfg(test)]
mod dispatcher_send_sync_assertions {
    use super::*;

    #[test]
    fn dispatcher_types_are_send_sync() {
        _assert_send_sync_dispatcher();
    }
}
