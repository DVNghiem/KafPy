# Testing Patterns

**Analysis Date:** 2026-04-15

## Overview

KafPy is a PyO3-based Python extension written in Rust. Testing strategy spans both Rust unit tests and Python integration tests. Currently, **no tests exist** in the codebase, making test infrastructure setup a priority.

## Build and Test Tooling

### maturin

Maturin is the build tool for Python extensions with Rust bindings:

```bash
# Build and test in development
maturin develop

# Run tests with maturin
maturin test

# Build release wheels
maturin build --release
```

Maturin automatically:
- Compiles the Rust extension
- Installs the package in the current Python environment
- Runs pytest if tests are present

### maturin Configuration

```toml
# pyproject.toml
[build-system]
requires = ["maturin>=1.0,<2.0"]
build-backend = "maturin"

[tool.maturin]
module-name = 'kafpy._kafpy'
bindings = 'pyo3'
```

## Test File Locations

### Rust Unit Tests (recommended)

Unit tests should be placed in `#[cfg(test)]` modules within source files:

```
src/
├── lib.rs
├── config.rs      # Tests: #[cfg(test)] mod tests { ... }
├── consume.rs    # Tests: #[cfg(test)] mod tests { ... }
├── errors.rs     # Tests: #[cfg(test)] mod tests { ... }
└── ...
```

Example unit test structure:

```rust
// src/config.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consumer_config_default_values() {
        let config = ConsumerConfig::new(
            "localhost:9092".to_string(),
            "test-group".to_string(),
            vec!["test-topic".to_string()],
            // All other params use defaults from signature
        );
        assert_eq!(config.auto_offset_reset, "earliest");
        assert!(!config.enable_auto_commit);
    }

    #[test]
    fn consumer_config_from_env_missing_vars() {
        // Clear and test environment variable handling
        std::env::remove_var("KAFKA_BROKERS");
        let config = ConsumerConfig::from_env().unwrap();
        assert_eq!(config.brokers, "localhost:9092");
    }
}
```

### Integration Tests (Python/pytest)

Integration tests go in the `tests/` directory at project root:

```
KafPy/
├── Cargo.toml
├── pyproject.toml
├── kafpy/                 # Python package source
│   ├── __init__.py
│   └── _kafpy.pyi
├── tests/                 # Python integration tests
│   ├── __init__.py
│   ├── conftest.py        # pytest fixtures
│   ├── test_consumer.py
│   └── test_producer.py
└── src/                   # Rust source
```

## Test Framework Patterns

### Rust Test Infrastructure

For Rust unit tests, use:

- `#[cfg(test)] mod tests { ... }` for inline tests
- `#[test]` attribute for individual tests
- `#[tokio::test]` for async tests (with `tokio` as dev dependency)
- `mockall` for mocking traits in tests

Cargo.toml should include test dependencies:

```toml
[dev-dependencies]
tokio = { version = "1.40", features = ["full"] }
mockall = "0.12"
rstest = "0.19"
```

### PyO3 Testing Considerations

When testing PyO3-exposed types:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn kafka_message_from_rdkafka() {
        // Test the conversion logic
        // KafkaMessage::from_rdkafka_message converts rdkafka types
    }
}
```

### Python pytest Fixtures

```python
# tests/conftest.py
import pytest
from kafpy import ConsumerConfig, ProducerConfig, Consumer, Producer

@pytest.fixture
def consumer_config():
    return ConsumerConfig(
        brokers="localhost:9092",
        group_id="test-group",
        topics=["test-topic"],
    )

@pytest.fixture
def producer_config():
    return ProducerConfig(
        brokers="localhost:9092",
    )
```

### Testing Async Python Methods

Since `Consumer.start()` and `Producer.init()` return coroutines:

```python
# tests/test_consumer.py
import pytest
import asyncio

@pytest.mark.asyncio
async def test_consumer_start_without_handlers():
    from kafpy import Consumer, ConsumerConfig

    config = ConsumerConfig(
        brokers="localhost:9092",
        group_id="test-group",
        topics=["test-topic"],
    )
    consumer = Consumer(config)

    # Should complete without error (no handlers registered)
    # Note: Will hang without Kafka broker - use mock or timeout
    await asyncio.wait_for(consumer.start(), timeout=1.0)
