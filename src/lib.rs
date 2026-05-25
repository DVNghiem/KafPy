#![deny(dead_code)]

use pyo3::prelude::*;

pub mod bindings;
pub mod config;
pub mod log;

pub mod kafka_message;
pub mod producer;

// Pure Rust Kafka consumer core — no PyO3 dependencies
pub mod consumer;

// Pure Rust Kafka dispatcher — routes OwnedMessage to per-handler bounded channels
pub mod dispatcher;

// Callback execution lane — PythonHandler, Executor trait, ExecutionResult
pub mod execution;

// Worker pool — N Tokio workers polling handler queues, invoking Python callbacks
pub mod worker_pool;

// Internal-only modules — not exposed to Python, used within Rust crate

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

// Runtime assembly — RuntimeBuilder for composing pure-Rust consumer core
pub(crate) mod runtime;

pub mod middleware;

use kafka_message::KafkaMessage;
// logging::Logger removed — using Python logging
use consumer::runtime::PyConsumer;
use producer::PyProducer;

#[pymodule]
fn _kafpy(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<KafkaMessage>()?;
    m.add_class::<PyConsumer>()?;
    m.add_class::<PyProducer>()?;
    m.add_class::<config::ConsumerConfig>()?;
    m.add_class::<config::ProducerConfig>()?;
    m.add_class::<bindings::PyRetryPolicy>()?;
    m.add_class::<bindings::PyObservabilityConfig>()?;
    m.add_class::<bindings::PyFailureCategory>()?;
    m.add_class::<bindings::PyFailureReason>()?;

    // Fan-out and fan-in registration result types
    m.add_class::<consumer::runtime::FanOutRegistration>()?;
    m.add_function(wrap_pyfunction!(
        consumer::runtime::get_runtime_snapshot,
        m.py()
    )?)?;
    m.add_function(wrap_pyfunction!(
        consumer::runtime::register_status_callback,
        m.py()
    )?)?;

    Ok(())
}

// ─── Compile-time Send+Sync guarantees ─────────────────────────────────────────

/// Compile-time assertion that Dispatcher types are Send+Sync.
fn _assert_send_sync_dispatcher()
where
    crate::dispatcher::Dispatcher: Send + Sync,
    crate::dispatcher::DispatchOutcome: Send + Sync,
    crate::dispatcher::error::DispatchError: Send + Sync,
{
}

#[cfg(test)]
mod dispatcher_send_sync_assertions {
    use super::*;

    #[test]
    fn dispatcher_types_are_send_sync() {
        _assert_send_sync_dispatcher();
    }
}
