//! Python-facing consumer that delegates to the pure-Rust consumer core.
//!
//! Bridges the PyO3 boundary: accepts Python callbacks, delegates message
//! ingestion to `ConsumerRunner`, converts `OwnedMessage` → `KafkaMessage`,
//! then dispatches to the Python handler.

use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::config::ConsumerConfig;
use crate::consumer::{ConsumerConfigBuilder, ConsumerRunner};
use crate::dispatcher::ConsumerDispatcher;
use crate::dlq::{DefaultDlqRouter, DlqRouter, SharedDlqProducer};
use crate::python::handler::PythonHandler;
use crate::python::{DefaultExecutor, Executor};
use crate::worker_pool::WorkerPool;

/// Python-callable consumer. Use `add_handler` to register a topic → callback
/// mapping, then `start()` to begin consumption.
#[pyclass]
pub struct Consumer {
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
                Arc::new(PythonHandler::new(
                    first_handler.clone(),
                    Some(default_retry_policy),
                    crate::python::handler::HandlerMode::SingleSync,
                    None,
                ))
            };

            let executor_arc: Arc<dyn Executor> = Arc::new(DefaultExecutor::default());
            let queue_manager_arc = dispatcher.queue_manager();
            let retry_coordinator =
                std::sync::Arc::new(crate::coordinator::RetryCoordinator::new(&rust_config));

            // Create DLQ producer and router from config
            let dlq_producer = std::sync::Arc::new(
                SharedDlqProducer::new(&rust_config).expect("Failed to create DLQ producer"),
            );
            let dlq_router: std::sync::Arc<dyn DlqRouter> =
                std::sync::Arc::new(DefaultDlqRouter::new(rust_config.dlq_topic_prefix.clone()));

            let n_workers = 4; // EXEC-08: configurable

            let pool = WorkerPool::new(
                n_workers,
                receivers,
                handler_arc,
                executor_arc,
                queue_manager_arc.clone(),
                offset_tracker.clone(),
                retry_coordinator,
                dlq_producer,
                dlq_router,
                shutdown_token.clone(),
            );

            // OBS-31: Spawn RuntimeSnapshotTask for introspection (global singleton)
            let _snapshot_task = crate::observability::RuntimeSnapshotTask::spawn(
                Some(queue_manager_arc.clone()),
                Some(offset_tracker.clone()),
                Some(std::sync::Arc::clone(&pool.worker_pool_state)),
                std::time::Duration::from_secs(10),
            );

            // BRIDGE-01 + D-02: Create OffsetCommitter and spawn as Tokio task
            let committer = crate::coordinator::OffsetCommitter::new(
                std::sync::Arc::clone(&runner_arc),
                std::sync::Arc::clone(&offset_tracker),
                crate::coordinator::CommitConfig::default(),
            );
            let (tx, rx) =
                tokio::sync::watch::channel(crate::coordinator::TopicPartition::new("", 0));
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

    /// Returns the current runtime status as a Python dict.
    ///
    /// Contains:
    /// - worker_states: dict of worker_id -> state dict
    /// - queue_depths: dict of handler_id -> {queue_depth, inflight}
    /// - accumulator_info: dict of handler_id -> {total_messages, partitions: {partition -> {message_count, has_deadline, deadline_ms_remaining}}}
    /// - consumer_lag_summary: {total_lag, per_topic: {topic -> {total_lag, partitions: {partition -> {consumer_lag, committed_offset}}}}
    ///
    /// This is a convenience method that calls get_runtime_snapshot().
    pub fn status(&self) -> PyResult<Py<PyAny>> {
        get_runtime_snapshot()
    }
}

// ─── Runtime Snapshot FFI ─────────────────────────────────────────────────────

use crate::observability::runtime_snapshot::{
    get_current_snapshot, get_callback_registry, RuntimeSnapshot,
    WorkerState as ObsWorkerState,
};

/// Returns the current runtime snapshot as a Python dict.
///
/// Contains:
/// - timestamp: Unix timestamp of snapshot
/// - worker_states: dict of worker_id -> state dict
/// - queue_depths: dict of handler_id -> {queue_depth, inflight}
/// - accumulator_info: dict of handler_id -> {total_messages, partitions: {partition -> {message_count, has_deadline, deadline_ms_remaining}}}
/// - consumer_lag_summary: {total_lag, per_topic: {topic -> {total_lag, partitions: {partition -> {consumer_lag, committed_offset}}}}
///
/// This function is zero-cost when not called — no atomic updates on hot path.
#[pyfunction]
pub fn get_runtime_snapshot() -> PyResult<Py<PyAny>> {
    let snapshot = get_current_snapshot();
    snapshot_to_pydict(snapshot)
}

