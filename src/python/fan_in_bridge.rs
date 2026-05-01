//! Fan-in bridge — assembles fan-in config and registers the handler with PyConsumer.
//!
//! Called by `PyConsumer::register_fanin` to attach a handler to multiple Kafka topics
//! that will be merged into one round-robin stream.

use pyo3::prelude::*;
use crate::pyconsumer::PyConsumer;
use crate::python::handler::{HandlerMode, PythonHandler};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Global counter for generating unique fan_in_ids.
static FAN_IN_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Python-facing fan-in builder.
///
/// Constructed from Python via `FanInBuilderRust::new(...)`, then
/// `.register_into_consumer()` to attach the handler to the PyConsumer.
#[derive(Clone)]
pub struct FanInBuilderRust {
    handler_key: String,
    sources: Vec<String>,
    callback: Arc<pyo3::Py<pyo3::PyAny>>,
    mode: HandlerMode,
    timeout_ms: Option<u64>,
}

impl FanInBuilderRust {
    /// Create a new fan-in builder.
    ///
    /// - `handler_key` — identifier for this handler (used in QueueManager)
    /// - `sources` — list of topic names to subscribe to (D-01: multi-topic subscription)
    /// - `callback` — the Python callable invoked for each message
    /// - `mode` — handler execution mode
    /// - `timeout_ms` — per-handler execution timeout in milliseconds
    pub fn new(
        handler_key: String,
        sources: Vec<String>,
        callback: Arc<pyo3::Py<pyo3::PyAny>>,
        mode: HandlerMode,
        timeout_ms: Option<u64>,
    ) -> Self {
        Self {
            handler_key,
            sources,
            callback,
            mode,
            timeout_ms,
        }
    }

    /// Register the fan-in handler with the PyConsumer.
    ///
    /// Stores the handler metadata and marks it as a fan-in handler by setting fan_in_id.
    /// Returns `FanInRegistration` with handler_key, fan_in_id, and sources.
    pub fn register_into_consumer(self, py_consumer: &mut PyConsumer) -> FanInRegistration {
        use std::time::Duration;

        let fan_in_id = FAN_IN_COUNTER.fetch_add(1, Ordering::SeqCst);

        // Build a PythonHandler for this fan-in handler
        let timeout = self.timeout_ms.map(Duration::from_millis);
        let handler = PythonHandler::new(
            Arc::clone(&self.callback),
            None, // retry_policy
            self.mode.clone(),
            None, // batch_policy
            timeout,
            self.handler_key.clone(),
            None, // rayon_pool
            None, // middleware
        );

        // Register with PyConsumer's handler map with fan_in_id set
        py_consumer.add_handler_with_fan_in(
            self.handler_key.clone(),
            Arc::new(handler),
            fan_in_id,
        );

        FanInRegistration {
            handler_key: self.handler_key,
            fan_in_id,
            sources: self.sources,
        }
    }
}

/// Fan-in registration result returned to Python.
#[pyclass]
pub struct FanInRegistration {
    #[pyo3(get)]
    pub handler_key: String,
    #[pyo3(get)]
    pub fan_in_id: u64,
    #[pyo3(get)]
    pub sources: Vec<String>,
}