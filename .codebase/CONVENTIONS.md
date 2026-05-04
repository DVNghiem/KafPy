# KafPy Conventions

## Naming

### Rust
- **Modules**: `snake_case` (e.g., `worker_pool`, `offset_tracker`)
- **Types/Structs**: `PascalCase` (e.g., `ConsumerConfig`, `DispatchOutcome`)
- **Enums**: `PascalCase` variants (e.g., `AutoOffsetReset::Latest`, `ShutdownPhase::Draining`)
- **Functions/Methods**: `snake_case` (e.g., `register_handler`, `try_send`)
- **Constants**: `SCREAMING_SNAKE_CASE` (e.g., `MAX_FAN_OUT_DEGREE`)
- **PyO3-exposed types**: Prefixed with `Py` when distinct from Rust-only (e.g., `PyConsumer`, `PyRetryPolicy`, `PyObservabilityConfig`)
- **Builder pattern**: `XxxBuilder` with chained setter methods, returning `Result<Xxx, BuildError>`

### Python
- **Classes**: `PascalCase` (e.g., `ConsumerConfig`, `KafkaMessage`, `HandlerResult`)
- **Functions/Methods**: `snake_case` (e.g., `register_handler`, `to_rust`, `run_scenario`)
- **Constants**: `SCREAMING_SNAKE_CASE` (e.g., `ROUTING_MODES`, `AUTO_OFFSET_RESET_VALUES`)
- **Private attributes**: Leading underscore (e.g., `_consumer`, `_handlers`, `_stopping`)
- **Frozen dataclasses**: All config and value classes use `@dataclass(frozen=True)`

### Design Decision IDs
- Decision identifiers follow `D-XX` format (e.g., D-02 handler type detection, D-08 silent None, D-10 error format)
- Requirement identifiers follow `ERR-XX`, `DISP-XX` format per feature area

## File Organization

- **Rust**: One primary type per file (e.g., `offset_tracker.rs` contains `OffsetTracker`)
- **Python**: One concept per file (e.g., `config.py` for all config classes, `handlers.py` for handler types)
- **Module `mod.rs`**: Contains doc comments explaining the module's purpose and re-exports

## Import Style

### Rust
- Grouped imports by crate: `use pyo3::`, `use tokio::`, `use crate::`
- Re-exports in `mod.rs` with `pub use`
- Internal-only modules: `pub(crate) mod`

### Python
- `from __future__ import annotations` at top of every file
- `__all__` list in every module
- Conditional Rust extension import: `try: from ._kafpy import ... except ModuleNotFoundError: ...`

## Error Handling

### Rust
- `thiserror` derive for error types (e.g., `#[derive(Debug, thiserror::Error)]`)
- `anyhow` for internal error propagation
- `ConsumerError`, `DispatchError`, `CoordinatorError` — each domain has its own error type
- Builder pattern returns `Result<Config, BuildError>` for validation errors

### Python
- `KafPyError(Exception)` as base with `@dataclass(frozen=True)` — unusual but intentional for structured context
- Exception hierarchy: `KafPyError` → `ConsumerError`, `HandlerError`, `ConfigurationError`
- Rust errors translated to Python via `translate_rust_error()` parsing D-10 format strings
- `HandlerError` raised for wrong-type access (D-09), silent None returns for absent values (D-08)

## Config Pattern

- Python validates in `__post_init__` (frozen dataclass)
- `ConsumerConfig.to_rust()` converts Python config to Rust PyO3 types
- Rust `ConsumerConfigBuilder` provides builder pattern with required fields validation
- All new config fields default to `None` for backward compatibility

## Testing Patterns

### Rust
- `#[cfg(test)]` module within source files for unit tests
- `tests/` directory for integration tests
- Compile-time Send+Sync assertions for routing and dispatcher types
- Design decision references in test comments (e.g., `DISP-01`, `ERR-03`)

### Python
- `pytest` with class-based test organization
- Test class names follow `TestFeatureArea` pattern
- Test method names use `test_what_condition_expectation` pattern

## Threading Model

- Tokio runtime manages all async Rust operations
- Python GIL acquired only during handler callback invocation
- Worker pool: N Tokio workers polling per-topic `mpsc` channels
- Python callbacks executed via `pyo3-async-runtimes` bridge
- Fan-out: parallel sink invocation with configurable concurrency (max 64)

## Key Patterns to Follow

1. **Always use `@dataclass(frozen=True)`** for Python value types — immutability by default
2. **Builder pattern for complex Rust types** — `ConsumerConfigBuilder`, `ProducerConfigBuilder`
3. **Bounded channels everywhere** — `mpsc::channel` with explicit capacity, never unbounded
4. **Non-blocking dispatch** — `try_send` returns `Backpressure` error, never blocks
5. **Offset commit via highest-contiguous algorithm** — never skip offsets on partial batch failure
6. **Middleware chain** — `before()` → handler → `after()` / `on_error()` pattern
7. **Phase references** — Decisions documented as D-XX, requirements as DISP-XX, ERR-XX, etc.