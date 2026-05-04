use pyo3::prelude::*;

pub mod config;
// Unified error re-exports (errors.rs remains internal as PyError)
pub mod error;
pub(crate) mod errors;
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

// Routing — zero-copy context, decision enum, router trait, and concrete routers
pub(crate) mod routing;

// Runtime assembly — RuntimeBuilder for composing pure-Rust consumer core
pub(crate) mod runtime;

pub mod rayon_pool;

pub mod middleware;

use kafka_message::KafkaMessage;
// logging::Logger removed — using Python logging
use produce::PyProducer;
use pyconsumer::PyConsumer;

#[pymodule]
fn _kafpy(m: &Bound<'_, PyModule>) -> PyResult<()> {

    m.add_class::<KafkaMessage>()?;
    m.add_class::<PyConsumer>()?;
    m.add_class::<PyProducer>()?;
    m.add_class::<config::ConsumerConfig>()?;
    m.add_class::<config::ProducerConfig>()?;
    m.add_class::<pyconfig::PyRetryPolicy>()?;
    m.add_class::<pyconfig::PyObservabilityConfig>()?;
    m.add_class::<pyconfig::PyFailureCategory>()?;
    m.add_class::<pyconfig::PyFailureReason>()?;

    // Fan-out and fan-in registration result types
    m.add_class::<pyconsumer::FanOutRegistration>()?;
    m.add_function(wrap_pyfunction!(pyconsumer::get_runtime_snapshot, m.py())?)?;
    m.add_function(wrap_pyfunction!(pyconsumer::register_status_callback, m.py())?)?;

    Ok(())
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
    crate::routing::header::HeaderRouter: Send + Sync,
    crate::routing::topic_pattern::TopicPatternRouter: Send + Sync,
    crate::routing::python_router::PythonRouter: Send + Sync,
{
}

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
