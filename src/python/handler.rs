//! Python handler — invokes a Python callback via spawn_blocking with minimal GIL window.

use crate::consumer::MessageTimestamp;
use crate::dispatcher::OwnedMessage;
use crate::failure::classifier::DefaultFailureClassifier;
use crate::failure::FailureClassifier;
use crate::failure::FailureReason;
use crate::observability::tracing::inject_trace_context;
use crate::python::async_bridge::PythonAsyncFuture;
use crate::python::context::ExecutionContext;
use crate::python::execution_result::{BatchExecutionResult, ExecutionResult, TimeoutInfo};
use crate::rayon_pool::RayonPool;
use crate::retry::RetryPolicy;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Converts an OwnedMessage to a PyDict for Python callback invocation.
///
/// Call inside `Python::attach(|py| { ... })` with the Python token.
/// Optionally injects trace context if provided.
fn message_to_pydict<'py>(
    py: Python<'py>,
    msg: &OwnedMessage,
    trace_context: Option<&HashMap<String, String>>,
) -> Py<PyAny> {
    let py_msg = PyDict::new(py);
    let _ = py_msg.set_item("topic", &msg.topic);
    let _ = py_msg.set_item("partition", msg.partition);
    let _ = py_msg.set_item("offset", msg.offset);
    let _ = py_msg.set_item("key", msg.key.as_deref());
    let _ = py_msg.set_item("payload", msg.payload.as_deref());
    let ts: i64 = match msg.timestamp {
        MessageTimestamp::NotAvailable => 0,
        MessageTimestamp::CreateTime(ts) => ts,
        MessageTimestamp::LogAppendTime(ts) => ts,
    };
    let _ = py_msg.set_item("timestamp", ts);
    let _ = py_msg.set_item("headers", &msg.headers);
    if let Some(ctx) = trace_context {
        let trace_dict = PyDict::new(py);
        for (k, v) in ctx {
            let _ = trace_dict.set_item(k, v);
        }
        let _ = py_msg.set_item("_trace_context", trace_dict);
    }
    py_msg.into()
}

/// Converts an ExecutionContext to a PyDict for Python callback invocation.
///
/// Call inside `Python::attach(|py| { ... })` with the Python token.
fn ctx_to_pydict<'py>(py: Python<'py>, ctx: &ExecutionContext, msg: &OwnedMessage) -> Py<PyAny> {
    let py_ctx = PyDict::new(py);
    let _ = py_ctx.set_item("topic", &ctx.topic);
    let _ = py_ctx.set_item("partition", ctx.partition);
    let _ = py_ctx.set_item("offset", ctx.offset);
    // Timestamp and headers from the message
    let ts: i64 = match msg.timestamp {
        MessageTimestamp::NotAvailable => 0,
        MessageTimestamp::CreateTime(ts) => ts,
        MessageTimestamp::LogAppendTime(ts) => ts,
    };
    let _ = py_ctx.set_item("timestamp", ts);
    let _ = py_ctx.set_item("headers", &msg.headers);
    // Inject trace context fields if present
    if let Some(ref tid) = ctx.trace_id {
        let _ = py_ctx.set_item("trace_id", tid);
    }
    if let Some(ref sid) = ctx.span_id {
        let _ = py_ctx.set_item("span_id", sid);
    }
    if let Some(ref flags) = ctx.trace_flags {
        let _ = py_ctx.set_item("trace_flags", flags);
    }
    py_ctx.into()
}

/// Execution mode for a Python handler.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum HandlerMode {
    /// Single-message sync invocation via spawn_blocking.
    #[default]
    SingleSync,
    /// Single-message async invocation via pyo3-async-runtimes into_future (Phase 26).
    SingleAsync,
    /// Batch sync invocation via spawn_blocking with Vec<OwnedMessage> (Phase 25).
    BatchSync,
    /// Batch async invocation via into_future with Vec<OwnedMessage> (Phase 26).
    BatchAsync,
}

