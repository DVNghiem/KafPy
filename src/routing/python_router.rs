//! PythonRouter — routes messages via a Python callback callable.

use crate::routing::context::RoutingContext;
use crate::routing::decision::{RejectReason, RoutingDecision};
use crate::routing::router::Router;
use std::sync::Arc;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use tracing::warn;

/// RoutingDecision variant for Python routing errors (per D-08).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoutingPythonError(pub String);

/// PythonRouter wraps a Py<PyAny> callback that receives routing context.
///
/// Per PYROUTER-01: stores Py<PyAny> callback callable.
/// Per PYROUTER-02: called via spawn_blocking for GIL release.
/// Per PYROUTER-03: Python callback returns RoutingDecision via string enum.
///
/// Python callable signature: `def my_router(msg: dict) -> str`
/// Returns: "route:{handler_id}" | "drop" | "reject:{reason}" | "defer"
pub struct PythonRouter {
    callback: Arc<Py<PyAny>>,
}

impl PythonRouter {
    /// Construct with a Py<PyAny> callback.
    pub fn new(callback: Arc<Py<PyAny>>) -> Self {
        Self { callback }
    }

    /// Build a PyDict from RoutingContext per D-06.
    fn build_py_dict<'py>(&self, py: Python<'py>, ctx: &RoutingContext) -> &'py PyDict {
        let dict = PyDict::new(py);
        let _ = dict.set_item("topic", ctx.topic);
        let _ = dict.set_item("partition", ctx.partition);
        let _ = dict.set_item("offset", ctx.offset);
        let _ = dict.set_item("key", ctx.key.map(|b| b.to_vec()));
        let _ = dict.set_item("payload", ctx.payload.map(|b| b.to_vec()));
        // Headers: dict of bytes values per D-06
        let headers_dict = PyDict::new(py);
        for (k, v) in ctx.headers.iter() {
            let _ = headers_dict.set_item(k, v.as_ref().map(|b| b.to_vec()));
        }
        let _ = dict.set_item("headers", headers_dict);
        dict
    }

    /// Parse returned string into RoutingDecision per D-07.
    fn parse_return(&self, result: String) -> RoutingDecision {
        if result.starts_with("route:") {
            let handler_id = result.strip_prefix("route:").unwrap().to_string();
            RoutingDecision::Route(handler_id)
        } else if result == "drop" {
            RoutingDecision::Drop
        } else if result.starts_with("reject:") {
            let reason = result.strip_prefix("reject:").unwrap().to_string();
            RoutingDecision::Reject(RejectReason::Explicit(reason))
        } else if result == "defer" {
            RoutingDecision::Defer
        } else {
            // Unknown return — treat as reject with reason
            RoutingDecision::Reject(RejectReason::Explicit(result))
        }
    }
}

