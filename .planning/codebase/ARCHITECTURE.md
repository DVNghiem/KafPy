# Architecture

**Analysis Date:** 2026-04-15

## Pattern Overview

**Overall:** PyO3 Native Extension with Async Kafka Integration

**Key Characteristics:**
- Rust extension module (`_kafpy`) providing high-performance Kafka producer/consumer
- Async-first design using Tokio runtime integrated with Python async via `pyo3-async-runtimes`
- Clear separation between synchronous Python API and asynchronous Rust internals
- Domain-driven design with explicit error types and message routing

## Layers

**Python API Layer (Public Interface):**
- Purpose: Expose Rust functionality to Python consumers with idiomatic API
- Location: `kafpy/__init__.py`
- Contains: Python wrapper classes re-exported from `_kafpy`
- Depends on: `_kafpy` native module
- Used by: Python applications consuming Kafka messages

**PyO3 Binding Layer (FFI Bridge):**
- Purpose: Bridge between Python and Rust, exposing Rust structs as Python classes
- Location: `src/lib.rs`
- Contains: Module initialization, class registration with `#[pymodule]` and `#[pyclass]`
- Depends on: All internal Rust modules
- Used by: Python interpreter via `_kafpy` import

**Configuration Layer:**
- Purpose: Encapsulate Kafka client configuration with environment variable support
- Location: `src/config.rs`
- Contains: `ConsumerConfig`, `ProducerConfig` with `from_env()` static methods
- Depends on: `dotenvy` for `.env` loading, `rdkafka` for config types
- Used by: `PyConsumer`, `PyProducer`

**Consumer Layer:**
- Purpose: Manage Kafka consumer lifecycle, message streaming, and processing
- Location: `src/consume.rs`
- Contains: `PyConsumer`, `KafkaManager`, `CustomConsumerContext`
- Depends on: `rdkafka::consumer`, `tokio`, `CancellationToken`
- Used by: Python code via `PyConsumer` wrapper

**Producer Layer:**
- Purpose: Send messages to Kafka topics with async/sync options
- Location: `src/produce.rs`
- Contains: `PyProducer`, `FutureProducer` wrapper
- Depends on: `rdkafka::producer`, `tokio::sync::RwLock`
- Used by: Python code via `PyProducer` wrapper

**Message Processing Layer:**
- Purpose: Route messages to registered handlers based on topic
- Location: `src/message_processor.rs`
- Contains: `MessageProcessor` trait, `MessageRouter`, `PyProcessor`
- Depends on: `async_trait`, Python GIL integration
- Used by: `PyConsumer` via message router

**Domain Layer:**
- Purpose: Represent Kafka message domain object
- Location: `src/kafka_message.rs`
- Contains: `KafkaMessage` struct with topic, partition, offset, key, payload, headers
- Depends on: `rdkafka::message`
- Used by: Consumer and processor layers

**Error Handling Layer:**
- Purpose: Define typed errors for consumer and domain operations
- Location: `src/errors.rs`
- Contains: `KafkaConsumerError`, `DomainError` enums using `thiserror`
- Depends on: `rdkafka::error`
- Used by: All layers via `?` operator

**Logging Layer:**
- Purpose: Initialize structured logging with tracing
- Location: `src/logging.rs`
- Contains: `Logger::init()` using `tracing_subscriber`
- Depends on: `tracing`, `tracing-subscriber`
- Used by: Module initialization in `lib.rs`

## Data Flow

**Consumer Flow:**

```
Python: consumer.start()
    |
    v
PyConsumer::start() --> pyo3_async_runtimes::future_into_py()
    |
    v
Tokio Runtime (async context)
    |
    v
KafkaManager::create_consumer() --> rdkafka::ClientConfig::create_with_context()
    |
    v
KafkaManager::start_consuming()
    |
    +--> tokio::spawn (message reader task)
    |       |
    |       v
    |   consumer.recv() --> rdkafka stream
    |       |
    |       v
    |   KafkaMessage::from_rdkafka_message()
    |       |
    |       v
    |   mpsc::channel::send()
    |
    +--> while let Some(message) = rx.recv()
            |
            v
        MessageRouter::route_message()
            |
            v
        PyProcessor::process_message()
            |
            v
        Python callback (via GIL)
            |
            v
        consumer.commit_consumer_state() (if manual commit)
```

