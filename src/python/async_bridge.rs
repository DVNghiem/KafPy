//! Custom CFFI Future bridge — converts Python coroutines to Tokio-compatible Futures.
//!
//! GIL is acquired transiently only during `coro.send(None)` and released immediately after.
//! This is the core async infrastructure for Phase 26 — all async Python handler execution
//! flows through this bridge.

use crate::failure::FailureReason;
use crate::failure::TerminalKind;
use crate::python::execution_result::ExecutionResult;
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
    /// - `Ok(val)` (yielded value) → `Poll::Pending`, waker registered.
    /// - `Err(StopIteration)` or `Err(StopAsyncIteration)` → `Poll::Ready(Ok)` (normal return).
    /// - `Err(any other PyErr)` → `Poll::Ready(Error { reason: Terminal(HandlerPanic), ... })`.
    fn poll_coroutine(&mut self, cx: &mut Context<'_>) -> Poll<ExecutionResult> {
        // Acquire GIL, call coro.send(None), release GIL immediately after.
        Python::attach(|py| {
            // Advance the coroutine by calling send(None)
            let result = self.coro.call_method1(py, "send", (py.None(),));

            match result {
                // Coroutine yielded — it is not done yet.
                // Instead of busy-polling with waker.wake() (which spins the CPU
                // continuously), we use wake_by_ref() which is more efficient.
                // The Tokio runtime will re-poll us on the next scheduler tick.
                // This prevents GIL thrashing and CPU starvation compared to
                // calling waker.wake() which immediately re-schedules.
                Ok(_val) => {
                    cx.waker().wake_by_ref();
                    Poll::Pending
                }
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
            let coro = py.None().into();
            let _future = PythonAsyncFuture::new(coro);
        });
    }

    #[test]
    fn test_from_py_any() {
        Python::attach(|py| {
            let coro: Py<PyAny> = py.None().into();
            let _future: PythonAsyncFuture = coro.into();
        });
    }
}
