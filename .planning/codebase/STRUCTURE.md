# Codebase Structure

**Analysis Date:** 2026-04-15

## Directory Layout

```
/home/nghiem/project/KafPy/
├── Cargo.toml              # Rust package manifest (Kafpy native extension)
├── Cargo.lock              # Dependency lockfile
├── pyproject.toml          # Python package manifest
├── README.md               # Project documentation
├── LICENSE                 # Apache 2.0 license
├── src/                    # Rust source code (native extension)
│   ├── lib.rs              # Module root, PyO3 bindings
│   ├── config.rs           # ConsumerConfig, ProducerConfig
│   ├── consume.rs          # PyConsumer, KafkaManager, CustomConsumerContext
│   ├── produce.rs          # PyProducer, FutureProducer
│   ├── errors.rs           # KafkaConsumerError, DomainError
│   ├── kafka_message.rs    # KafkaMessage domain object
│   ├── logging.rs          # Logger initialization
│   └── message_processor.rs # MessageProcessor trait, MessageRouter, PyProcessor
├── kafpy/                  # Python package source
│   └── __init__.py         # Public Python API
├── target/                 # Build output (generated, not committed)
├── .github/                # GitHub workflows
└── .planning/              # Planning documents
```

## Directory Purposes

**`src/` (Rust Source):**
- Purpose: Rust extension module source code compiled to `_kafpy.{so,dylib}`
- Contains: All Rust implementation files
- Key files: `lib.rs`, `consume.rs`, `produce.rs`

**`kafpy/` (Python Package):**
- Purpose: Python-facing package that wraps the native extension
- Contains: `__init__.py` with public re-exports
- Key files: `__init__.py`

**`target/` (Build Output):**
- Purpose: Compiled artifacts from `cargo build`
- Generated: Yes
- Committed: No (in `.gitignore`)

**`.planning/` (Documentation):**
- Purpose: Architecture and planning documents
- Contains: ARCHITECTURE.md, STRUCTURE.md, and other GSD planning docs

## Key File Locations

**Entry Points:**
- `src/lib.rs`: PyO3 module initialization (`#[pymodule] fn _kafpy`)
- `kafpy/__init__.py`: Python package public API

**Configuration:**
- `src/config.rs`: `ConsumerConfig` and `ProducerConfig` structs with `#[pymethods]`
- `Cargo.toml`: Package metadata, dependencies, build configuration

**Core Logic:**
- `src/consume.rs`: `PyConsumer`, `KafkaManager`, `CustomConsumerContext`
- `src/produce.rs`: `PyProducer` with `FutureProducer`
- `src/message_processor.rs`: `MessageProcessor` trait, `MessageRouter`, `PyProcessor`

**Domain Objects:**
- `src/kafka_message.rs`: `KafkaMessage` struct with `from_rdkafka_message()` constructor

**Error Handling:**
- `src/errors.rs`: `KafkaConsumerError`, `DomainError` enums using `thiserror`

**Utilities:**
- `src/logging.rs`: `Logger::init()` for tracing subscriber setup

## File Purposes

**`src/lib.rs` (Module Root):**
```rust
// Responsibilities:
// 1. Import all submodules
// 2. Initialize tracing via Logger::init()
// 3. Register all #[pyclass] types with Python module
// 4. Export: KafkaMessage, PyConsumer, PyProducer, ConsumerConfig, ProducerConfig
```

**`src/config.rs` (Configuration):**
- `ConsumerConfig`: All Kafka consumer settings (brokers, group_id, topics, timeouts, security)
- `ProducerConfig`: All Kafka producer settings (brokers, timeouts, batching, compression, retries)
- Both have `#[new]` constructor and `#[staticmethod] from_env()` for `.env` loading
- Used by: `PyConsumer::new()`, `PyProducer::new()`

**`src/consume.rs` (Consumer Implementation):**
- `PyConsumer`: Python-facing consumer class with `#[pyclass]`
- `KafkaManager`: Internal consumer management (creation, subscription, message loop)
- `CustomConsumerContext`: `rdkafka::consumer::ConsumerContext` impl for rebalance callbacks
- `CancellationToken`: Graceful shutdown signaling
- Used by: Python code calling `consumer.start()`

**`src/produce.rs` (Producer Implementation):**
- `PyProducer`: Python-facing producer class with `#[pyclass]`
- `FutureProducer`: `rdkafka::producer::FutureProducer` wrapped in `Arc<RwLock<Option<_>>>`
- Methods: `init()`, `send()` (async), `send_sync()`, `flush()`, `in_flight_count()`
- Used by: Python code calling `producer.init()` and `producer.send()`

**`src/kafka_message.rs` (Domain Object):**
- `KafkaMessage`: Domain representation of Kafka message
- Fields: topic, partition, offset, key, payload, headers
- `from_rdkafka_message()`: Constructor from `rdkafka::message::BorrowedMessage`
- `#[pyclass]` with `#[pyo3(get)]` for Python property access
- Used by: Message routing and Python callback delivery

**`src/errors.rs` (Error Types):**
- `KafkaConsumerError`: Kafka-specific errors (KafkaError, Processing, Serialization)
- `DomainError`: Business errors (Validation, BusinessRule, NotFound, Conflict, Serialization, Database, Processing)
- Both derive `thiserror::Error` for `#[error()]` formatting
- Used by: All layers via `?` operator

