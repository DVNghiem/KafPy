# Contributing

KafPy is a Python library built with Rust (PyO3). This guide covers everything you need to contribute.

## Prerequisites

- Python 3.11+
- Rust toolchain (`rustup`)
- `librdkafka` system library

### Installing librdkafka

```bash
# macOS
brew install librdkafka

# Ubuntu / Debian
apt install librdkafka-dev

# Fedora
dn install librdkafka-devel
```

## Development Setup

1. **Clone the repository:**

```bash
git clone https://github.com/DVNghiem/KafPy.git
cd KafPy
```

2. **Create a virtual environment:**

```bash
python -m venv .venv
source .venv/bin/activate  # macOS/Linux
# .venv\Scripts\activate  # Windows
```

3. **Install maturin:**

```bash
pip install maturin
```

4. **Build and install the package:**

```bash
maturin develop --release
```

## Running Tests

```bash
# Run all Python tests
pytest

# Run a specific test file
pytest tests/test_exceptions.py

# Run with verbose output
pytest -v

# Run tests matching a pattern
pytest -k "test_consumer"
```

## Code Style

### Python

- Type annotations on all public functions
- f-strings for string formatting
- snake_case for variables and functions
- PascalCase for classes
- Docstrings on all public functions using Google style

```python
def process_message(msg: KafkaMessage, ctx: HandlerContext) -> HandlerResult:
    """Process a single Kafka message.

    Args:
        msg: The incoming Kafka message.
        ctx: Handler context with topic/partition metadata.

    Returns:
        HandlerResult indicating ack/nack action.

    Raises:
        HandlerError: If message processing fails.
    """
    ...
```

### Rust

- Follow standard Rust idioms (rustfmt, clippy)
- Document all public items with `///` doc comments
- Use `thiserror` for error types

```rust
/// Consumes messages from a single topic partition.
pub struct PartitionConsumer { ... }
```

### Test Naming

Use descriptive test names that explain the expected behavior:

```python
def test_consumer_error_str_with_topic_partition(self):
    """ConsumerError.__str__ includes topic@partition when both are present."""
    ...

def test_kafka_message_wrong_type_raises_handler_error(self):
    """Wrong-type access raises HandlerError, not raw Rust panics."""
    ...
```

## Submitting Changes

1. **Fork the repository** on GitHub.

2. **Create a feature branch:**

```bash
git checkout -b feature/your-feature-name
# or
git checkout -b fix/your-bug-fix
```

3. **Make your changes.** Run tests to ensure nothing is broken:

```bash
pytest -v
```

4. **Commit using conventional commits:**

```bash
git commit -m "feat(consumer): add batch message processing"
git commit -m "fix(exceptions): correct error string format"
git commit -m "test(handlers): add coverage for async handlers"
```

5. **Push and create a pull request** on GitHub.

## Reporting Issues

### Bug Reports

Include:
- Python and Rust versions (`python --version`, `rustc --version`)
- librdkafka version
- Minimal reproducible example
- Full error traceback

### Feature Requests

Describe the problem you're solving and your proposed solution. Check existing issues first to avoid duplicates.

## Project Structure

```
KafPy/
├── kafpy/              # Python package source
│   ├── __init__.py     # Public API exports
│   ├── config.py       # ConsumerConfig
│   ├── consumer.py     # Consumer
│   ├── exceptions.py   # Exception hierarchy
│   ├── handlers.py     # KafkaMessage, HandlerContext
│   └── runtime.py      # KafPy app runtime
├── src/                # Rust source (PyO3 bindings)
├── tests/              # Python tests
└── Cargo.toml          # Rust dependencies
```