```

### Mocking rdkafka for Tests

For unit tests without a real Kafka broker:

```python
# tests/conftest.py
@pytest.fixture
def mock_kafka():
    """Fixture for tests requiring Kafka mocking."""
    # For now, mark tests that need real Kafka
    # Later: use testcontainers or mock
    pytest.skip("Requires Kafka broker")
```

## CI Integration

### maturin test in CI

The release workflow at `.github/workflows/release.yml` uses `PyO3/maturin-action` for building. Testing can be added:

```yaml
# Add to existing release workflow or create test workflow
- name: Run tests
  run: |
    maturin test
  env:
    MATURIN_DEFAULT_TOOLCHAIN: stable
```

### Cross-Platform Testing Considerations

The project builds for multiple platforms:
- Linux (x86_64, x86, aarch64)
- musllinux
- Windows (x64, x86)
- macOS (x86_64, aarch64)

Tests should be platform-aware. Consider:
- Skipping integration tests on platforms without Docker
- Using Docker Compose for Linux/macOS integration tests
- Windows: skip or mock Kafka-dependent tests

### GitHub Actions Test Matrix

```yaml
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-python@v5
        with:
          python-version: "3.12"
      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
      - name: Install maturin
        run: pip install maturin
      - name: Run unit tests
        run: cargo test --lib
      - name: Run integration tests
        run: maturin test
```

## Coverage

### Rust Coverage with cargo-llvm-cov

```bash
# Install cargo-llvm-cov
cargo install cargo-llvm-cov

# Run with coverage
cargo llvm-cov --html
cargo llvm-cov --fail-under-lines 80
```

### Python Coverage with pytest-cov

```bash
pytest --cov=kafpy --cov-report=term-missing
```

## Recommended Test Organization

```
KafPy/
├── Cargo.toml
├── pyproject.toml
├── tests/
│   ├── __init__.py
│   ├── conftest.py
│   ├── test_consumer.py
│   ├── test_producer.py
│   ├── test_config.py
│   └── test_message.py
├── src/
│   ├── lib.rs
│   ├── config.rs       # #[cfg(test)] mod tests { ... }
│   ├── consume.rs      # #[cfg(test)] mod tests { ... }
│   ├── produce.rs      # #[cfg(test)] mod tests { ... }
│   └── ...
```

## Key Testing Patterns to Implement

### 1. Config Validation Tests (Rust)

```rust
#[cfg(test)]
mod config_tests {
    #[test]
    fn rejects_empty_brokers() {
        // Test that empty string brokers causes error
    }

    #[test]
    fn validates_auto_offset_reset() {
        // Valid values: earliest, latest
    }
}
```

### 2. Message Conversion Tests (Rust)

```rust
#[cfg(test)]
mod message_tests {
    #[test]
    fn kafka_message_payload_to_string() {
        // Test payload conversion from Option<String>
    }
}
```

### 3. Consumer Integration Tests (Python)

```python
# tests/test_consumer.py
import pytest

def test_consumer_initialization():
    config = ConsumerConfig(
        brokers="localhost:9092",
        group_id="test-group",
        topics=["test-topic"],
    )
    consumer = Consumer(config)
    assert consumer.config.brokers == "localhost:9092"

def test_add_handler():
    consumer = Consumer(consumer_config())
    def handler(msg):
        pass
    consumer.add_handler("test-topic", handler)
    consumer.stop()  # Clean up
```

### 4. Producer Integration Tests (Python)

```python
# tests/test_producer.py
import pytest

@pytest.mark.asyncio
async def test_producer_init():
    config = ProducerConfig(brokers="localhost:9092")
    producer = Producer(config)
    await producer.init()
    assert producer.in_flight_count() >= 0
```

## Testing Infrastructure Gaps

1. **No existing tests** - First priority is establishing test infrastructure
2. **No Kafka mock** - Integration tests require real Kafka or testcontainers
3. **No coverage enforcement** - Add 80% coverage requirement
4. **No pytest configuration** - Add `pytest.ini` or `pyproject.toml` pytest config

## Next Steps for Testing

1. Add `#[cfg(test)]` modules to existing source files
2. Add `tests/` directory with conftest.py
3. Add pytest configuration to `pyproject.toml`
4. Add Rust test dependencies to `Cargo.toml`
5. Add GitHub Actions workflow for running tests
6. Consider testcontainers for integration tests

---

*Testing analysis: 2026-04-15*