impl Router for PythonRouter {
    fn route(&self, ctx: &RoutingContext) -> RoutingDecision {
        let callback = Arc::clone(&self.callback);

        // Call via spawn_blocking to release the Tokio thread; GIL acquired inside with_gil
        let result = tokio::task::spawn_blocking(move || {
            Python::with_gil(|py| {
                let py_dict = {
                    let dict = PyDict::new(py);
                    let _ = dict.set_item("topic", ctx.topic);
                    let _ = dict.set_item("partition", ctx.partition);
                    let _ = dict.set_item("offset", ctx.offset);
                    let _ = dict.set_item("key", ctx.key.map(|b| b.to_vec()));
                    let _ = dict.set_item("payload", ctx.payload.map(|b| b.to_vec()));
                    let headers_dict = PyDict::new(py);
                    for (k, v) in ctx.headers.iter() {
                        let _ = headers_dict.set_item(k, v.as_ref().map(|b| b.to_vec()));
                    }
                    let _ = dict.set_item("headers", headers_dict);
                    dict
                };

                match callback.call1(py, (py_dict,)) {
                    Ok(py_result) => {
                        let s: String = py_result.extract().unwrap_or_else(|_| {
                            "reject:python_router_invalid_return".to_string()
                        });
                        if s.starts_with("route:") {
                            let handler_id = s.strip_prefix("route:").unwrap().to_string();
                            RoutingDecision::Route(handler_id)
                        } else if s == "drop" {
                            RoutingDecision::Drop
                        } else if s.starts_with("reject:") {
                            let reason = s.strip_prefix("reject:").unwrap().to_string();
                            RoutingDecision::Reject(RejectReason::Explicit(reason))
                        } else if s == "defer" {
                            RoutingDecision::Defer
                        } else {
                            RoutingDecision::Reject(RejectReason::Explicit(s))
                        }
                    }
                    Err(py_err) => {
                        // Per D-08: log WARN and return Reject(RoutingPythonError)
                        let err_msg = py_err.to_string();
                        warn!(error = %err_msg, "Python router error");
                        RoutingDecision::Reject(RejectReason::Explicit(format!(
                            "python_router_error: {}",
                            err_msg
                        )))
                    }
                }
            })
        });

        match result {
            Ok(decision) => decision,
            Err(_) => {
                // spawn_blocking panicked — treat as routing error
                warn!("Python router spawn_blocking task panicked");
                RoutingDecision::Reject(RejectReason::Explicit("python_router_panic".to_string()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_callback(py: Python<'_>, result: &str) -> Arc<Py<PyAny>> {
        let code = format!("lambda msg: '{}'", result);
        let locals = PyDict::new(py);
        py.run_bound(&code, None, Some(&locals)).unwrap();
        Arc::new(locals.get_item("lambda").unwrap().unwrap().into_py(py))
    }

    fn routing_ctx() -> RoutingContext<'static> {
        RoutingContext {
            topic: "test-topic",
            partition: 0,
            offset: 100,
            key: None,
            payload: Some(b"hello"),
            headers: Box::leak(vec![("x-trace".to_string(), Some(b"abc".to_vec()))].into_boxed_slice()),
        }
    }

    #[test]
    fn construction_accepts_arc_py_any() {
        // This test verifies PythonRouter::new compiles with Arc<Py<PyAny>>
        // We can't fully test without a real Python interpreter, but the type check passes
        // The actual async test below exercises the full path
        PythonRouter::new(Arc::new(pyo3::Python::with_gil(|py| pyo3::types::PyNone(py).into_py(py))));
    }

    #[tokio::test]
    async fn returns_drop_when_python_returns_drop() {
        let py_result: String = tokio::task::spawn_blocking(|| {
            Python::with_gil(|py| {
                let callback = make_callback(py, "drop");
                let router = PythonRouter::new(callback);
                let ctx = routing_ctx();
                router.route(&ctx)
            })
        })
        .await
        .unwrap();

        assert_eq!(py_result, "drop");
    }

    #[tokio::test]
    async fn returns_route_when_python_returns_route_handler_id() {
        let py_result: String = tokio::task::spawn_blocking(|| {
            Python::with_gil(|py| {
                let callback = make_callback(py, "route:my-handler");
                let router = PythonRouter::new(callback);
                let ctx = routing_ctx();
                match router.route(&ctx) {
                    RoutingDecision::Route(id) => id,
                    other => panic!("expected Route, got {:?}", other),
                }
            })
        })
        .await
        .unwrap();

        assert_eq!(py_result, "my-handler");
    }

    #[tokio::test]
    async fn returns_reject_when_python_returns_reject_reason() {
        let py_result: String = tokio::task::spawn_blocking(|| {
            Python::with_gil(|py| {
                let callback = make_callback(py, "reject:no_match");
                let router = PythonRouter::new(callback);
                let ctx = routing_ctx();
                match router.route(&ctx) {
                    RoutingDecision::Reject(RejectReason::Explicit(msg)) => msg,
                    other => panic!("expected Reject, got {:?}", other),
                }
            })
        })
        .await
        .unwrap();

        assert_eq!(py_result, "no_match");
    }

    #[tokio::test]
    async fn returns_defer_when_python_returns_defer() {
        let py_result: String = tokio::task::spawn_blocking(|| {
            Python::with_gil(|py| {
                let callback = make_callback(py, "defer");
                let router = PythonRouter::new(callback);
                let ctx = routing_ctx();
                router.route(&ctx)
            })
        })
        .await
        .unwrap();

        assert_eq!(py_result, "defer");
    }
}
