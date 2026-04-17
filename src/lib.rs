use pyo3::prelude::*;

pub mod config;
pub mod errors;
pub mod kafka_message;
pub mod logging;
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

// Offset commit coordinator — per-topic-partition ack tracking with highest-contiguous-offset
pub mod coordinator;

// Failure classification — structured failure taxonomy for retry/DLQ handling
pub mod failure;

// Retry scheduling — RetryPolicy, RetrySchedule for exponential backoff with jitter
pub mod retry;

// DLQ routing — DlqMetadata envelope, DlqRouter trait, fire-and-forget produce
pub mod dlq;

// Routing — zero-copy context, decision enum, router trait, and concrete routers
pub mod routing;
pub use routing::config::{RoutingRule, RoutingRuleBuilder, PatternType, RoutingRuleBuildError};

use kafka_message::KafkaMessage;
use logging::Logger;
use produce::PyProducer;
use pyconsumer::Consumer;

#[pymodule]
fn _kafpy(m: &Bound<'_, PyModule>) -> PyResult<()> {
    Logger::init();

    m.add_class::<KafkaMessage>()?;
    m.add_class::<Consumer>()?;
    m.add_class::<PyProducer>()?;
    m.add_class::<config::ConsumerConfig>()?;
    m.add_class::<config::ProducerConfig>()?;
    Ok(())
}
