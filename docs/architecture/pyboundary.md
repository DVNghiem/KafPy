# PyO3 Boundary

Documentation of the GIL boundary, async bridges, and type conversions between Rust and Python.

## GIL Management Strategy

Python's Global Interpreter Lock (GIL) prevents concurrent Python code execution. KafPy minimizes GIL hold time by:

1. **All Python calls go through `spawn_blocking`** — Releases GIL during Python execution
2. **Rust async continues during Python calls** — Other tasks progress while waiting
3. **No GIL held across Rust orchestration** — GIL only acquired for actual Python calls

```mermaid
flowchart TB
    subgraph Rust["Rust Async Context"]
        T[Tokio Runtime]
        W[WorkerPool worker_loop]
        SB[spawn_blocking]
    end

    subgraph Python["Python Context"]
        GIL[Python GIL]
        CB[Callback execution]
    end

    W -->|message| SB
    SB -->|acquire GIL| GIL
    GIL -->|execute| CB
    CB -->|result| SB
    SB -->|release GIL| W
    T -->|其他任务| T

    style GIL fill:#ffcccc
    style CB fill:#ccffcc
```

## spawn_blocking Pattern

```rust
// python/handler.rs
impl PythonHandler {
    pub async fn execute(
        &self,
        message: OwnedMessage,
        context: ExecutionContext,
    ) -> ExecutionResult {
        let py_callback = self.callback.clone();

        tokio::task::spawn_blocking(move || {
            // GIL acquired here automatically
            Python::with_gil(|py| {
                let result = py_callback.call1(py, (/* args */));
                // GIL released when closure returns
            })
        })
        .await
        .map_err(|_| ExecutionError::TaskCancelled)?
    }
}
```

## Async/Sync Bridge

KafPy uses `pyo3-async-runtimes` to bridge Python async with Rust Tokio:

```mermaid
sequenceDiagram
    participant P as Python<br/>async def handler
    participant PA as PythonAsyncFuture
    participant T as Tokio Runtime
    participant K as Kafka

    Note over P: Python async def handler(msg):
        return await process(msg)

    P->>PA: Create future via PEP-492
    PA->>T: future_into_py() registers with Tokio
    T->>K: Kafka message arrives
    K-->>T: OwnedMessage
    T->>PA: poll() called
    PA->>P: Python coroutine advances
    P-->>PA: await yields
    PA-->>T: Poll::Pending
    T->>T: yield to other tasks
    T->>PA: poll() called again
    PA->>P: Python coroutine completes
    P-->>PA: result
    PA-->>T: Poll::Ready(result)
```

## Type Conversions

### Rust → Python

```rust
use pyo3::prelude::*;

// Rust struct becomes Python class
#[pyclass]
pub struct KafkaMessage {
    #[pyo3(get)]
    topic: String,
    #[pyo3(get)]
    partition: i32,
    #[pyo3(get)]
    offset: i64,
    #[pyo3(get)]
    payload: Option<Vec<u8>>,
}

// Rust error becomes Python exception
fn risky_operation() -> PyResult<i32> {
    Err(PyErr::new::<pyo3::exceptions::PyValueError, _>("invalid input"))
}
```

### Python → Rust

```rust
// Python callable stored as Py<PyAny>
#[pyclass]
pub struct PythonHandler {
    callback: Py<PyAny>,
}

// Call Python from Rust
fn invoke_callback(callback: &Py<PyAny>, py: Python<'_>) -> PyResult<HandlerResult> {
    callback.call1(py, (message,), None)
}
```

### PyO3 Configuration Type Conversions

KafPy bridges Python configuration dataclasses to Rust internal types via intermediate PyO3 types in `src/pyconfig.rs`:

#### PyRetryPolicy → RetryPolicy

```rust
// Python RetryConfig → PyRetryPolicy (PyO3 boundary) → Rust RetryPolicy (internal)
impl PyRetryPolicy {
    pub fn to_retry_policy(&self) -> RetryPolicy {
        RetryPolicy {
            max_attempts: self.max_attempts,
            base_delay: Duration::from_millis(self.base_delay_ms),
            max_delay: Duration::from_millis(self.max_delay_ms),
            jitter_factor: self.jitter_factor,
        }
    }
}
```

| Python Field (RetryConfig) | PyO3 Type (PyRetryPolicy) | Rust Type (RetryPolicy) |
|---|---|---|
| `max_attempts: int` | `max_attempts: u32` | `max_attempts: u32` |
| `base_delay_ms: int` | `base_delay_ms: u64` | `base_delay: Duration` |
| `max_delay_ms: int` | `max_delay_ms: u64` | `max_delay: Duration` |
| `jitter_factor: float` | `jitter_factor: f64` | `jitter_factor: f64` |

