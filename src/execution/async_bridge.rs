//! Custom CFFI Future bridge — converts Python coroutines to Tokio-compatible Futures.
//!
//! # WARNING — Not for use in production hot paths
//!
//! `PythonAsyncFuture` drives a Python coroutine by calling `coro.send(None)` in each
//! `Future::poll` invocation. This has two critical problems when used directly on
//! Tokio worker threads:
//!
//! 1. **GIL contention on Tokio threads**: `Python::attach()` is called during each
//!    poll. If another thread holds the GIL, the Tokio worker thread is blocked until
//!    the GIL is released. This can stall the entire Tokio runtime.
//!
//! 2. **Busy-polling**: When the coroutine yields (returns `Ok(val)`), the previous
//!    implementation called `waker.wake_by_ref()` immediately, causing the task to be
//!    rescheduled on the very next Tokio tick. This produces a spin loop: Tokio polls
//!    the task → acquires GIL → coroutine yields → wakes immediately → repeat.
//!
//! 3. **No real asyncio event loop**: Python async I/O primitives (`asyncio.sleep`,
//!    network calls) do not advance correctly — the coroutine is polled manually
//!    without a running asyncio event loop. Any `await` that depends on asyncio
//!    internals (e.g., event loop scheduling) will behave incorrectly.
//!
//! # Production alternative
//!
//! Use `spawn_blocking` + `asyncio.run()` for async Python handlers (see
//! `PythonHandler::invoke_async` and `PythonHandler::invoke_batch_async`). This:
//! - Frees the Tokio worker thread while Python executes
//! - Provides a real asyncio event loop for Python async I/O
//! - Acquires the GIL only on the blocking thread pool
//!
//! This module is retained for internal testing only.

use crate::execution::execution_result::ExecutionResult;
use crate::failure::FailureReason;
use crate::failure::TerminalKind;
use pyo3::exceptions::{PyStopAsyncIteration, PyStopIteration};
use pyo3::prelude::*;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A Future that wraps a Python coroutine, bridging it to Tokio's async runtime.
///
/// The coroutine is polled by calling `coro.send(None)` which advances it to the next yield.
/// GIL is held only during the `send` call and released immediately.
///
/// # Type
/// `Output = ExecutionResult` — Ok on normal completion, Error on exception.
pub struct PythonAsyncFuture {
    /// The Python coroutine object — GIL-independent, Send + Sync.
    coro: Py<PyAny>,
}

impl PythonAsyncFuture {
    /// Creates a new `PythonAsyncFuture` from a Python coroutine object.
    pub fn new(coro: Py<PyAny>) -> Self {
        Self { coro }
    }

    /// Polls the underlying Python coroutine.
    ///
    /// # Behavior
    /// - Acquires GIL, calls `coro.send(None)`, releases GIL.
    /// - `Ok(val)` (yielded value) → `Poll::Pending`. Does NOT call `wake_by_ref()`
    ///   immediately. The caller is responsible for re-polling when appropriate.
    ///   Note: without an asyncio event loop driving wakeups, this Future will never
    ///   make progress after returning `Pending` unless the caller re-schedules it.
    ///   This is a fundamental limitation — see module-level documentation.
    /// - `Err(StopIteration)` or `Err(StopAsyncIteration)` → `Poll::Ready(Ok)` (normal return).
    /// - `Err(any other PyErr)` → `Poll::Ready(Error { reason: Terminal(HandlerPanic), ... })`.
    fn poll_coroutine(&mut self, _cx: &mut Context<'_>) -> Poll<ExecutionResult> {
        // Acquire GIL, call coro.send(None), release GIL immediately after.
        Python::attach(|py| {
            // Advance the coroutine by calling send(None)
            let result = self.coro.call_method1(py, "send", (py.None(),));

            match result {
                // Coroutine yielded — not done yet.
                // Return Pending without scheduling a wakeup. Without a running asyncio
                // event loop, the correct behavior here is implementation-defined.
                // Callers that need progress must re-poll explicitly.
                Ok(_val) => Poll::Pending,
                // Coroutine raised StopIteration or StopAsyncIteration — normal completion
                Err(py_err)
                    if py_err.is_instance_of::<PyStopIteration>(py)
                        || py_err.is_instance_of::<PyStopAsyncIteration>(py) =>
                {
                    Poll::Ready(ExecutionResult::Ok)
                }
                // Coroutine raised some other exception — propagate as error
                Err(py_err) => {
                    let exception = py_err
                        .get_type(py)
                        .name()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|_| "Unknown".to_string());
                    let traceback = py_err.to_string();
                    Poll::Ready(ExecutionResult::Error {
                        reason: FailureReason::Terminal(TerminalKind::HandlerPanic),
                        exception,
                        traceback,
                    })
                }
            }
        })
    }
}

/// Construct a `PythonAsyncFuture` from a `Py<PyAny>` coroutine object.
impl From<Py<PyAny>> for PythonAsyncFuture {
    fn from(coro: Py<PyAny>) -> Self {
        Self::new(coro)
    }
}

/// Clean up the coroutine when `PythonAsyncFuture` is dropped.
impl Drop for PythonAsyncFuture {
    fn drop(&mut self) {
        // Close the coroutine to ensure it releases resources properly.
        Python::attach(|py| {
            let _ = self.coro.call_method0(py, "close");
        });
    }
}

impl Future for PythonAsyncFuture {
    type Output = ExecutionResult;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Tokio guarantees poll is only called once until Pending is returned.
        // We call send(None) each time we are polled (after a wakeup).
        self.poll_coroutine(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_python_async_future_new() {
        // Verify construction from a valid Python object.
        Python::attach(|py| {
            let coro = py.None();
            let _future = PythonAsyncFuture::new(coro);
        });
    }

    #[test]
    fn test_from_py_any() {
        Python::attach(|py| {
            let coro: Py<PyAny> = py.None();
            let _future: PythonAsyncFuture = coro.into();
        });
    }
}
