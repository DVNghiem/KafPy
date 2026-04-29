//! Python middleware bindings for MIDW-04.
//!
//! Provides a bridge for Python middleware classes to implement HandlerMiddleware.
//! Python's BaseMiddleware class is defined in Python (kafpy/__init__.py).
//!
//! Built-in Rust middleware (Logging, Metrics) are exposed to Python via
//! type-name detection — when a Python `Logging()` or `Metrics()` instance
//! is passed to add_handler, we detect the type name and create the appropriate
//! Rust middleware. Custom Python middleware is wrapped in PythonMiddleware.

use crate::middleware::{HandlerMiddleware, Logging, Metrics, MiddlewareChain};
use crate::observability::metrics::SharedPrometheusSink;
use crate::python::context::ExecutionContext;
use crate::python::execution_result::ExecutionResult;
use pyo3::prelude::*;
use std::time::Duration;

/// Wraps a Python object implementing before/after/on_error as a HandlerMiddleware.
/// The Python object is stored as Py<PyAny> (Send+Sync when GIL not held).
pub struct PythonMiddleware {
    instance: Py<PyAny>,
}

impl PythonMiddleware {
    pub fn new(instance: Py<PyAny>) -> Self {
        Self { instance }
    }
}

impl HandlerMiddleware for PythonMiddleware {
    fn before(&self, ctx: &ExecutionContext) {
        Python::attach(|py| {
            if let Ok(inst) = self.instance.bind(py).getattr("before") {
                let ctx_dict = ctx_to_pydict(py, ctx);
                let _ = inst.call1((ctx_dict,));
            }
        });
    }

    fn after(&self, ctx: &ExecutionContext, result: &ExecutionResult, elapsed: Duration) {
        Python::attach(|py| {
            if let Ok(inst) = self.instance.bind(py).getattr("after") {
                let ctx_dict = ctx_to_pydict(py, ctx);
                let result_label = result.error_type_label().to_string();
                let elapsed_ms = elapsed.as_millis() as f64;
                let _ = inst.call1((ctx_dict, result_label, elapsed_ms));
            }
        });
    }

    fn on_error(&self, ctx: &ExecutionContext, result: &ExecutionResult) {
        Python::attach(|py| {
            if let Ok(inst) = self.instance.bind(py).getattr("on_error") {
                let ctx_dict = ctx_to_pydict(py, ctx);
                let result_label = result.error_type_label().to_string();
                let _ = inst.call1((ctx_dict, result_label));
            }
        });
    }
}

/// Helper to convert ExecutionContext to a Python dict for middleware hooks.
fn ctx_to_pydict<'py>(py: Python<'py>, ctx: &ExecutionContext) -> Py<PyAny> {
    use pyo3::types::PyDict;
    let dict = PyDict::new(py);
    let _ = dict.set_item("topic", &ctx.topic);
    let _ = dict.set_item("partition", ctx.partition);
    let _ = dict.set_item("offset", ctx.offset);
    let _ = dict.set_item("worker_id", ctx.worker_id);
    if let Some(ref tid) = ctx.trace_id {
        let _ = dict.set_item("trace_id", tid);
    }
    if let Some(ref sid) = ctx.span_id {
        let _ = dict.set_item("span_id", sid);
    }
    if let Some(ref flags) = ctx.trace_flags {
        let _ = dict.set_item("trace_flags", flags);
    }
    dict.into()
}

/// Attempts to convert a Python middleware instance (middleware: Vec<Py<PyAny>>)
/// into a MiddlewareChain. Checks type name to detect built-in vs custom Python middleware.
pub fn build_middleware_chain(
    middleware: Vec<Py<PyAny>>,
    metrics_sink: SharedPrometheusSink,
) -> MiddlewareChain {
    let mut chain = MiddlewareChain::new();
    for inst in middleware {
        // Check the Python type name to determine which middleware to create
        let type_name = Python::attach(|py| {
            inst.bind(py)
                .get_type()
                .name()
                .map(|s| s.to_string())
                .unwrap_or_default()
        });

        match type_name.as_str() {
            "Logging" => {
                // Built-in Logging — stateless, create directly
                chain = chain.add(Box::new(Logging::new()));
            }
            "Metrics" => {
                // Built-in Metrics — needs the metrics sink
                chain = chain.add(Box::new(Metrics::new(metrics_sink.clone())));
            }
            _ => {
                // Custom Python middleware — wrap in PythonMiddleware
                chain = chain.add(Box::new(PythonMiddleware::new(inst)));
            }
        }
    }
    chain
}