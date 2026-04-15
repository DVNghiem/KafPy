# Technology Stack

**Analysis Date:** 2026-04-15

## Languages

**Primary:**
- Rust (latest) - Core library implementation via `cdylib` crate type in `Cargo.toml`
- Python 3.11+ - Public API and user-facing interface defined in `kafpy/__init__.py`

**Secondary:**
- Python (packaging) - Build configuration in `pyproject.toml` and `Cargo.toml`

## Runtime

**Environment:**
- Python 3.11, 3.12, 3.13, 3.14 (CPython implementation)
- Rust toolchain (latest stable recommended)

**Package Manager:**
- maturin (Python-Rust build tool)
- Cargo (Rust dependency management)
- Lockfile: `Cargo.lock` present

## Frameworks

**Core:**
- **PyO3** v0.27.2 - Rust-Python bindings for creating native Python extensions
  - `extension-module` feature: Allows the crate to be used as an extension module
  - `generate-import-lib`: Generates import library for linking
  - Configured in `[lib]` section of `Cargo.toml` with `crate-type = ["cdylib"]`
  - Module name: `_kafpy` (private implementation)
  - Python package name: `kafpy` (public interface in `kafpy/__init__.py`)

- **rdkafka** v0.38 - Rust bindings for librdkafka
  - Unix: Standard build via system librdkafka
  - Windows: `cmake-build` feature for CMake-based build
  - Handles all Kafka protocol communication
  - Provides `ClientConfig`, `StreamConsumer`, `FutureProducer`

**Async Runtime:**
- **tokio** v1.40 - Async runtime for Rust
  - `full` feature: Enables all async capabilities
  - Powers async message consumption and production
  - Used in `consume.rs` with `StreamConsumer` for message streaming
  - Used in `produce.rs` with `FutureProducer` for async message sending

- **tokio-util** v0.7.17 - Utilities for tokio
  - `CancellationToken` for graceful shutdown handling
  - Used in `consume.rs` for cancellation support

- **pyo3-async-runtimes** v0.27.0 - Python async integration for PyO3
  - `tokio-runtime` feature: Integrates PyO3 async with tokio runtime
  - Enables `future_into_py` for converting Rust futures to Python coroutines
  - Used in `consume.rs` and `produce.rs` to bridge async boundaries

**Serialization:**
- Built-in rdkafka serialization (no additional crates needed)
- Message payloads accessed via `payload_view::<str>()` in `kafka_message.rs`

## Key Dependencies

**Critical (Core Functionality):**

| Package | Version | Purpose |
|---------|---------|---------|
| pyo3 | 0.27.2 | Python-Rust FFI, creates native extension module |
| pyo3-async-runtimes | 0.27.0 | Bridges Python asyncio with Rust async |
| rdkafka | 0.38 | Kafka protocol implementation |
| tokio | 1.40 | Async runtime for message processing |
| tokio-util | 0.7.17 | CancellationToken and utilities |

**Infrastructure:**

| Package | Version | Purpose |
|---------|---------|---------|
| async-trait | 0.1 | Adds `async fn` support to trait methods |
| dotenvy | 0.15.7 | Loads environment variables from `.env` files |
| tracing | 0.1.44 | Structured logging framework |
| tracing-subscriber | 0.3.22 | Logging output formatting with env filter |
| thiserror | 2.0.17 | Error type derivation with `#[derive(Error)]` |
| num_cpus | 1.16 | CPU core detection for performance tuning |

**Build Dependencies:**

| Package | Version | Purpose |
|---------|---------|---------|
| pyo3-build-config | =0.27.2 | Build-time PyO3 configuration |

## Configuration

**Build System:**
- **maturin** >=1.0 <2.0 - Python-Rust build backend
  - Configured in `pyproject.toml` `[build-system]` section
  - Module name: `kafpy._kafpy`
  - Bindings: `pyo3`
  - Source path: `kafpy/` directory

**Python Package Metadata (`pyproject.toml`):**
```toml
[project]
name = "kafpy"
version = "0.1.0"
requires-python = ">=3.11"
classifiers = [
    "Programming Language :: Python :: 3.11",
    "Programming Language :: Python :: 3.12",
    "Programming Language :: Python :: 3.13",
    "Programming Language :: Python :: 3.14",
    "Programming Language :: Rust",
    "Framework :: AsyncIO",
]
```

**Rust Build Profiles (`Cargo.toml`):**
```toml
[profile.release]
opt-level = 3       # Maximum optimization
lto = true           # Link-time optimization
codegen-units = 1    # Single codegen unit for better optimization
panic = "abort"      # Abort on panic (smaller binaries)
strip = true         # Strip symbols
debug = true         # Include debug info (for crash reports)

[profile.dev]
opt-level = 0        # No optimization for dev builds
debug = true         # Debug symbols
```

## Platform Requirements

**Development:**
- Python 3.11+
- Rust toolchain (stable)
- maturin (`pip install maturin`)
- librdkafka development libraries (for Unix)
- CMake (for Windows rdkafka build)

**Production:**
- Python 3.11+ runtime
- Pre-built wheel from PyPI (contains compiled Rust extension)
- librdkafka runtime library

## Architecture Overview

The project follows a Python extension architecture where:

1. **Rust Core (`src/`):** Implements Kafka consumer and producer logic
   - `lib.rs`: Module entry point, exports PyO3 module `_kafpy`
   - `consume.rs`: `PyConsumer` class - async message consumption
   - `produce.rs`: `PyProducer` class - async message production
   - `config.rs`: `ConsumerConfig` and `ProducerConfig` - configuration structs
   - `kafka_message.rs`: `KafkaMessage` - message representation
   - `message_processor.rs`: `MessageProcessor` trait, `PyProcessor`, `MessageRouter`
   - `logging.rs`: `Logger` - tracing subscriber initialization
   - `errors.rs`: Error types via `thiserror`

2. **Python Public API (`kafpy/`):** Thin wrapper exposing public interface
   - `__init__.py`: Imports from `_kafpy`, re-exports public classes
   - Package name: `kafpy`
   - Public exports: `Consumer`, `ConsumerConfig`, `Producer`, `ProducerConfig`, `KafkaMessage`

3. **Build Output:**
   - Library name: `_kafpy.cdylib` (or platform equivalent)
   - Installed as `kafpy._kafpy` module

## Code Organization

```
/home/nghiem/project/KafPy/
├── Cargo.toml              # Rust dependencies and build config
├── pyproject.toml          # Python package metadata and build config
├── Cargo.lock              # Rust dependency lock file
├── src/
│   ├── lib.rs              # PyO3 module entry point (_kafpy)
│   ├── consume.rs           # PyConsumer implementation
│   ├── produce.rs          # PyProducer implementation
│   ├── config.rs           # ConsumerConfig, ProducerConfig (with from_env)
│   ├── kafka_message.rs    # KafkaMessage Python class
│   ├── message_processor.rs # MessageProcessor trait and PyProcessor
│   ├── logging.rs          # Logger initialization
│   └── errors.rs           # KafkaConsumerError, DomainError types
└── kafpy/
    └── __init__.py         # Python public API re-exports
```

---

*Stack analysis: 2026-04-15*