**Producer Flow:**

```
Python: producer.init()
    |
    v
PyProducer::init() --> pyo3_async_runtimes::future_into_py()
    |
    v
Tokio Runtime
    |
    v
PyProducer::create_producer() --> rdkafka::ClientConfig::create()
    |
    v
Store FutureProducer in Arc<RwLock<Option<FutureProducer>>>

Python: producer.send(topic, key, payload, ...)
    |
    v
PyProducer::send() --> pyo3_async_runtimes::future_into_py()
    |
    v
Tokio Runtime
    |
    v
Read producer from RwLock
    |
    v
rdkafka::FutureProducer::send() with FutureRecord
    |
    v
Return (partition, offset) or raise PyErr
```

## Key Abstractions

**MessageProcessor Trait:**
- Purpose: Abstract message handler interface for router pattern
- Examples: `PyProcessor` wraps Python callbacks, `MessageRouter` delegates to registered processors
- Pattern: `async_trait` with `process_message()`, `supported_topic()`, `processor_name()`
- Location: `src/message_processor.rs`

**MessageRouter:**
- Purpose: Route incoming messages to appropriate processor based on topic
- Pattern: Registry of processors, wildcard `"*"` support for global handlers
- Location: `src/message_processor.rs`

**CancellationToken:**
- Purpose: Graceful shutdown coordination across async tasks
- Pattern: `tokio_util::sync::CancellationToken` passed to reader and processor loops
- Location: `src/consume.rs`

**Arc<RwLock<Option<T>>> for Producer:**
- Purpose: Thread-safe lazy initialization of FutureProducer
- Pattern: `Arc` allows sharing across async tasks, `RwLock` allows write-once read-many
- Location: `src/produce.rs`

## Entry Points

**Python Module (`_kafpy`):**
- Location: `src/lib.rs`
- Triggers: `import _kafpy` from Python
- Responsibilities: Initialize logging, register all `#[pyclass]` types with Python interpreter

**Python Package (`kafpy`):**
- Location: `kafpy/__init__.py`
- Triggers: `from kafpy import Consumer, Producer, ...`
- Responsibilities: Re-export public API from `_kafpy`, define `__version__`

**Consumer Start:**
- Location: `src/consume.rs` - `PyConsumer::start()`
- Triggers: Python calls `consumer.start()`
- Responsibilities: Initialize Kafka consumer, subscribe to topics, start reader/processor loops

**Producer Init:**
- Location: `src/produce.rs` - `PyProducer::init()`
- Triggers: Python calls `producer.init()`
- Responsibilities: Create FutureProducer, store in Arc<RwLock>

## Error Handling

**Strategy:** Typed error enums with `thiserror`, converted to Python exceptions via `PyErr`

**Patterns:**
- `KafkaConsumerError`: Wraps `rdkafka::error::KafkaError`, `Processing(String)`, `Serialization(String)`
- `DomainError`: Validation, BusinessRule, NotFound, Conflict, Serialization, Database, Processing
- All errors converted to `PyErr` via `PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(...)` in async contexts

**Key Error Conversions:**
```rust
// In async context
.map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))

// Sync context
.map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?
```

## Cross-Cutting Concerns

**Logging:** `tracing` crate with `tracing_subscriber::fmt()`, environment-based filter (`RUST_LOG`), includes line numbers, thread IDs, file names

**Validation:** Consumer and Producer configs validate via `from_env()` with `parse()?` for numeric types, returning `PyResult` with descriptive errors

**Authentication:** SASL configuration via optional fields in config structs (`security_protocol`, `sasl_mechanism`, `sasl_username`, `sasl_password`)

**Thread Safety:**
- `PyProducer` uses `Arc<RwLock<Option<FutureProducer>>>` for producer sharing
- `PyConsumer` uses `Arc<StreamConsumer<CustomConsumerContext>>` for consumer sharing
- `MessageRouter` uses `Arc<MessageRouter>` for processor registry sharing

**Graceful Shutdown:**
- `CancellationToken` propagates to reader and processor loops
- SIGINT (Ctrl+C) and SIGTERM (Unix) handlers in `KafkaManager::shutdown_signal()`
- Consumer stops processing on token cancellation

---

*Architecture analysis: 2026-04-15*
