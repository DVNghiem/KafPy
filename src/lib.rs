use pyo3::prelude::*;

pub mod config;
pub mod errors;
pub mod kafka_message;
pub mod produce;
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

use kafka_message::KafkaMessage;
use logging::Logger;
use produce::PyProducer;
use pyconsumer::Consumer;

#[pymodule]
fn _kafpy(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Initialize logging with default observability config (OBS-38)
    let observability_config = crate::observability::ObservabilityConfig::default();
    Logger::init(&observability_config);

    m.add_class::<KafkaMessage>()?;
    m.add_class::<Consumer>()?;
    m.add_class::<PyProducer>()?;
    m.add_class::<config::ConsumerConfig>()?;
    m.add_class::<config::ProducerConfig>()?;
    Ok(())
}