**`src/logging.rs` (Logging Setup):**
- `Logger::init()`: Initializes `tracing_subscriber::fmt()` with environment filter
- Filter default: `"info,consumer=debug"` if `RUST_LOG` not set
- Format: line numbers, thread IDs, file names enabled
- Used by: `lib.rs` module initialization

**`src/message_processor.rs` (Routing):**
- `MessageProcessor` trait: `async_trait` with `process_message()`, `supported_topic()`, `processor_name()`
- `PyProcessor`: Wraps Python callback `Py<PyAny>` implementing `MessageProcessor`
- `MessageRouter`: Registry of processors, routes messages by topic, supports wildcard `"*"`
- Used by: `PyConsumer::start()` via `message_router`

**`kafpy/__init__.py` (Python Public API):**
```python
# Re-exports from _kafpy:
# - ConsumerConfig, ProducerConfig (configuration)
# - KafkaMessage (message object)
# - Consumer, Producer (main classes)
# Note: Renamed PyConsumer -> Consumer, PyProducer -> Producer
```

## Naming Conventions

**Rust Files:**
- Pattern: `snake_case.rs` matching the module name
- Example: `message_processor.rs` contains `MessageProcessor` trait and related types

**Rust Types:**
- `PascalCase`: Structs, enums, traits (`PyConsumer`, `KafkaMessage`, `MessageProcessor`)
- `snake_case`: Functions, methods, variables (`create_producer`, `start_consuming`)
- `SCREAMING_SNAKE_CASE`: Constants (not heavily used in this codebase)

**Python Files:**
- Standard Python: `snake_case.py` (`__init__.py`)

**Python Classes:**
- Remapped via `#[pyclass(name = "...")]`: `PyConsumer` exposed as `Consumer`, `PyProducer` as `Producer`

## Where to Add New Code

**New Feature (Consumer-side):**
- Implementation: `src/consume.rs` (extend `KafkaManager` or add new method to `PyConsumer`)
- Python API: `kafpy/__init__.py` (if adding new public class)
- Testing: Add to consumer example or create test script

**New Feature (Producer-side):**
- Implementation: `src/produce.rs` (extend `PyProducer` or add new producer type)
- Python API: `kafpy/__init__.py`
- Testing: Add to producer example or create test script

**New Message Processor:**
- Implementation: `src/message_processor.rs` (add new `impl MessageProcessor for ...`)
- Registration: `PyConsumer::add_handler()` already supports dynamic registration

**New Configuration Option:**
- Config struct: `src/config.rs` (add field to `ConsumerConfig` or `ProducerConfig`)
- PyO3 binding: Add `#[pyo3(signature = (...))]` parameter and `#[pymethods]`
- Kafka mapping: Add `.set()` call in `KafkaManager::create_consumer()` or `PyProducer::create_producer()`

**New Error Type:**
- Location: `src/errors.rs` (add variant to `KafkaConsumerError` or `DomainError`)
- Conversion: Add `.map_err()` in async context or use `?` operator

**Utilities/Helpers:**
- Location: `src/` (create new `util.rs` or add to existing module)
- If Python-accessible: Add `#[pyclass]` and register in `lib.rs`

## Special Directories

**`target/` (Generated):**
- Purpose: Cargo build output including compiled native extension
- Generated: Yes (by `cargo build` or `cargo maturin develop`)
- Committed: No (in `.gitignore`)
- Note: `_kafpy*.so` or `_kafpy*.dylib` is the actual Python importable module

**`.github/` (CI/CD):**
- Purpose: GitHub Actions workflows
- Contains: CI configuration
- Committed: Yes

**`.planning/` (GSD Framework):**
- Purpose: Architecture and planning documentation for GSD workflow
- Contains: ARCHITECTURE.md, STRUCTURE.md, STACK.md, etc.
- Generated: Yes (by GSD agents like this one)
- Committed: Yes

## Cargo.toml Structure

```toml
[package]
name = "KafPy"
version = "0.1.0"
edition = "2021"

[lib]
name = "_kafpy"              # Python extension module name
crate-type = ["cdylib"]      # Compiles to C dynamic library for Python

[dependencies]
pyo3 = { version = "0.27.2", features = ["extension-module", "generate-import-lib"] }
pyo3-async-runtimes = { version = "0.27.0", features = ["tokio-runtime"] }
tokio = { version = "1.40", features = ["full"] }
rdkafka = { version = "0.38" }  # Platform-specific (unix vs windows with cmake)
# ... other dependencies

[target.'cfg(unix)'.dependencies]
rdkafka = { version = "0.38" }  # Standard build on Unix

[target.'cfg(windows)'.dependencies]
rdkafka = { version = "0.38", features = ["cmake-build"] }  # CMake on Windows
```

## Build Artifacts

| Artifact | Location | Purpose |
|----------|----------|---------|
| `_kafpy.{so,dylib}` | `target/debug/` or `target/release/` | Python importable native module |
| `_kafpy.pyd` | Windows equivalent | Windows Python extension |
| `Cargo.lock` | Project root | Dependency lockfile (committed) |
| `target/` | Project root | All build artifacts (not committed) |

---

*Structure analysis: 2026-04-15*
