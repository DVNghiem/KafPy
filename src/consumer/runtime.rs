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
use crate::worker_pool::fan_out::FanOutConfig;

/// FanOutHandler is a PythonHandler with FanOutConfig attached.
type FanOutHandler = crate::python::handler::PythonHandler;

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
    /// Middleware class objects to be executed around handler invocation.
    /// Stored as Arc-wrapped Py objects for Clone derivability and GIL-safe access.
    /// Chain is built at invocation time when the metrics sink is available.
    pub middleware: Option<Vec<Arc<Py<PyAny>>>>,
    /// Fan-out configuration for this handler. None means not a fan-out sink.
    pub fan_out_config: Option<Arc<FanOutConfig>>,
}

/// Builder for `HandlerMetadata` to reduce constructor argument count.
#[derive(Debug)]
pub struct HandlerMetadataBuilder {
    callback: Arc<Py<PyAny>>,
    mode: HandlerMode,
    batch_max_size: Option<usize>,
    batch_max_wait_ms: Option<u64>,
    timeout_ms: Option<u64>,
    concurrency: Option<usize>,
    middleware: Option<Vec<Arc<Py<PyAny>>>>,
    fan_out_config: Option<Arc<FanOutConfig>>,
}

impl HandlerMetadataBuilder {
    /// Creates a new builder with the required callback.
    pub fn new(callback: Arc<Py<PyAny>>) -> Self {
        Self {
            callback,
            mode: HandlerMode::default(),
            batch_max_size: None,
            batch_max_wait_ms: None,
            timeout_ms: None,
            concurrency: None,
            middleware: None,
            fan_out_config: None,
        }
    }

    /// Sets the handler mode.
    pub fn mode(mut self, mode: HandlerMode) -> Self {
        self.mode = mode;
        self
    }

    /// Sets the batch max size.
    pub fn batch_max_size(mut self, batch_max_size: usize) -> Self {
        self.batch_max_size = Some(batch_max_size);
        self
    }

    /// Sets the batch max wait in milliseconds.
    pub fn batch_max_wait_ms(mut self, batch_max_wait_ms: u64) -> Self {
        self.batch_max_wait_ms = Some(batch_max_wait_ms);
        self
    }

    /// Sets the timeout in milliseconds.
    pub fn timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = Some(timeout_ms);
        self
    }

    /// Sets the concurrency limit.
    pub fn concurrency(mut self, concurrency: usize) -> Self {
        self.concurrency = Some(concurrency);
        self
    }

    /// Sets the middleware.
    pub fn middleware(mut self, middleware: Vec<Arc<Py<PyAny>>>) -> Self {
        self.middleware = Some(middleware);
        self
    }

    /// Sets the fan-out config.
    pub fn fan_out_config(mut self, fan_out_config: Arc<FanOutConfig>) -> Self {
        self.fan_out_config = Some(fan_out_config);
        self
    }

    /// Builds the `HandlerMetadata`.
    pub fn build(self) -> HandlerMetadata {
        HandlerMetadata {
            callback: self.callback,
            mode: self.mode,
            batch_max_size: self.batch_max_size,
            batch_max_wait_ms: self.batch_max_wait_ms,
            timeout_ms: self.timeout_ms,
            concurrency: self.concurrency,
            middleware: self.middleware,
            fan_out_config: self.fan_out_config,
        }
    }
}

impl HandlerMetadata {
    /// Creates a HandlerMetadata with middleware objects wrapped in Arc.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        callback: Arc<Py<PyAny>>,
        mode: HandlerMode,
        batch_max_size: Option<usize>,
        batch_max_wait_ms: Option<u64>,
        timeout_ms: Option<u64>,
        concurrency: Option<usize>,
        middleware: Option<Vec<Py<PyAny>>>,
        fan_out_config: Option<Arc<FanOutConfig>>,
    ) -> Self {
        Self {
            callback,
            mode,
            batch_max_size,
            batch_max_wait_ms,
            timeout_ms,
            concurrency,
            middleware: middleware.map(|v| v.into_iter().map(Arc::new).collect()),
            fan_out_config,
        }
    }
}

/// Fan-out registration result returned to Python.
///
/// Holds the group_name and fan_out_id for later correlation.
#[pyclass]
pub struct FanOutRegistration {
    #[pyo3(get)]
    pub group_name: String,
    #[pyo3(get)]
    pub fan_out_id: u64,
    #[pyo3(get)]
    pub sink_topics: Vec<String>,
}

