# KafPy Testing

## Test Frameworks

### Rust Tests
- **In-tree**: `#[cfg(test)] mod tests` within source files
- **Integration**: `tests/` directory (e.g., `builder_test.rs`, `dispatcher_test.rs`)
- **Assertions**: Standard `assert!`, `assert_eq!`, `assert!` with `matches!` macro
- **Async**: Tokio test runtime (when needed)

### Python Tests
- **Framework**: `pytest`
- **Location**: `tests/test_exceptions.py`
- **Pattern**: Class-based organization (`TestXxx`), method names `test_what_condition_expectation`

## Running Tests

```bash
# Build Rust extension first
maturin develop

# Run Python tests
pytest tests/

# Run specific Python test
pytest tests/test_exceptions.py::TestExceptionHierarchy::test_exceptions_module_exports

# Run Rust tests
cargo test

# Run specific Rust test
cargo test -- consumer_builder_requires_brokers

# Run Rust tests for a specific module
cargo test -- dispatcher
```

## Test Coverage Areas

### Rust (Integration Tests)
| Test File | Area Covered | Design References |
|-----------|-------------|-----------------|
| `builder_test.rs` | ConsumerConfigBuilder validation, ProducerConfigBuilder | Required field validation |
| `dispatcher_test.rs` | Dispatcher routing, backpressure, QueueManager, error variants | DISP-01 through DISP-20 |

### Python
| Test File | Area Covered | Design References |
|-----------|-------------|-----------------|
| `test_exceptions.py` | Exception hierarchy, inheritance, structured attributes, D-10 format, Rust→Python translation | ERR-01 through ERR-05 |

### Rust (In-tree Unit Tests)
- `src/lib.rs` — Send+Sync assertions for routing and dispatcher types
- `src/failure/tests.rs` — Failure classification unit tests
- Various modules contain `#[cfg(test)]` inline tests

## Test Design Principles

1. **Design decision references**: Test comments reference D-XX / ERR-XX / DISP-XX decisions
2. **Compile-time guarantees**: Send+Sync assertions are compile-time tests, not runtime
3. **PyO3 boundary testing**: Python tests verify Rust→Python type translation, not Rust internals
4. **Frozen dataclass testing**: Verify immutability and `__post_init__` validation
5. **Error format testing**: D-10 format strings parsed and verified in Python tests

## Test Gaps

- **No integration tests with live Kafka**: Tests mock or use internal APIs; no containerized Kafka integration test suite
- **No benchmark tests in CI**: Benchmark infrastructure exists but no CI pipeline
- **Limited Python-side handler testing**: Only exception tests exist; no consumer E2E tests
- **No concurrency tests**: Worker pool and fan-out have no concurrent test coverage
- **No DLQ / retry integration tests**: RetryCoordinator and DlqRouter lack test files