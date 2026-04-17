//! Python handler — invokes a Python callback via spawn_blocking with minimal GIL window.

use crate::dispatcher::OwnedMessage;
use crate::failure::classifier::DefaultFailureClassifier;
use crate::failure::FailureReason;
use crate::failure::FailureClassifier;
use crate::python::context::ExecutionContext;
use crate::python::execution_result::ExecutionResult;
use std::sync::Arc;

/// Wraps a Python callable stored as `Py<PyAny>` (GIL-independent, Send+Sync).
pub struct PythonHandler {
    callback: Arc<Py<PyAny>>,
}

impl PythonHandler {
    /// Wraps a Python callable stored as `Arc<Py<PyAny>>` (GIL-independent, Send+Sync).
    pub(crate) fn new(callback: Arc<Py<PyAny>>) -> Self {
        Self { callback }
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
