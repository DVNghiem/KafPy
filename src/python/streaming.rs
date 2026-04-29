//! Streaming handler — drives Python async generators to completion.
//!
//! StreamingHandler wraps a Python async generator and polls it via PythonAsyncFuture
//! in a loop. Each yield is treated as a message; StopAsyncIteration signals end of stream.

use crate::dispatcher::OwnedMessage;
use crate::failure::{FailureReason, TerminalKind};
use crate::python::async_bridge::PythonAsyncFuture;
use crate::python::context::ExecutionContext;
use crate::python::execution_result::ExecutionResult;
use crate::python::handler::message_to_pydict;
use std::sync::Arc;

use pyo3::prelude::*;

/// Wraps a streaming async handler invocation — drives a Python async generator via polling loop.
pub struct StreamingHandler {
    callback: Arc<Py<PyAny>>,
}

impl StreamingHandler {
    /// Creates a new StreamingHandler from a Python async generator callable.
    pub fn new(callback: Arc<Py<PyAny>>) -> Self {
        Self { callback }
    }

    /// Drives a Python async generator until StopAsyncIteration or error.
    ///
    /// Creates a coroutine inside GIL, wraps in PythonAsyncFuture, polls in a loop.
    /// Each poll represents one yield from the generator. Normal completion (StopAsyncIteration)
    /// returns `ExecutionResult::Ok`. Errors propagate as `ExecutionResult::Error`.
    pub async fn invoke_streaming(
        &self,
        _ctx: &ExecutionContext,
        initial_message: OwnedMessage,
    ) -> ExecutionResult {
        // Build the coroutine object inside GIL
        let coro: Py<PyAny> = Python::attach(|py| {
            let py_msg = message_to_pydict(py, &initial_message, None);
            self.callback
                .call1(py, (py_msg,))
                .expect("callback must be an async generator")
        });

        // Wrap in PythonAsyncFuture and poll in a loop
        let future = PythonAsyncFuture::from(coro);

        loop {
            match future.await {
                ExecutionResult::Ok => {
                    // Generator exhausted normally — end of stream
                    break;
                }
                ExecutionResult::Error {
                    reason,
                    exception,
                    traceback,
                } => {
                    // Error during yield — propagate
                    return ExecutionResult::Error {
                        reason,
                        exception,
                        traceback,
                    };
                }
                // Rejected or Timeout — treat as terminal error in streaming context
                _ => {
                    return ExecutionResult::Error {
                        reason: FailureReason::Terminal(TerminalKind::HandlerPanic),
                        exception: "StreamingError".to_string(),
                        traceback: "Unexpected result in streaming loop".to_string(),
                    };
                }
            }
        }

        ExecutionResult::Ok
    }
}