/// Python-callable consumer. Use `add_handler` to register a topic → callback
/// mapping, then `start()` to begin consumption.
#[pyclass(name = "Consumer")]
pub struct PyConsumer {
    config: ConsumerConfig,
    /// Stores handler metadata per topic.
    handlers: Arc<Mutex<HashMap<String, HandlerMetadata>>>,
    /// Stores PythonHandler for fan-out sinks (topic -> handler with FanOutConfig attached).
    /// Separate from handlers map since these need FanOutConfig set before RuntimeBuilder runs.
    fan_out_handlers: Arc<Mutex<HashMap<String, Arc<FanOutHandler>>>>,
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
            fan_out_handlers: Arc::new(Mutex::new(HashMap::new())),
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
    ///           If None, uses KAFPY_HANDLER_MODE_DEFAULT when set, otherwise "sync".
    ///     batch_max_size: Max messages per batch (batch modes only). Defaults to 100.
    ///     batch_max_wait_ms: Max wait time per batch in ms (batch modes only). Defaults to 1000.
    ///     timeout_ms: Per-handler execution timeout in ms. None uses ConsumerConfig.handler_timeout_ms.
    ///     concurrency: Maximum concurrent executions for this handler.
    ///                  None = global default limit (KAFPY_HANDLER_CONCURRENCY_DEFAULT or 4).
    #[pyo3(signature = (topic, callback, mode=None, batch_max_size=None, batch_max_wait_ms=None, timeout_ms=None, concurrency=None, middleware=None))]
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
        middleware: Option<Vec<Py<PyAny>>>,
    ) {
        let default_mode = std::env::var("KAFPY_HANDLER_MODE_DEFAULT").ok();
        let resolved_mode = mode.or(default_mode);
        let mode = HandlerMode::from_opt_str(resolved_mode.as_deref());
        let meta = HandlerMetadata::new(
            Arc::new(callback.unbind()),
            mode,
            batch_max_size,
            batch_max_wait_ms,
            timeout_ms,
            concurrency,
            middleware,
            None, // no fan-out config
        );
        if let Ok(mut handlers) = self.handlers.lock() {
            handlers.insert(topic, meta);
        }
    }

    /// Starts the consumer and runs indefinitely, dispatching messages to
    /// registered Python handlers via WorkerPool.
    pub fn start(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let config = self.config.clone();
        let handlers = Arc::clone(&self.handlers);
        let fan_out_handlers = self.get_fan_out_handlers();
        let shutdown_token = self.shutdown_token.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let builder = RuntimeBuilder::new(config, handlers, fan_out_handlers, shutdown_token);
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

    /// Registers a fan-out group: a handler callable that fans out to multiple sink topics.
    ///
    /// Args:
    ///     group_name: Identifier for this fan-out group.
    ///     sink_topics: List of sink topic names to fan out to.
    ///     callback: Python callable invoked for each sink topic.
    ///     max_fan_out: Maximum concurrent sink branches (default 4, max 64).
    ///     timeout_ms: Per-branch execution timeout in milliseconds.
    ///
    /// Returns a `FanOutRegistration` with group_name, fan_out_id, and sink_topics.
    #[pyo3(signature = (group_name, sink_topics, callback, max_fan_out=None, timeout_ms=None))]
    pub fn register_fanout(
        &mut self,
        group_name: String,
        sink_topics: Vec<String>,
        callback: Bound<'_, PyAny>,
        max_fan_out: Option<u8>,
        timeout_ms: Option<u64>,
    ) -> PyResult<FanOutRegistration> {
        use crate::python::fan_out_bridge::FanOutBuilderRust;
        use std::sync::Arc;

        let mode = HandlerMode::from_opt_str(None); // sync by default
        let builder = FanOutBuilderRust::new(
            group_name.clone(),
            sink_topics.clone(),
            Arc::new(callback.unbind()),
            mode,
            timeout_ms,
        );
        let max = max_fan_out.unwrap_or(4).min(64);
        let (returned_group_name, fan_out_id) = builder.register_into_consumer(self, max);
        Ok(FanOutRegistration {
            group_name: returned_group_name,
            fan_out_id,
            sink_topics,
        })
    }

}

// ─── Internal methods (not PyO3 wrapped) ───────────────────────────────────────

impl PyConsumer {
    /// Internal method used by FanOutBuilderRust to register a sink handler
    /// with an attached FanOutConfig.
    #[allow(unsafe_code)]
    pub fn add_handler_with_fan_out(
        &mut self,
        topic: String,
        handler: std::sync::Arc<FanOutHandler>,
        fan_out_config: FanOutConfig,
    ) {
        use crate::python::handler::HandlerMode;
        use std::sync::Arc;

        // For fan-out sinks, the actual handler with FanOutConfig is stored in
        // fan_out_handlers. HandlerMetadata.callback is set to PyNone as a marker
        // so RuntimeBuilder knows this topic needs special handling.
        let py_none = unsafe { Python::assume_attached() }.None();
        let meta = HandlerMetadata::new(
            Arc::new(py_none),
            HandlerMode::SingleSync,
            None,
            None,
            None,
            None,
            None,
            Some(Arc::new(fan_out_config)),
        );
        // Insert metadata into handlers map (RuntimeBuilder will look up fan_out_config)
        if let Ok(mut handlers) = self.handlers.lock() {
            handlers.insert(topic.clone(), meta);
        }
        // Store the actual handler with FanOutConfig attached in fan_out_handlers map
        if let Ok(mut fan_handlers) = self.fan_out_handlers.lock() {
            fan_handlers.insert(topic, handler);
        }
    }

    /// Exposes the fan-out handlers map for RuntimeBuilder to consume.
    pub fn get_fan_out_handlers(
        &self,
    ) -> std::collections::HashMap<String, std::sync::Arc<FanOutHandler>> {
        self.fan_out_handlers
            .lock()
            .expect("fan_out_handlers poisoned")
            .iter()
            .map(|(k, v)| (k.clone(), Arc::clone(v)))
            .collect()
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