/// Register a Python callable to be invoked on every runtime snapshot update.
///
/// The callback receives a single dict argument (same structure as get_runtime_snapshot()).
///
/// This is opt-in — no callbacks are invoked unless one is registered.
#[pyfunction]
pub fn register_status_callback(callback: Py<PyAny>) -> PyResult<()> {
    if let Some(registry) = get_callback_registry() {
        registry.register(callback);
    }
    Ok(())
}

fn snapshot_to_pydict(snapshot: RuntimeSnapshot) -> PyResult<Py<PyAny>> {
    Python::attach(|py| {
        let dict = PyDict::new(py);

        // timestamp
        let timestamp_secs = snapshot
            .timestamp
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        dict.set_item("timestamp", timestamp_secs)?;

        // worker_states
        let worker_states = PyDict::new(py);
        for (id, state) in &snapshot.worker_states {
            let state_dict = match state {
                ObsWorkerState::Idle => {
                    let d = PyDict::new(py);
                    d.set_item("status", "idle")?;
                    d
                }
                ObsWorkerState::Active {
                    handler_id,
                    topic,
                    partition,
                    offset,
                } => {
                    let d = PyDict::new(py);
                    d.set_item("status", "active")?;
                    d.set_item("handler_id", handler_id)?;
                    d.set_item("topic", topic)?;
                    d.set_item("partition", partition)?;
                    d.set_item("offset", offset)?;
                    d
                }
                ObsWorkerState::Busy { handler_id } => {
                    let d = PyDict::new(py);
                    d.set_item("status", "busy")?;
                    d.set_item("handler_id", handler_id)?;
                    d
                }
            };
            worker_states.set_item(id, state_dict)?;
        }
        dict.set_item("worker_states", worker_states)?;

        // queue_depths
        let queue_depths = PyDict::new(py);
        for (handler_id, info) in &snapshot.queue_depths {
            let info_dict = PyDict::new(py);
            info_dict.set_item("queue_depth", info.queue_depth)?;
            info_dict.set_item("inflight", info.inflight)?;
            queue_depths.set_item(handler_id, info_dict)?;
        }
        dict.set_item("queue_depths", queue_depths)?;

        // accumulator_info
        let accumulator_info = PyDict::new(py);
        for (handler_id, info) in &snapshot.accumulator_info {
            let info_dict = PyDict::new(py);
            info_dict.set_item("total_messages", info.total_messages)?;
            let partitions = PyDict::new(py);
            for (partition, pinfo) in &info.partitions {
                let p_dict = PyDict::new(py);
                p_dict.set_item("message_count", pinfo.message_count)?;
                p_dict.set_item("has_deadline", pinfo.has_deadline)?;
                p_dict.set_item(
                    "deadline_ms_remaining",
                    pinfo.deadline_ms_remaining.unwrap_or(-1),
                )?;
                partitions.set_item(partition, p_dict)?;
            }
            info_dict.set_item("partitions", partitions)?;
            accumulator_info.set_item(handler_id, info_dict)?;
        }
        dict.set_item("accumulator_info", accumulator_info)?;

        // consumer_lag_summary
        let lag_dict = PyDict::new(py);
        lag_dict.set_item("total_lag", snapshot.consumer_lag_summary.total_lag)?;
        let per_topic = PyDict::new(py);
        for (topic, tinfo) in &snapshot.consumer_lag_summary.per_topic {
            let t_dict = PyDict::new(py);
            t_dict.set_item("total_lag", tinfo.total_lag)?;
            let part_dict = PyDict::new(py);
            for (partition, pinfo) in &tinfo.partitions {
                let p_dict = PyDict::new(py);
                p_dict.set_item("consumer_lag", pinfo.consumer_lag)?;
                p_dict.set_item("committed_offset", pinfo.committed_offset)?;
                part_dict.set_item(partition, p_dict)?;
            }
            t_dict.set_item("partitions", part_dict)?;
            per_topic.set_item(topic, t_dict)?;
        }
        lag_dict.set_item("per_topic", per_topic)?;
        dict.set_item("consumer_lag_summary", lag_dict)?;

        Ok(dict.into_any().unbind())
    })
}
