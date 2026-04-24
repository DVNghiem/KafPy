# Development Setup

This guide covers setting up your development environment for KafPy.

## Prerequisites

- **Rust 1.75+** — Required for PyO3 and async traits
- **Python 3.10+** — Required for type hints and native types
- **Kafka broker** — For integration testing

## Initial Setup

### 1. Clone and Install

```bash
git clone https://github.com/dangvansam/kafpy.git
cd kafpy
pip install -e ".[dev]"
```

### 2. Install Development Dependencies

```bash
pip install pytest pytest-asyncio maturin pytest-mock
```

### 3. Build the Project

```bash
maturin develop
```

### 4. Run Tests

```bash
pytest tests/ -v
```

## Project Structure

```
KafPy/
├── src/                 # Rust source code (PyO3 module)
│   ├── lib.rs          # Module root + Send+Sync assertions
│   ├── consumer/       # Kafka consumer implementation
│   ├── dispatcher/     # Message routing to handlers
│   ├── worker_pool/    # Tokio worker tasks
│   ├── routing/        # Handler routing chain
│   ├── python/         # Python callback execution
│   ├── offset/         # Offset tracking + commit
│   ├── shutdown/       # Graceful shutdown
│   ├── retry/          # Retry scheduling
│   ├── dlq/            # Dead letter queue
│   ├── failure/         # Failure classification
│   ├── observability/   # Metrics + tracing
│   └── runtime/        # Runtime assembly
├── kafpy/              # Python package
├── docs/               # MkDocs documentation
├── tests/              # Python integration tests
└── src/benchmarks/     # Rust benchmarks
```

## Coding Standards

See [Conventions](conventions.md) for detailed Rust and PyO3 patterns.

## Running Documentation Locally

```bash
mkdocs serve
```

Then open `http://localhost:8000`

## Testing

```bash
# Unit tests
cargo test

# Integration tests
pytest tests/integration/ -v

# Full test suite
cargo test && pytest tests/
```

## Debugging

### Enable trace logging:

```python
import logging
logging.basicConfig(level=logging.DEBUG)
```

### Rust debug builds:

```bash
RUST_LOG=debug cargo run --example basic_consumer
```