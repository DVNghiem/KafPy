//! Python-facing consumer that delegates to the pure-Rust consumer core.
//!
//! Bridges the PyO3 boundary: accepts Python callbacks, delegates message
//! ingestion to `ConsumerRunner`, converts `OwnedMessage` → `KafkaMessage`,
//! then dispatches to the Python handler.

use pyo3::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::config::ConsumerConfig;
use crate::consumer::{ConsumerConfigBuilder, ConsumerRunner};
use crate::kafka_message::KafkaMessage;

use tokio_stream::StreamExt;

/// Python-callable consumer. Use `add_handler` to register a topic → callback
/// mapping, then `start()` to begin consumption.
#[pyclass]
pub struct Consumer {
    /// Effective config for the Rust core (built from the pyclass fields).
    runner: Option<ConsumerRunner>,
    config: ConsumerConfig,
    handlers: Arc<Mutex<HashMap<String, Py<PyAny>>>>,
}

#[pymethods]
impl Consumer {
    #[new]
    pub fn new(config: ConsumerConfig) -> Self {
        Self {
            runner: None,
            config,
            handlers: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Registers a Python callback to handle messages from `topic`.
    /// The callback receives exactly one argument: a `KafkaMessage`.
    pub fn add_handler(&mut self, topic: String, callback: Bound<'_, PyAny>) {
        if let Ok(mut handlers) = self.handlers.lock() {
            handlers.insert(topic, callback.unbind());
        }
    }

    /// Starts the consumer and runs indefinitely, dispatching messages to
    /// registered Python handlers. Blocks the current thread.
    pub fn start(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let config = self.config.clone();
        let handlers = Arc::clone(&self.handlers);

        // Default handler if none registered
        let default_handler: Py<PyAny> = py
            .eval(c"lambda msg: print(f'received: {msg}')", None, None)?
            .unbind();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            // Build pure-Rust config from the PyO3 config fields
            let rust_config = ConsumerConfigBuilder::new()
                .brokers(&config.brokers)
                .group_id(&config.group_id)
                .topics(config.topics.iter().map(|s| s.as_str()))
                .auto_offset_reset(match config.auto_offset_reset.as_str() {
                    "earliest" => crate::consumer::AutoOffsetReset::Earliest,
                    _ => crate::consumer::AutoOffsetReset::Latest,
                })
                .enable_auto_commit(config.enable_auto_commit)
                .session_timeout(std::time::Duration::from_millis(
                    config.session_timeout_ms as u64,
                ))
                .heartbeat_interval(std::time::Duration::from_millis(
                    config.heartbeat_interval_ms as u64,
                ))
                .max_poll_interval(std::time::Duration::from_millis(
                    config.max_poll_interval_ms as u64,
                ))
                .build()
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

            let runner = ConsumerRunner::new(rust_config)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

            let mut stream = runner.stream();

            while let Some(result) = stream.next().await {
                match result {
                    Ok(msg) => {
                        let py_msg = KafkaMessage::from(msg);
                        let topic = py_msg.topic.clone();
                        // Get handler pointer from map (we need GIL to clone_ref)
                        let handler_ptr: *mut pyo3::ffi::PyObject = {
                            let guard = match handlers.lock() {
                                Ok(g) => g,
                                Err(poisoned) => poisoned.into_inner(),
                            };
                            guard.get(&topic).map(|h| h.as_ptr()).unwrap_or_else(|| default_handler.as_ptr())
                        };
                        let py_msg_clone = py_msg.clone();
                        pyo3::Python::attach(|py| {
                            // SAFETY: handler_ptr is a valid Py<PyAny> from our HashMap
                            let handler: Py<PyAny> = unsafe { Py::from_owned_ptr(py, handler_ptr) };
                            let _ = handler.call1(py, (py_msg_clone,));
                        });
                    }
                    Err(e) => {
                        tracing::error!("Consumer error: {}", e);
                        break;
                    }
                }
            }

            Ok(())
        })
        .map(|b| b.unbind())
    }

    pub fn stop(&self) {
        // TODO: signal runner shutdown
    }
}
