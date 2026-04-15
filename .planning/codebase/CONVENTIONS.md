# Coding Conventions

**Analysis Date:** 2026-04-15

## Overview

KafPy is a PyO3-based Python extension written in Rust, providing high-performance Kafka consumer and producer functionality. The codebase follows standard Rust conventions with PyO3-specific patterns for Python interoperability.

## Rust Naming Conventions

The project follows standard Rust naming conventions:

**Functions and Methods:** `snake_case`
- Example from `src/consume.rs`: `pub fn start(&self, py: Python<'_>) -> PyResult<Py<PyAny>>`
- Example from `src/produce.rs`: `pub fn send(...)` and `pub fn flush(...)`

**Types and Structs:** `PascalCase`
- `PyConsumer`, `PyProducer`, `ConsumerConfig`, `ProducerConfig`, `KafkaMessage`
- Exception: The library crate name `_kafpy` (with leading underscore) due to Python module naming requirements

**Enums:** `PascalCase`
- `KafkaConsumerError`, `DomainError` in `src/errors.rs`

**Constants:** `SCREAMING_SNAKE_CASE`
- Not heavily used in this codebase, but follows Rust convention

## PyO3 Attribute Patterns

### #[pyclass]

Structs exposed to Python use `#[pyclass]`:

```rust
// src/consume.rs
#[pyclass(name="Consumer")]
pub struct PyConsumer { ... }

// src/config.rs
#[pyclass]
pub struct ConsumerConfig { ... }

// src/kafka_message.rs
#[pyclass]
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct KafkaMessage { ... }
```

Note: `name="Consumer"` renames the Python-exposed class. Without this, it would be `PyConsumer` in Python.

### #[pymethods]

Async methods use `pyo3_async_runtimes::tokio::future_into_py`:

```rust
// src/consume.rs - Async start method
pub fn start(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
    pyo3_async_runtimes::tokio::future_into_py(py, async move {
        // Async logic here
    })
    .map(|b| b.unbind())
}
```

### #[pyo3(signature = (...))]

Default parameter values are specified in the signature attribute:

```rust
// src/config.rs
#[pymethods]
impl ConsumerConfig {
    #[new]
    #[pyo3(signature = (
        brokers,
        group_id,
        topics,
        auto_offset_reset="earliest".to_string(),
        enable_auto_commit=false,
        session_timeout_ms=30000,
        ...
    ))]
    pub fn new(...) -> Self { ... }
}
```

### Getters and Custom Properties

```rust
// src/kafka_message.rs
#[pymethods]
impl KafkaMessage {
    #[getter]
    pub fn get_key(&self) -> Option<Vec<u8>> {
        self.key.clone()
    }

    #[getter]
    pub fn get_headers(&self) -> Vec<(String, Option<Vec<u8>>)> {
        self.headers.clone()
    }
}
```

## Error Handling

### thiserror for Typed Errors

Internal Rust errors use `thiserror` for derive macros:

```rust
// src/errors.rs
#[derive(thiserror::Error, Debug)]
#[allow(dead_code)]
pub enum KafkaConsumerError {
    #[error("Kafka error: {0}")]
    Kafka(#[from] KafkaError),

    #[error("Message processing error: {0}")]
    Processing(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

#[derive(thiserror::Error, Debug)]
#[allow(dead_code)]
pub enum DomainError {
    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Entity not found: {0}")]
    NotFound(String),
    ...
}
```

### PyErr for Python Interop

When crossing the Python/Rust boundary, errors are converted to `PyErr`:

```rust
// src/consume.rs
pyo3_async_runtimes::tokio::future_into_py(py, async move {
    KafkaManager::create_consumer(&config, context)
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?
    ...
})
```

Pattern: `PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(message)` for generic runtime errors.

## Async Patterns

### tokio Runtime with pyo3_async_runtimes

All async Rust code runs on tokio, integrated with Python via `pyo3_async_runtimes`:

```rust
// src/produce.rs - Async init method
pub fn init(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
    pyo3_async_runtimes::tokio::future_into_py(py, async move {
        let producer = Self::create_producer(&config)
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        // ...
    })
    .map(|b| b.unbind())
}
```

### CancellationToken for Graceful Shutdown

Async operations use `tokio_util::sync::CancellationToken`:

```rust
// src/consume.rs
pub struct PyConsumer {
    config: ConsumerConfig,
    message_router: Arc<MessageRouter>,
    cancellation_token: CancellationToken,
}

pub fn stop(&self) {
    self.cancellation_token.cancel();
}
```

Usage in async loops:
```rust
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

Logging is configured in `src/logging.rs`:

```rust
// src/logging.rs
pub struct Logger;

impl Logger {
    pub fn init() {
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "info,consumer=debug".into()),
            )
            .with_line_number(true)
            .with_thread_ids(true)
            .with_file(true)
            .init();
    }
}
```

### Log Levels and Usage

```rust
// src/consume.rs
info!("Starting Kafka message consumption");
debug!("Offset commit successful");
error!("Failed to commit offset: {}", e);
warn!("No processor found for topic: {}", message.topic);
```

## Configuration Defaults

Default values are specified in `#[pyo3(signature = (...))]` attributes in `src/config.rs`:

```rust
// ConsumerConfig defaults
#[pyo3(signature = (
    brokers,
    group_id,
    topics,
    auto_offset_reset="earliest".to_string(),
    enable_auto_commit=false,
    session_timeout_ms=30000,
    heartbeat_interval_ms=3000,
    max_poll_interval_ms=300000,
    security_protocol = None,
    sasl_mechanism = None,
    ...
))]

// ProducerConfig defaults
#[pyo3(signature = (
    brokers,
    message_timeout_ms = 30000,
    queue_buffering_max_messages = 100000,
    queue_buffering_max_kbytes = 1048576,
    batch_num_messages = 10000,
    compression_type = "snappy".to_string(),
    linger_ms = 5,
    ...
))]
```

### from_env() Static Method

Config types provide `from_env()` to load from environment variables:

```rust
// src/config.rs
#[pymethods]
impl ConsumerConfig {
    #[staticmethod]
    pub fn from_env() -> PyResult<Self> {
        // Load from .env file if it exists
        dotenvy::dotenv().ok();

        Ok(ConsumerConfig {
            brokers: env::var("KAFKA_BROKERS").unwrap_or_else(|_| "localhost:9092".to_string()),
            ...
        })
    }
}
```

## Module Structure

```
src/
├── lib.rs           # Module imports, #[pymodule] definition
├── config.rs        # ConsumerConfig, ProducerConfig structs
├── consume.rs       # PyConsumer, KafkaManager
├── produce.rs       # PyProducer
├── errors.rs        # KafkaConsumerError, DomainError enums
├── kafka_message.rs # KafkaMessage wrapper
├── logging.rs       # Logger initialization
└── message_processor.rs  # MessageProcessor trait, PyProcessor, MessageRouter
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
- Private implementation details (e.g., `KafkaManager`) remain module-private
- `#[allow(dead_code)]` on enums that may be used in future

## Code Organization Principles

1. **Single responsibility**: Each module handles one domain (consume, produce, config, errors)
2. **PyO3 boundary**: Python-callable APIs clearly separated from internal Rust logic
3. **Async/Sync bridge**: `future_into_py` for all async methods exposed to Python
4. **Error conversion**: All errors converted to `PyErr` at Python boundary

---

*Conventions analysis: 2026-04-15*