//! PythonRouter — routes messages via a Python callback callable.

use crate::routing::context::{RoutingContext, HandlerId};
use crate::routing::decision::{RejectReason, RoutingDecision};
use crate::routing::router::Router;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::sync::Arc;
use tracing::warn;

/// PythonRouter wraps a Py<PyAny> callback that receives routing context.
///
/// Per PYROUTER-01: stores Py<PyAny> callback callable.
/// Per PYROUTER-02: called via spawn_blocking for GIL release.
/// Per PYROUTER-03: Python callback returns RoutingDecision via string enum.
///
/// Python callable signature: `def my_router(msg: dict) -> str`
/// Returns: "route:{handler_id}" | "drop" | "reject:{reason}" | "defer"
#[allow(dead_code)]
pub struct PythonRouter {
    callback: Arc<Py<PyAny>>,
}

impl PythonRouter {
    /// Construct with a Py<PyAny> callback.
    #[allow(dead_code)]
    pub fn new(callback: Arc<Py<PyAny>>) -> Self {
        Self { callback }
    }

    /// Build a PyDict from RoutingContext inside a Python GIL guard.
    #[allow(dead_code)]
    fn call_py_and_parse(&self, ctx: &RoutingContext) -> RoutingDecision {
        let callback = Arc::clone(&self.callback);
        let topic = ctx.topic.to_string();
        let partition = ctx.partition;
        let offset = ctx.offset;
        let key = ctx.key.map(|b| b.to_vec());
        let payload = ctx.payload.map(|b| b.to_vec());
        let headers: Vec<(String, Option<Vec<u8>>)> = ctx
            .headers
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        // Use block_in_place so route() (sync) can call into async spawn_blocking
        let handle = tokio::task::spawn_blocking(move || {
            Python::attach(|py| {
                let py_dict = PyDict::new(py);
                let _ = py_dict.set_item("topic", topic);
                let _ = py_dict.set_item("partition", partition);
                let _ = py_dict.set_item("offset", offset);
                let _ = py_dict.set_item("key", key);
                let _ = py_dict.set_item("payload", payload);
                let headers_dict = PyDict::new(py);
                for (k, v) in &headers {
                    let _ = headers_dict.set_item(k, v.as_ref().map(|b| b.to_vec()));
                }
                let _ = py_dict.set_item("headers", headers_dict);

                match callback.call1(py, (py_dict,)) {
                    Ok(py_result) => {
                        let s: String = py_result
                            .extract(py)
                            .unwrap_or_else(|_| "reject:python_router_invalid_return".to_string());
                        Self::parse_return(s)
                    }
                    Err(py_err) => {
                        // Per D-08: log WARN and return Reject
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

        // route() is sync but spawn_blocking returns a JoinHandle, so block_on it
        tokio::runtime::Handle::current()
            .block_on(handle)
            .unwrap_or_else(|_| {
                // spawn_blocking panicked — treat as routing error
                warn!("Python router spawn_blocking task panicked");
                RoutingDecision::Reject(RejectReason::Explicit("python_router_panic".to_string()))
            })
    }

    /// Parse returned string into RoutingDecision per D-07.
    #[allow(dead_code)]
    fn parse_return(result: String) -> RoutingDecision {
        if result.starts_with("route:") {
            let handler_id = result.strip_prefix("route:").unwrap().to_string();
            RoutingDecision::Route(HandlerId::new(handler_id))
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
        self.call_py_and_parse(ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_callback(py: Python<'_>, result: &str) -> Arc<Py<PyAny>> {
        let code = format!("lambda msg: '{}'", result);
        let module = pyo3::types::PyModule::from_code(
            py,
            std::ffi::CString::new(code.as_bytes()).unwrap().as_c_str(),
            c"callback_builder",
            c"callback_builder",
        )
        .unwrap();
        let callback: Py<PyAny> = module.getattr("lambda").unwrap().into();
        Arc::new(callback)
    }

    fn routing_ctx() -> RoutingContext<'static> {
        RoutingContext {
            topic: "test-topic",
            partition: 0,
            offset: 100,
            key: None,
            payload: Some(b"hello"),
            headers: Box::leak(
                vec![("x-trace".to_string(), Some(b"abc".to_vec()))].into_boxed_slice(),
            ),
        }
    }

    #[test]
    fn construction_accepts_arc_py_any() {
        // Verifies PythonRouter::new compiles with Arc<Py<PyAny>>
        // Use a lambda that returns "defer" since we just need a valid Arc<Py<PyAny>>
        pyo3::Python::attach(|py| {
            let callback = make_callback(py, "defer");
            PythonRouter::new(callback);
        });
    }

    #[tokio::test]
    async fn returns_drop_when_python_returns_drop() {
        let decision = tokio::task::spawn_blocking(|| {
            Python::attach(|py| {
                let callback = make_callback(py, "drop");
                let router = PythonRouter::new(callback);
                router.route(&routing_ctx())
            })
        })
        .await
        .unwrap();

        assert!(matches!(decision, RoutingDecision::Drop));
    }

    #[tokio::test]
    async fn returns_route_when_python_returns_route_handler_id() {
        let decision = tokio::task::spawn_blocking(|| {
            Python::attach(|py| {
                let callback = make_callback(py, "route:my-handler");
                let router = PythonRouter::new(callback);
                router.route(&routing_ctx())
            })
        })
        .await
        .unwrap();

        match decision {
            RoutingDecision::Route(id) => assert_eq!(id.as_str(), "my-handler"),
            other => panic!("expected Route, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn returns_reject_when_python_returns_reject_reason() {
        let decision = tokio::task::spawn_blocking(|| {
            Python::attach(|py| {
                let callback = make_callback(py, "reject:no_match");
                let router = PythonRouter::new(callback);
                router.route(&routing_ctx())
            })
        })
        .await
        .unwrap();

        match decision {
            RoutingDecision::Reject(RejectReason::Explicit(msg)) => assert_eq!(msg, "no_match"),
            other => panic!("expected Reject, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn returns_defer_when_python_returns_defer() {
        let decision = tokio::task::spawn_blocking(|| {
            Python::attach(|py| {
                let callback = make_callback(py, "defer");
                let router = PythonRouter::new(callback);
                router.route(&routing_ctx())
            })
        })
        .await
        .unwrap();

        assert!(matches!(decision, RoutingDecision::Defer));
    }
}
