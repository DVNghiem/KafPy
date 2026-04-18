//! Python handler — invokes a Python callback via spawn_blocking with minimal GIL window.

use crate::dispatcher::OwnedMessage;
use crate::failure::classifier::DefaultFailureClassifier;
use crate::failure::FailureReason;
use crate::failure::FailureClassifier;
use crate::python::context::ExecutionContext;
use crate::python::execution_result::ExecutionResult;
use crate::retry::RetryPolicy;
use std::sync::Arc;

/// Execution mode for a Python handler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandlerMode {
    /// Single-message sync invocation via spawn_blocking.
    SingleSync,
    /// Single-message async invocation via pyo3-async-runtimes into_future (Phase 26).
    SingleAsync,
    /// Batch sync invocation via spawn_blocking with Vec<OwnedMessage> (Phase 25).
    BatchSync,
    /// Batch async invocation via into_future with Vec<OwnedMessage> (Phase 26).
    BatchAsync,
}

impl Default for HandlerMode {
    fn default() -> Self {
        HandlerMode::SingleSync
    }
}

/// Batch configuration for batch-mode handlers.
///
/// # Defaults
/// - max_batch_size: 1 (effectively no batching)
/// - max_batch_wait_ms: 0 (flush immediately on size)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchPolicy {
    pub max_batch_size: usize,
    pub max_batch_wait_ms: u64,
}

impl Default for BatchPolicy {
    fn default() -> Self {
        Self {
            max_batch_size: 1,
            max_batch_wait_ms: 0,
        }
    }
}

/// Wraps a Python callable stored as `Py<PyAny>` (GIL-independent, Send+Sync).
pub struct PythonHandler {
    callback: Arc<Py<PyAny>>,
    retry_policy: Option<RetryPolicy>,
    mode: HandlerMode,
    batch_policy: Option<BatchPolicy>,
}

impl PythonHandler {
    /// Wraps a Python callable stored as `Arc<Py<PyAny>>` (GIL-independent, Send+Sync).
    pub(crate) fn new(
        callback: Arc<Py<PyAny>>,
        retry_policy: Option<RetryPolicy>,
        mode: HandlerMode,
        batch_policy: Option<BatchPolicy>,
    ) -> Self {
        Self { callback, retry_policy, mode, batch_policy }
    }

    /// Returns the retry policy for this handler, if configured.
    pub fn retry_policy(&self) -> Option<&RetryPolicy> {
        self.retry_policy.as_ref()
    }

    /// Returns the execution mode for this handler.
    pub fn mode(&self) -> HandlerMode {
        self.mode.clone()
    }

    /// Returns the batch policy for this handler, if configured.
    pub fn batch_policy(&self) -> Option<&BatchPolicy> {
        self.batch_policy.as_ref()
    }

    /// Invokes the handler according to its mode.
    /// For SingleSync: calls invoke() via spawn_blocking (existing behavior).
    /// For SingleAsync/BatchSync/BatchAsync: placeholder — actual implementation in Phase 25/26.
    pub async fn invoke_mode(
        &self,
        ctx: &ExecutionContext,
        message: OwnedMessage,
    ) -> ExecutionResult {
        match self.mode() {
            HandlerMode::SingleSync => self.invoke(ctx, message).await,
            HandlerMode::SingleAsync => {
                // Phase 26: into_future bridge
                unimplemented!("SingleAsync (Phase 26)")
            }
            HandlerMode::BatchSync => {
                // Phase 25: batch invoke with Vec
                unimplemented!("BatchSync (Phase 25)")
            }
            HandlerMode::BatchAsync => {
                // Phase 26: batch async invoke
                unimplemented!("BatchAsync (Phase 26)")
            }
        }
    }

    /// Invokes the Python callable with the given message.
    ///
    /// Uses `spawn_blocking` to release the Tokio thread. GIL acquired only
    /// inside `Python::with_gil`.
    pub async fn invoke(&self, ctx: &ExecutionContext, message: OwnedMessage) -> ExecutionResult {
        let callback = Arc::clone(&self.callback);
        let topic = ctx.topic.clone();
        let partition = ctx.partition;
        let offset = ctx.offset;
        let _worker_id = ctx.worker_id;
        let key = message.key.clone();
        let payload = message.payload.clone();
        let headers = message.headers.clone();
        let timestamp = message.timestamp;

        let result = tokio::task::spawn_blocking(move || {
            Python::attach(|py| {
                let py_msg = PyDict::new(py);
                let _ = py_msg.set_item("topic", &topic);
                let _ = py_msg.set_item("partition", partition);
                let _ = py_msg.set_item("offset", offset);
                let _ = py_msg.set_item("key", key.as_deref());
                let _ = py_msg.set_item("payload", payload.as_deref());
                let ts: i64 = match timestamp {
                    MessageTimestamp::NotAvailable => 0,
                    MessageTimestamp::CreateTime(ts) => ts,
                    MessageTimestamp::LogAppendTime(ts) => ts,
                };
                let _ = py_msg.set_item("timestamp", ts);
                let _ = py_msg.set_item("headers", &headers);

                match callback.call1(py, (py_msg,)) {
                    Ok(_) => ExecutionResult::Ok,
                    Err(py_err) => {
                        let classifier = DefaultFailureClassifier;
                        let ctx_clone = ExecutionContext::new(topic, partition, offset, _worker_id);
                        let reason = classifier.classify(&py_err, &ctx_clone);
                        let exception = py_err
                            .get_type(py)
                            .name()
                            .map(|s| s.to_string())
                            .unwrap_or_else(|_| "Unknown".to_string());
                        let traceback = py_err.to_string();
                        ExecutionResult::Error {
                            reason,
                            exception,
                            traceback,
                        }
                    }
                }
            })
        })
        .await;

        match result {
            Ok(r) => r,
            Err(_) => ExecutionResult::Error {
                reason: FailureReason::Terminal(crate::failure::TerminalKind::HandlerPanic),
                exception: "Panic".to_string(),
                traceback: "spawn_blocking task panicked".to_string(),
            },
        }
    }
}

use crate::consumer::MessageTimestamp;
use pyo3::prelude::*;
use pyo3::types::PyDict;