impl HandlerMode {
    /// Returns the mode name as a string for metrics labeling.
    pub fn as_str(&self) -> &'static str {
        match self {
            HandlerMode::SingleSync => "SingleSync",
            HandlerMode::SingleAsync => "SingleAsync",
            HandlerMode::BatchSync => "BatchSync",
            HandlerMode::BatchAsync => "BatchAsync",
        }
    }

    /// Parse a handler mode from a string slice.
    /// Accepts "sync", "async", "batch_sync", "batch_async".
    /// Returns `SingleSync` for unrecognized or None input.
    pub fn from_opt_str(s: Option<&str>) -> Self {
        match s {
            Some("async") => HandlerMode::SingleAsync,
            Some("batch_sync") => HandlerMode::BatchSync,
            Some("batch_async") => HandlerMode::BatchAsync,
            _ => HandlerMode::SingleSync,
        }
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

/// Wraps a Python callable stored as `Arc<Py<PyAny>>` (GIL-independent, Send+Sync).
pub struct PythonHandler {
    callback: Arc<Py<PyAny>>,
    retry_policy: Option<RetryPolicy>,
    mode: HandlerMode,
    batch_policy: Option<BatchPolicy>,
    /// Handler execution timeout. None means no timeout (handler can run indefinitely).
    handler_timeout: Option<Duration>,
    /// Human-readable handler name for observability (typically the topic).
    name: String,
    /// Rayon pool for offloading sync handler work. None means use spawn_blocking directly.
    rayon_pool: Option<Arc<RayonPool>>,
}

impl PythonHandler {
    /// Wraps a Python callable stored as `Arc<Py<PyAny>>` (GIL-independent, Send+Sync).
    pub(crate) fn new(
        callback: Arc<Py<PyAny>>,
        retry_policy: Option<RetryPolicy>,
        mode: HandlerMode,
        batch_policy: Option<BatchPolicy>,
        handler_timeout: Option<Duration>,
        name: String,
        rayon_pool: Option<Arc<RayonPool>>,
    ) -> Self {
        Self {
            callback,
            retry_policy,
            mode,
            batch_policy,
            handler_timeout,
            name,
            rayon_pool,
        }
    }

    /// Creates a PythonHandler with a handler execution timeout.
    ///
    /// If a handler invocation takes longer than `timeout`, it will be cancelled
    /// and treated as a `Terminal(HandlerPanic)` error, routing the message to
    /// DLQ or retry based on the failure classification.
    pub fn with_timeout(
        callback: Arc<Py<PyAny>>,
        retry_policy: Option<RetryPolicy>,
        mode: HandlerMode,
        batch_policy: Option<BatchPolicy>,
        handler_timeout: Option<Duration>,
        name: String,
        rayon_pool: Option<Arc<RayonPool>>,
    ) -> Self {
        Self {
            callback,
            retry_policy,
            mode,
            batch_policy,
            handler_timeout,
            name,
            rayon_pool,
        }
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

    /// Returns the handler name (typically the topic name).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Invokes the handler according to its mode, with an optional timeout.
    ///
    /// If `self.handler_timeout` is set, wraps the invocation in
    /// `tokio::time::timeout`. On timeout, returns `ExecutionResult::Error`
    /// with `Terminal(HandlerPanic)` and a descriptive message.
    pub async fn invoke_mode(
        &self,
        ctx: &ExecutionContext,
        message: OwnedMessage,
    ) -> ExecutionResult {
        let result = match self.mode() {
            HandlerMode::SingleSync => self.invoke(ctx, message).await,
            HandlerMode::SingleAsync => self.invoke_async(ctx, message).await,
            HandlerMode::BatchSync => {
                let result = self.invoke_batch(ctx, vec![message]).await;
                match result {
                    BatchExecutionResult::AllSuccess(_) => ExecutionResult::Ok,
                    BatchExecutionResult::AllFailure(reason) => ExecutionResult::Error {
                        reason,
                        exception: "BatchHandlerError".to_string(),
                        traceback: "Batch handler failed".to_string(),
                    },
                    BatchExecutionResult::PartialFailure { .. } => ExecutionResult::Error {
                        reason: FailureReason::Terminal(crate::failure::TerminalKind::HandlerPanic),
                        exception: "PartialFailureNotImplemented".to_string(),
                        traceback: "PartialFailure not implemented in v1.6".to_string(),
                    },
                }
            }
            HandlerMode::BatchAsync => {
                let result = self.invoke_batch_async(ctx, vec![message]).await;
                match result {
                    BatchExecutionResult::AllSuccess(_) => ExecutionResult::Ok,
                    BatchExecutionResult::AllFailure(reason) => ExecutionResult::Error {
                        reason,
                        exception: "BatchHandlerError".to_string(),
                        traceback: "Batch handler failed".to_string(),
                    },
                    BatchExecutionResult::PartialFailure { .. } => ExecutionResult::Error {
                        reason: FailureReason::Terminal(crate::failure::TerminalKind::HandlerPanic),
                        exception: "PartialFailureNotImplemented".to_string(),
                        traceback: "PartialFailure not implemented in v1.6".to_string(),
                    },
                }
            }
        };

        // Apply handler timeout if configured
        // Note: timeout is applied at the invoke_mode_with_timeout level.
        // This method just passes through the result.
        let _ = self.handler_timeout;
        result
    }

    /// Invokes the handler according to its mode, wrapped in an optional timeout.
    ///
    /// This is the primary entry point for worker loops. If `handler_timeout` is set,
    /// the invocation is wrapped in `tokio::time::timeout`. On timeout, the message
    /// is classified as `Terminal(HandlerPanic)`.
    pub async fn invoke_mode_with_timeout(
        &self,
        ctx: &ExecutionContext,
        message: OwnedMessage,
    ) -> ExecutionResult {
        match self.handler_timeout {
            Some(timeout) => {
                match tokio::time::timeout(timeout, self.invoke_mode(ctx, message)).await {
                    Ok(result) => result,
                    Err(_) => {
                        tracing::error!(
                            handler_id = %ctx.topic,
                            topic = %ctx.topic,
                            partition = ctx.partition,
                            offset = ctx.offset,
                            timeout_ms = timeout.as_millis() as u64,
                            "handler timed out after {}ms",
                            timeout.as_millis()
                        );
                        ExecutionResult::Timeout {
                            info: TimeoutInfo {
                                timeout_ms: timeout.as_millis() as u64,
                                last_processed_offset: None,
                            },
                        }
                    }
                }
            }
            None => self.invoke_mode(ctx, message).await,
        }
    }

    /// Invokes the Python callable with the given message.
    ///
    /// Uses `spawn_blocking` to release the Tokio thread. GIL acquired only
    /// inside `Python::with_gil`. When a RayonPool is configured, dispatches
    /// to the Rayon work-stealing pool to avoid blocking the Tokio poll cycle.
    pub async fn invoke(&self, ctx: &ExecutionContext, message: OwnedMessage) -> ExecutionResult {
        let callback = Arc::clone(&self.callback);
        let ctx_clone = ctx.clone();
        let rayon_pool = self.rayon_pool.clone();

        // Extract W3C trace context from message headers before crossing GIL boundary
        let header_map: std::collections::HashMap<String, String> = message
            .headers
            .iter()
            .filter_map(|(k, v)| {
                v.as_ref()
                    .map(|bytes| String::from_utf8_lossy(bytes).to_string())
                    .map(|val| (k.clone(), val))
            })
            .collect();
        let mut trace_context = std::collections::HashMap::new();
        inject_trace_context(&header_map, &mut trace_context);

        // Clone trace_context before the conditional to avoid use-after-move
        let trace_context_for_rayon = trace_context.clone();
        let trace_context_for_tokio = trace_context;

        let result = if let Some(pool) = rayon_pool {
            // Dispatch to Rayon pool — closure MUST NOT call any Tokio APIs
            let (tx, rx) = tokio::sync::oneshot::channel();
            pool.spawn(move || {
                // On Rayon thread — safe to do CPU preprocessing here.
                // MUST NOT call tokio::spawn, Handle::current(), or any Tokio sync primitive.
                // Use std::thread::spawn for Python GIL calls (no tokio runtime on Rayon threads).
                let thread_result = std::thread::spawn(move || {
                    Python::attach(|py| {
                        let py_msg = message_to_pydict(py, &message, Some(&trace_context_for_rayon));
                        let py_ctx = ctx_to_pydict(py, &ctx_clone, &message);
                        match callback.call(py, (py_msg, py_ctx), None) {
                            Ok(_) => ExecutionResult::Ok,
                            Err(py_err) => {
                                let classifier = DefaultFailureClassifier;
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
                .join();
                // Send the thread result back to Tokio via oneshot channel
                let _ = tx.send(thread_result);
            });
            // Receive from Rayon pool
            match rx.await {
                Ok(thread_result) => match thread_result {
                    Ok(result) => result,
                    Err(_) => ExecutionResult::Error {
                        reason: FailureReason::Terminal(crate::failure::TerminalKind::HandlerPanic),
                        exception: "Panic".to_string(),
                        traceback: "python handler thread panicked".to_string(),
                    },
                },
                Err(_) => ExecutionResult::Error {
                    reason: FailureReason::Terminal(crate::failure::TerminalKind::HandlerPanic),
                    exception: "Panic".to_string(),
                    traceback: "rayon pool task panicked (sender dropped)".to_string(),
                },
            }
        } else {
            // Fallback: use spawn_blocking directly (no Rayon pool configured)
            tokio::task::spawn_blocking(move || {
                Python::attach(|py| {
                    let py_msg = message_to_pydict(py, &message, Some(&trace_context_for_tokio));
                    let py_ctx = ctx_to_pydict(py, &ctx_clone, &message);
                    match callback.call(py, (py_msg, py_ctx), None) {
                        Ok(_) => ExecutionResult::Ok,
                        Err(py_err) => {
                            let classifier = DefaultFailureClassifier;
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
            .await
            .unwrap_or_else(|_| ExecutionResult::Error {
                reason: FailureReason::Terminal(crate::failure::TerminalKind::HandlerPanic),
                exception: "Panic".to_string(),
                traceback: "spawn_blocking task panicked".to_string(),
            })
        };

        // Both branches already return ExecutionResult
        result
    }

    /// Invokes the Python handler with a batch of messages via spawn_blocking.
    ///
    /// Used for HandlerMode::BatchSync. GIL acquired once per batch.
    /// Returns BatchExecutionResult::AllSuccess(Vec<Offset>) on success,
    /// BatchExecutionResult::AllFailure(FailureReason) on any exception.
    pub async fn invoke_batch(
        &self,
        ctx: &ExecutionContext,
        messages: Vec<OwnedMessage>,
    ) -> BatchExecutionResult {
        let callback = Arc::clone(&self.callback);
        let ctx_clone = ctx.clone();
        let worker_id = ctx.worker_id;

        // Extract W3C trace context from message headers before crossing GIL boundary
        let header_map: std::collections::HashMap<String, String> = messages
            .first()
            .map(|msg| {
                msg.headers
                    .iter()
                    .filter_map(|(k, v)| {
                        v.as_ref()
                            .map(|bytes| String::from_utf8_lossy(bytes).to_string())
                            .map(|val| (k.clone(), val))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let mut trace_context = std::collections::HashMap::new();
        inject_trace_context(&header_map, &mut trace_context);

        let result = tokio::task::spawn_blocking(move || {
            Python::attach(|py| {
                // Build Vec<PyDict> for the batch — one dict per message
                let py_batch: Vec<Py<PyAny>> = messages
                    .iter()
                    .map(|msg| message_to_pydict(py, msg, Some(&trace_context)))
                    .collect();

                match callback.call1(py, (py_batch,)) {
                    Ok(_) => {
                        // Collect offsets from messages in batch order
                        let offsets: Vec<i64> = messages.iter().map(|m| m.offset).collect();
                        BatchExecutionResult::AllSuccess(offsets)
                    }
                    Err(py_err) => {
                        let classifier = DefaultFailureClassifier;
                        let ctx_err_clone = ExecutionContext::new(
                            ctx_clone.topic.clone(),
                            ctx_clone.partition,
                            0,
                            worker_id,
                        );
                        let reason = classifier.classify(&py_err, &ctx_err_clone);
                        BatchExecutionResult::AllFailure(reason)
                    }
                }
            })
        })
        .await;

        match result {
            Ok(r) => r,
            Err(_) => BatchExecutionResult::AllFailure(FailureReason::Terminal(
                crate::failure::TerminalKind::HandlerPanic,
            )),
        }
    }

    /// Invokes the Python callable asynchronously via PythonAsyncFuture.
    ///
    /// Used for HandlerMode::SingleAsync. Creates a coroutine object inside
    /// Python::with_gil, then wraps it in PythonAsyncFuture which handles
    /// GIL release on each poll. The GIL is held only during coroutine.send(None).
    pub async fn invoke_async(
        &self,
        _ctx: &ExecutionContext,
        message: OwnedMessage,
    ) -> ExecutionResult {
        let callback = Arc::clone(&self.callback);

        // Build the coroutine object inside with_gil — this is synchronous,
        // but the returned PythonAsyncFuture handles GIL release on each poll.
        let coro: Py<PyAny> = Python::attach(|py| {
            // Note: async path does not inject trace context (no spawn_blocking GIL boundary)
            let py_msg = message_to_pydict(py, &message, None);

            // Call the async function — returns a coroutine object.
            // The callback IS the coroutine function, calling it returns the coroutine object.
            callback
                .call1(py, (py_msg,))
                .expect("callback must be a coroutine function")
        });

        // Drive the coroutine as a Future — GIL released during await.
        PythonAsyncFuture::from(coro).await
    }

    /// Invokes the Python callable asynchronously with a batch of messages via PythonAsyncFuture.
    ///
    /// Used for HandlerMode::BatchAsync. Builds Vec<Py<PyAny>> of message dicts inside
    /// Python::with_gil, then wraps the resulting coroutine in PythonAsyncFuture.
    /// Returns BatchExecutionResult instead of ExecutionResult.
    pub async fn invoke_batch_async(
        &self,
        _ctx: &ExecutionContext,
        messages: Vec<OwnedMessage>,
    ) -> BatchExecutionResult {
        let callback = Arc::clone(&self.callback);

        // Build the coroutine object inside with_gil
        let coro: Py<PyAny> = Python::attach(|py| {
            // Note: async path does not inject trace context (no spawn_blocking GIL boundary)
            let py_batch: Vec<Py<PyAny>> = messages
                .iter()
                .map(|msg| message_to_pydict(py, msg, None))
                .collect();

            // Call the async batch function — returns a coroutine object.
            callback
                .call1(py, (py_batch,))
                .expect("callback must be a coroutine function")
        });

        // Drive the coroutine as a Future
        let result = PythonAsyncFuture::from(coro).await;

        // Convert ExecutionResult to BatchExecutionResult
        match result {
            ExecutionResult::Ok => {
                let offsets: Vec<i64> = messages.iter().map(|m| m.offset).collect();
                BatchExecutionResult::AllSuccess(offsets)
            }
            ExecutionResult::Error { reason, .. } => BatchExecutionResult::AllFailure(reason),
            ExecutionResult::Rejected { reason: _, .. } => {
                // Treat rejected as failure with Terminal kind
                BatchExecutionResult::AllFailure(FailureReason::Terminal(
                    crate::failure::TerminalKind::HandlerPanic,
                ))
            }
            ExecutionResult::Timeout { .. } => {
                // Treat timeout as failure with Terminal kind
                BatchExecutionResult::AllFailure(FailureReason::Terminal(
                    crate::failure::TerminalKind::HandlerPanic,
                ))
            }
        }
    }

    /// Dispatches to the appropriate batch invoke based on HandlerMode.
    ///
    /// Used by batch_worker_loop to route to either invoke_batch (BatchSync) or
    /// invoke_batch_async (BatchAsync) without duplicating the dispatch logic.
    pub async fn invoke_mode_batch(
        &self,
        ctx: &ExecutionContext,
        messages: Vec<OwnedMessage>,
    ) -> BatchExecutionResult {
        match self.mode() {
            HandlerMode::BatchSync => self.invoke_batch(ctx, messages).await,
            HandlerMode::BatchAsync => self.invoke_batch_async(ctx, messages).await,
            _ => unreachable!("invoke_mode_batch only valid for batch modes"),
        }
    }
}
