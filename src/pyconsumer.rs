//! Python-facing consumer that delegates to the pure-Rust consumer core.
//!
//! Bridges the PyO3 boundary: accepts Python callbacks, delegates message
//! ingestion to `RuntimeBuilder`, which assembles the full runtime.

use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::config::ConsumerConfig;
use crate::python::handler::HandlerMode;
use crate::runtime::RuntimeBuilder;

/// Per-handler metadata stored alongside the callback.
#[derive(Debug, Clone)]
pub struct HandlerMetadata {
    pub callback: Arc<Py<PyAny>>,
    pub mode: HandlerMode,
    pub batch_max_size: Option<usize>,
    pub batch_max_wait_ms: Option<u64>,
    pub timeout_ms: Option<u64>,
    /// Maximum concurrent executions for this handler. None = use default.
    pub concurrency: Option<usize>,
}

/// Python-callable consumer. Use `add_handler` to register a topic → callback
/// mapping, then `start()` to begin consumption.
#[pyclass(name = "Consumer")]
pub struct PyConsumer {
    config: ConsumerConfig,
    /// Stores handler metadata per topic.
    handlers: Arc<Mutex<HashMap<String, HandlerMetadata>>>,
    /// Shared shutdown token — stop() cancels this to signal workers to exit.
    shutdown_token: tokio_util::sync::CancellationToken,
}

#[pymethods]
impl PyConsumer {
    #[new]
    pub fn new(config: ConsumerConfig) -> Self {
        Self {
            config,
            handlers: Arc::new(Mutex::new(HashMap::new())),
            shutdown_token: tokio_util::sync::CancellationToken::new(),
        }
    }

    /// Registers a Python callback to handle messages from `topic`.
    /// Accepts optional handler mode, batch config, and per-handler timeout.
    ///
    /// Args:
    ///     topic: Kafka topic to subscribe to.
    ///     callback: Python callable invoked for each message.
    ///     mode: Optional handler mode — "sync", "async", "batch_sync", "batch_async".
    ///           If None, defaults to "sync".
    ///     batch_max_size: Max messages per batch (batch modes only). Defaults to 100.
    ///     batch_max_wait_ms: Max wait time per batch in ms (batch modes only). Defaults to 1000.
    ///     timeout_ms: Per-handler execution timeout in ms. None uses ConsumerConfig.handler_timeout_ms.
    ///     concurrency: Maximum concurrent executions for this handler. None = no limit.
    #[pyo3(signature = (topic, callback, mode=None, batch_max_size=None, batch_max_wait_ms=None, timeout_ms=None, concurrency=None))]
    #[allow(clippy::too_many_arguments)]
    pub fn add_handler(
        &mut self,
        topic: String,
        callback: Bound<'_, PyAny>,
        mode: Option<String>,
        batch_max_size: Option<usize>,
        batch_max_wait_ms: Option<u64>,
        timeout_ms: Option<u64>,
        concurrency: Option<usize>,
    ) {
        let mode = HandlerMode::from_opt_str(mode.as_deref());
        let meta = HandlerMetadata {
            callback: Arc::new(callback.unbind()),
            mode,
            batch_max_size,
            batch_max_wait_ms,
            timeout_ms,
            concurrency,
        };
        if let Ok(mut handlers) = self.handlers.lock() {
            handlers.insert(topic, meta);
        }
    }

    /// Starts the consumer and runs indefinitely, dispatching messages to
    /// registered Python handlers via WorkerPool.
    pub fn start(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let config = self.config.clone();
        let handlers = Arc::clone(&self.handlers);
        let shutdown_token = self.shutdown_token.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let builder = RuntimeBuilder::new(config, handlers, shutdown_token);
            let runtime = builder
                .build()
                .await
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
            runtime.run_with_sigterm().await;
            Ok(())
        })
        .map(|b| b.unbind())
    }

    pub fn stop(&self) {
        self.shutdown_token.cancel();
    }

    /// Enter the context manager — returns self.
    fn __enter__(slf: pyo3::PyRefMut<'_, Self>) -> pyo3::PyResult<pyo3::PyRefMut<'_, Self>> {
        Ok(slf)
    }

    /// Exit the context manager — calls stop() to trigger graceful shutdown.
    fn __exit__(
        &mut self,
        _exc_type: &pyo3::Bound<'_, pyo3::PyAny>,
        _exc_val: &pyo3::Bound<'_, pyo3::PyAny>,
        _traceback: &pyo3::Bound<'_, pyo3::PyAny>,
    ) -> pyo3::PyResult<bool> {
        self.stop();
        Ok(false) // don't suppress exceptions
    }

    /// Returns the current runtime status as a Python dict.
    pub fn status(&self) -> PyResult<Py<PyAny>> {
        get_runtime_snapshot()
    }
}

// ─── Runtime Snapshot FFI ─────────────────────────────────────────────────────

use crate::observability::runtime_snapshot::{
    get_callback_registry, get_current_snapshot, RuntimeSnapshot, WorkerState as ObsWorkerState,
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
