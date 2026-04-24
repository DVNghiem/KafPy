# Coding Conventions

KafPy follows standard Rust conventions with PyO3-specific patterns for Python interoperability.

## Rust Naming Conventions

**Functions and Methods:** `snake_case`
```rust
pub fn start(&self, py: Python<'_>) -> PyResult<Py<PyAny>>
pub fn send(...) -> Result<...>
```

**Types and Structs:** `PascalCase`
```rust
pub struct PyConsumer { ... }
pub struct ConsumerConfig { ... }
pub enum KafkaConsumerError { ... }
```

**Constants:** `SCREAMING_SNAKE_CASE`

## PyO3 Attribute Patterns

### `#[pyclass]`

Structs exposed to Python use `#[pyclass]`:

```rust
#[pyclass(name="Consumer")]
pub struct PyConsumer { ... }

#[pyclass]
pub struct ConsumerConfig { ... }

#[pyclass]
#[derive(Debug, Clone)]
pub struct KafkaMessage { ... }
```

Note: `name="Consumer"` renames the Python-exposed class.

### `#[pymethods]`

Async methods use `pyo3_async_runtimes::tokio::future_into_py`:

```rust
pub fn start(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
    pyo3_async_runtimes::tokio::future_into_py(py, async move {
        // Async logic here
    })
    .map(|b| b.unbind())
}
```

### `#[pyo3(signature = (...))]`

Default parameter values in the signature attribute:

```rust
#[pymethods]
impl ConsumerConfig {
    #[new]
    #[pyo3(signature = (
        brokers,
        group_id,
        topics,
        auto_offset_reset="earliest".to_string(),
        enable_auto_commit=false,
    ))]
    pub fn new(...) -> Self { ... }
}
```

### Getters and Custom Properties

```rust
#[pymethods]
impl KafkaMessage {
    #[getter]
    pub fn get_key(&self) -> Option<Vec<u8>> {
        self.key.clone()
    }
}
```

## Error Handling

### thiserror for Typed Errors

```rust
#[derive(thiserror::Error, Debug)]
pub enum KafkaConsumerError {
    #[error("Kafka error: {0}")]
    Kafka(#[from] KafkaError),

    #[error("Message processing error: {0}")]
    Processing(String),
}
```

### PyErr for Python Interop

```rust
pyo3_async_runtimes::tokio::future_into_py(py, async move {
    create_consumer(&config)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?
})
```

## Async Patterns

### tokio Runtime with pyo3_async_runtimes

```rust
pub fn init(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
    pyo3_async_runtimes::tokio::future_into_py(py, async move {
        let producer = Self::create_producer(&config)?;
        Ok(producer)
    })
    .map(|b| b.unbind())
}
```

### CancellationToken for Graceful Shutdown

```rust
pub struct PyConsumer {
    cancellation_token: CancellationToken,
}

pub fn stop(&self) {
    self.cancellation_token.cancel();
}

// Usage in async loops:
tokio::select! {
    _ = cancellation_token.cancelled() => {
        info!("Received shutdown signal");
        break;
    }
    message_result = consumer.recv() => { ... }
}
```

## Logging

### tracing and tracing-subscriber

```rust
pub fn init() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,consumer=debug".into()),
        )
        .with_line_number(true)
        .with_thread_ids(true)
        .init();
}
```

## Python Module Mapping

The Rust library `_kafpy` is mapped to Python package `kafpy`:

```toml
# Cargo.toml
[package.metadata.maturin]
name = "kafpy"
python-source = "kafpy"
```

```python
# kafpy/__init__.py
__version__ = "0.1.0"
from ._kafpy import (
    ConsumerConfig,
    ProducerConfig,
    KafkaMessage,
    Consumer,
    Producer,
)
```

## Visibility and Privacy

- `pub struct` for PyO3-exposed types
- `pub fn` for methods exposed to Python
- Private implementation details remain module-private
- `#[allow(dead_code)]` on enums that may be used in future

## Code Organization Principles

1. **Single responsibility**: Each module handles one domain
2. **PyO3 boundary**: Python-callable APIs clearly separated from internal Rust logic
3. **Async/Sync bridge**: `future_into_py` for all async methods exposed to Python
4. **Error conversion**: All errors converted to `PyErr` at Python boundary