#### PyObservabilityConfig → ObservabilityConfig

```rust
impl PyObservabilityConfig {
    pub fn to_observability_config(&self) -> ObservabilityConfig {
        ObservabilityConfig {
            otlp_endpoint: self.otlp_endpoint.clone(),
            service_name: self.service_name.clone(),
            sampling_ratio: self.sampling_ratio,
            log_format: self.log_format.clone(),
        }
    }
}
```

| Python Field (ObservabilityConfig) | PyO3 Type (PyObservabilityConfig) | Rust Type (ObservabilityConfig) |
|---|---|---|
| `otlp_endpoint: str \| None` | `otlp_endpoint: Option<String>` | `otlp_endpoint: Option<String>` |
| `service_name: str` | `service_name: String` | `service_name: String` |
| `sampling_ratio: float` | `sampling_ratio: f64` | `sampling_ratio: f64` |
| `log_format: str` | `log_format: String` | `log_format: String` |

#### PyFailureCategory / PyFailureReason

```rust
// Python FailureCategory → PyFailureCategory (PyO3 enum) → Rust FailureCategory
#[pyclass]
pub enum PyFailureCategory {
    Retryable,
    Terminal,
    NonRetryable,
}

// Python FailureReason → PyFailureReason (PyO3 struct) → Rust FailureReason
#[pyclass]
pub struct PyFailureReason {
    pub category: PyFailureCategory,
    pub description: String,
}
```

| Python Type | PyO3 Type | Rust Type |
|---|---|---|
| `FailureCategory.Retryable` | `PyFailureCategory::Retryable` | `FailureCategory::Retryable` |
| `FailureCategory.Terminal` | `PyFailureCategory::Terminal` | `FailureCategory::Terminal` |
| `FailureCategory.NonRetryable` | `PyFailureCategory::NonRetryable` | `FailureCategory::NonRetryable` |
| `FailureReason` | `PyFailureReason` | `FailureReason` |

#### ConsumerConfig Extended Fields

The `ConsumerConfig.to_rust()` method in `src/runtime/builder.rs` wires the new optional fields through the PyO3 boundary:

```rust
// In ConsumerConfig (Python) → ConsumerConfig.to_rust() → RuntimeBuilder
impl ConsumerConfig {
    pub fn to_rust(&self) -> PyResult<RuntimeBuilder> {
        let mut builder = RuntimeBuilder::new(/* ... */);
        if let Some(retry_policy) = &self.default_retry_policy {
            builder = builder.retry_policy(retry_policy.to_retry_policy());
        }
        if let Some(obs_config) = &self.observability_config {
            builder = builder.observability(obs_config.to_observability_config());
        }
        builder = builder.dlq_prefix(self.dlq_topic_prefix.as_deref());
        builder = builder.drain_timeout(self.drain_timeout_secs);
        builder = builder.num_workers(self.num_workers.unwrap_or(4));
        builder = builder.auto_offset_store(self.enable_auto_offset_store.unwrap_or(false));
        Ok(builder)
    }
}
```

| Python Field | PyO3 Type | Rust Internal Target |
|---|---|---|
| `default_retry_policy` | `Option<PyRetryPolicy>` | `RetryPolicy` via `RetryCoordinator` |
| `dlq_topic_prefix` | `Option<String>` | `DlqRouter` prefix |
| `drain_timeout_secs` | `Option<u64>` | `ShutdownCoordinator` timeout |
| `num_workers` | `Option<usize>` | `WorkerPool` thread count |
| `enable_auto_offset_store` | `Option<bool>` | `OffsetCoordinator` auto-store |
| `observability_config` | `Option<PyObservabilityConfig>` | `MetricsSink` + `TracingSink` |

## Thread Safety Patterns

### Arc<RwLock<T>> for Shared Mutable State

```rust
// pyconsumer.rs
pub struct PyConsumer {
    runtime: Arc<RwLock<Option<ConsumerRuntime>>>,
}

impl PyConsumer {
    pub fn start(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let runtime = self.runtime.clone();
        future_into_py(py, async move {
            let mut guard = runtime.write().await;
            if guard.is_none() {
                *guard = Some(self.build_runtime().await?);
            }
            // ...
        })
    }
}
```

### Send + Sync Guarantees

Compile-time assertions ensure all shared types are thread-safe:

```rust
// lib.rs
fn _assert_send_sync_routing()
where
    crate::routing::HandlerId: Send + Sync,
    crate::routing::context::RoutingContext<'static>: Send + Sync,
    crate::routing::decision::RoutingDecision: Send + Sync,
    crate::routing::key::KeyRouter: Send + Sync,
{}

#[cfg(test)]
mod send_sync_assertions {
    #[test]
    fn routing_types_are_send_sync() {
        super::_assert_send_sync_routing();
    }
}
```