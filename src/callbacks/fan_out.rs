//! Fan-out bridge — assembles fan-out config and registers sinks with PyConsumer.
//!
//! Called by `PyConsumer::register_fanout` to attach a fan-out group to one or more
//! sink topics, all sharing the same Python handler callable.

use crate::pyconsumer::PyConsumer;
use crate::python::handler::{HandlerMode, PythonHandler};
use crate::worker_pool::fan_out::FanOutConfig;
use std::sync::Arc;
use std::time::Duration;
use std::sync::atomic::{AtomicU64, Ordering};

/// Global counter for generating unique fan_out_ids.
static FAN_OUT_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Python-facing fan-out builder.
///
/// Constructed from Python via `FanOutBuilderRust::new(...)`, then
/// `.max_fan_out(n)` to configure the degree, then `.register()` to
/// attach all sink handlers to the PyConsumer.
#[derive(Clone)]
pub struct FanOutBuilderRust {
    group_name: String,
    sink_topics: Vec<String>,
    callback: Arc<pyo3::Py<pyo3::PyAny>>,
    mode: HandlerMode,
    timeout_ms: Option<u64>,
    max_fan_out: Option<u8>,
}

impl FanOutBuilderRust {
    /// Create a new fan-out builder.
    ///
    /// - `group_name` — identifier for this fan-out group
    /// - `sink_topics` — list of sink topic names to fan out to
    /// - `callback` — the Python callable invoked for each sink topic
    /// - `mode` — handler execution mode (derived from callback inspection)
    /// - `timeout_ms` — per-branch execution timeout in milliseconds
    pub fn new(
        group_name: String,
        sink_topics: Vec<String>,
        callback: Arc<pyo3::Py<pyo3::PyAny>>,
        mode: HandlerMode,
        timeout_ms: Option<u64>,
    ) -> Self {
        Self {
            group_name,
            sink_topics,
            callback,
            mode,
            timeout_ms,
            max_fan_out: None,
        }
    }

    /// Set the maximum fan-out degree.
    ///
    /// Capped at 64 regardless of input.
    pub fn max_fan_out(mut self, n: u8) -> Self {
        self.max_fan_out = Some(n.min(64));
        self
    }

    /// Register the fan-out group with the PyConsumer.
    ///
    /// Attaches each sink topic as a handler and returns the fan_out_id.
    /// Returns `(group_name, fan_out_id)` for constructing `FanOutRegistration`.
    pub fn register_into_consumer(self, py_consumer: &mut PyConsumer, max: u8) -> (String, u64) {
        let fan_out_id = FAN_OUT_COUNTER.fetch_add(1, Ordering::SeqCst);

        for sink_topic in &self.sink_topics {
            // Build a PythonHandler for this sink topic
            let timeout = self.timeout_ms.map(Duration::from_millis);
            let handler = PythonHandler::new(
                Arc::clone(&self.callback),
                None, // retry_policy — use default from config if needed
                self.mode.clone(),
                None, // batch_policy
                timeout,
                format!("{}[{}]", self.group_name, sink_topic),
                None, // rayon_pool
                None, // middleware
            );

            // Attach fan-out config to the handler so dispatch knows it's a sink
            let fan_out_config = FanOutConfig {
                sinks: Vec::new(), // sinks are registered separately per primary message
                max_fan_out: max,
                slot_manager: None,
            };

            // Register with PyConsumer's handler map directly
            // This bypasses add_handler to attach fan_out config
            py_consumer.add_handler_with_fan_out(
                sink_topic.clone(),
                Arc::new(handler),
                fan_out_config,
            );
        }

        (self.group_name, fan_out_id)
    }
}

/// Extension trait to allow PyConsumer::add_handler to accept fan-out config.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum HandlerModeRust {
    #[default]
    SingleSync,
    SingleAsync,
    BatchSync,
    BatchAsync,
    StreamingAsync,
}

impl HandlerModeRust {
    pub fn from_opt_str(s: Option<&str>) -> Self {
        match s {
            Some("async") => HandlerModeRust::SingleAsync,
            Some("batch_sync") => HandlerModeRust::BatchSync,
            Some("batch_async") => HandlerModeRust::BatchAsync,
            Some("streaming_async") => HandlerModeRust::StreamingAsync,
            _ => HandlerModeRust::SingleSync,
        }
    }
}

impl From<HandlerModeRust> for HandlerMode {
    fn from(_: HandlerModeRust) -> Self {
        HandlerMode::SingleSync
    }
}
