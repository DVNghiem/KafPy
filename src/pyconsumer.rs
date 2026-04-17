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
use crate::dispatcher::ConsumerDispatcher;
use crate::kafka_message::KafkaMessage;
use crate::python::handler::PythonHandler;
use crate::python::{DefaultExecutor, Executor};
use crate::worker_pool::WorkerPool;

use tokio_stream::StreamExt;

/// Python-callable consumer. Use `add_handler` to register a topic → callback
/// mapping, then `start()` to begin consumption.
#[pyclass]
pub struct Consumer {
    /// Effective config for the Rust core (built from the pyclass fields).
    runner: Option<ConsumerRunner>,
    config: ConsumerConfig,
    /// Stores Arc<Py<PyAny>> so handlers can be cloned for PythonHandler::new.
    handlers: Arc<Mutex<HashMap<String, Arc<Py<PyAny>>>>>,
    /// Shared shutdown token — stop() cancels this to signal workers to exit.
    shutdown_token: tokio_util::sync::CancellationToken,
}

#[pymethods]
impl Consumer {
    #[new]
    pub fn new(config: ConsumerConfig) -> Self {
        Self {
            runner: None,
            config,
            handlers: Arc::new(Mutex::new(HashMap::new())),
            shutdown_token: tokio_util::sync::CancellationToken::new(),
        }
    }

    /// Registers a Python callback to handle messages from `topic`.
    /// The callback receives exactly one argument: a `KafkaMessage`.
    /// Note: add_handler ONLY stores the callback. All dispatcher/WorkerPool wiring
    /// happens in start().
    pub fn add_handler(&mut self, topic: String, callback: Bound<'_, PyAny>) {
        if let Ok(mut handlers) = self.handlers.lock() {
            handlers.insert(topic, Arc::new(callback.unbind()));
        }
    }

    /// Starts the consumer and runs indefinitely, dispatching messages to
    /// registered Python handlers via WorkerPool.
    pub fn start(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let config = self.config.clone();
        let handlers = Arc::clone(&self.handlers);
        let shutdown_token = self.shutdown_token.clone();

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

            let default_retry_policy = rust_config.default_retry_policy.clone();
            let runner = ConsumerRunner::new(rust_config.clone())
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

            // BRIDGE-01 + D-02: Create OffsetTracker and wire ConsumerRunner before pool
            let runner_arc: std::sync::Arc<ConsumerRunner> = std::sync::Arc::new(runner);
            let offset_tracker = std::sync::Arc::new(crate::coordinator::OffsetTracker::new());
            offset_tracker.set_runner(std::sync::Arc::clone(&runner_arc));

            // Create ConsumerDispatcher (owned, not stored in Consumer struct)
            let dispatcher = ConsumerDispatcher::new((*runner_arc).clone());

            // Collect receivers from all registered handlers
            let receivers: Vec<_> = {
                let handlers_guard = handlers.lock().unwrap();
                handlers_guard
                    .keys()
                    .map(|topic| dispatcher.register_handler(topic.clone(), 100, None))
                    .collect()
            };

            // Build PythonHandler from the registered callbacks
            let handler_arc: Arc<PythonHandler> = {
                let handlers_guard = handlers.lock().unwrap();
                let first_handler = handlers_guard
                    .values()
                    .next()
                    .expect("at least one handler must be registered");
                Arc::new(PythonHandler::new(first_handler.clone(), Some(default_retry_policy)))
            };

            let executor_arc: Arc<dyn Executor> = Arc::new(DefaultExecutor::default());
            let queue_manager_arc = dispatcher.queue_manager();
            let retry_coordinator = std::sync::Arc::new(crate::coordinator::RetryCoordinator::new(&rust_config));

            let n_workers = 4; // EXEC-08: configurable

            let pool = WorkerPool::new(
                n_workers,
                receivers,
                handler_arc,
                executor_arc,
                queue_manager_arc.clone(),
                offset_tracker.clone(),
                retry_coordinator,
                shutdown_token,
            );

            // BRIDGE-01 + D-02: Create OffsetCommitter and spawn as Tokio task
            let committer = crate::coordinator::OffsetCommitter::new(
                std::sync::Arc::clone(&runner_arc),
                std::sync::Arc::clone(&offset_tracker),
                crate::coordinator::CommitConfig::default(),
            );
            let (tx, rx) = tokio::sync::watch::channel(crate::coordinator::TopicPartition::new("", 0));
            let committer_handle = tokio::spawn(async move {
                committer.run(rx).await;
            });
            drop(tx);

            // Run dispatcher and pool concurrently
            let dispatcher_handle = tokio::spawn(async move {
                dispatcher
                    .run(&crate::dispatcher::DefaultBackpressurePolicy)
                    .await;
            });

            pool.run().await;

            // Keep handles alive until pool completes
            let _ = dispatcher_handle;
            let _ = committer_handle;

            Ok(())
        })
        .map(|b| b.unbind())
    }

    pub fn stop(&self) {
        self.shutdown_token.cancel();
    }
}
