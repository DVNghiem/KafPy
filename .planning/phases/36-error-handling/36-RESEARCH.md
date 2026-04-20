# Phase 36: Error Handling - Research

**Researched:** 2026-04-20
**Domain:** Python exception hierarchy, Rust-to-Python error translation at PyO3 boundary, meaningful Python exceptions, KafkaMessage access errors
**Confidence:** HIGH

## Summary

Phase 36 implements the public exception API for KafPy v1.8. The Rust side already has error types (`PyError` in `src/errors.rs`, `ConsumerError` in `src/consumer/error.rs`, `DispatchError`, `CoordinatorError`). The Python side has stub imports in `kafpy/__init__.py` with a try/except that passes when `exceptions` is absent. The primary deliverable is `kafpy/exceptions.py` with the full Python exception hierarchy, plus wiring at the PyO3 boundary to translate Rust errors to Python exceptions with context-preserving messages. The `KafkaMessage` access pattern (D-08: `.key` returns None when absent; D-09: wrong-type access raises `HandlerError`) applies to Python-level `KafkaMessage` (currently in `_kafpy.pyi` stub and passed as `PyDict` to handlers in `python/handler.rs`).

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Python exception classes | Python (`kafpy/exceptions.py`) | — | Pure Python types, frozen dataclass pattern |
| Rust-to-Python error translation | PyO3 boundary (`lib.rs`, `pyconsumer.rs`) | — | Translates Rust errors to Python at the FFI boundary |
| KafkaMessage access errors | Python (`kafpy/handlers.py`) | — | Python-level validation when accessing message fields |
| Exception imports/public API | Python (`kafpy/__init__.py`) | — | ERR-05: only `kafpy.exceptions` leaks exceptions |
| Error message format | Python + Rust | — | Structured strings include Kafka error code, topic, partition |

## User Constraints (from CONTEXT.md)

### Locked Decisions
- **D-01:** Exception classes inherit from `KafPyError`, not `Exception` directly
- **D-02:** Subclasses carry structured attributes: `error_code` (Kafka error code), `partition`, `topic` where relevant
- **D-03:** Class hierarchy: `KafPyError` → `ConsumerError`, `HandlerError`, `ConfigurationError`
- **D-04:** All exceptions exported from `kafpy.exceptions` only (ERR-05)
- **D-05:** PyO3 boundary catches Rust errors and maps to Python exceptions
- **D-06:** Context preserved: Kafka error code + source module + formatted message
- **D-07:** Each Rust error type maps to specific Python exception (`KafkaError` → `ConsumerError`, etc.)
- **D-08:** Accessing `.key` when None returns `None` silently — no exception raised
- **D-09:** Wrong-type access (e.g., accessing wrong type field) raises `HandlerError`
- **D-10:** Human-readable structured strings: `"Consumer error: NOT_LEADER (error 6) on topic-0@partition 3"`
- **D-11:** Messages include Kafka error code, error name, topic, and partition where applicable

### Deferred Ideas
None.

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| ERR-01 | Python-side exceptions in `kafpy.exceptions`: `KafPyError`, `ConsumerError`, `HandlerError`, `ConfigurationError` | Section: Standard Stack, Architecture Patterns |
| ERR-02 | `KafPyError` is the base; framework errors inherit from it, not from `Exception` directly | Section: Architecture Patterns |
| ERR-03 | Rust errors translated to Python exceptions at the PyO3 boundary with context-preserving messages | Section: Rust-to-Python Translation Pattern |
| ERR-04 | `KafkaMessage` access errors (missing key, wrong type) raise `HandlerError`, not raw Rust panics | Section: KafkaMessage Access Errors |
| ERR-05 | `kafpy.exceptions` is the only public import path for exceptions; nothing from `_kafpy` or `kafpy._rust` leaks | Section: Public API Boundaries |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `rdkafka` | system | Kafka client library | Already in project |
| `pyo3` | 0.22+ | Rust-to-Python FFI | Already in project |
| `thiserror` | 1.x | Rust error derive | Already in project (used in existing `PyError`, `ConsumerError`, `DispatchError`, `CoordinatorError`) |

**No new Python packages required** — exception classes are pure Python.

### Existing Rust Error Types to Map

| Rust Error | File | Python Target |
|------------|------|---------------|
| `PyError::Consumer` | `src/errors.rs` | `ConsumerError` |
| `PyError::Producer` | `src/errors.rs` | `ConsumerError` (or new `ProducerError`) |
| `PyError::Config` | `src/errors.rs` | `ConfigurationError` |
| `ConsumerError::Kafka` | `src/consumer/error.rs` | `ConsumerError` (with error_code) |
| `ConsumerError::Subscription` | `src/consumer/error.rs` | `ConsumerError` |
| `ConsumerError::Receive` | `src/consumer/error.rs` | `ConsumerError` |
| `ConsumerError::Serialization` | `src/consumer/error.rs` | `ConsumerError` |
| `ConsumerError::Processing` | `src/consumer/error.rs` | `HandlerError` |
| `DispatchError::QueueFull` | `src/dispatcher/error.rs` | `ConsumerError` |
| `DispatchError::Backpressure` | `src/dispatcher/error.rs` | `ConsumerError` |
| `DispatchError::UnknownTopic` | `src/dispatcher/error.rs` | `ConsumerError` |
| `DispatchError::HandlerNotRegistered` | `src/dispatcher/error.rs` | `HandlerError` |
| `CoordinatorError` | `src/coordinator/error.rs` | `ConsumerError` |

## Architecture Patterns

### Exception Class Pattern
**What:** Frozen dataclass-based exception classes following the established project pattern.

```python
# Source: Based on existing frozen dataclass pattern in kafpy/config.py
from __future__ import annotations
from dataclasses import dataclass

__all__ = [
    "KafPyError",
    "ConsumerError",
    "HandlerError",
    "ConfigurationError",
]

@dataclass(frozen=True)
class KafPyError(Exception):
    """Base exception for all KafPy errors.

    All framework exceptions inherit from this, not from Exception directly.
    Carries structured context: error_code, partition, topic where applicable.
    """

    message: str
    error_code: int | None = None
    partition: int | None = None
    topic: str | None = None

    def __str__(self) -> str:
        return self.message

    def __repr__(self) -> str:
        parts = [f"message={self.message!r}"]
        if self.error_code is not None:
            parts.append(f"error_code={self.error_code}")
        if self.partition is not None:
            parts.append(f"partition={self.partition}")
        if self.topic is not None:
            parts.append(f"topic={self.topic!r}")
        return f"{type(self).__name__}({', '.join(parts)})"


@dataclass(frozen=True)
class ConsumerError(KafPyError):
    """Raised for consumer-level errors.

    Covers Kafka errors, subscription issues, message receive failures,
    and serialization errors.
    """

    def __str__(self) -> str:
        # D-10/D-11: structured format
        base = self.message
        if self.topic is not None and self.partition is not None:
            return f"{base} on {self.topic}@{self.partition}"
        return base


@dataclass(frozen=True)
class HandlerError(KafPyError):
    """Raised for handler processing errors.

    Covers Python handler exceptions, wrong-type message field access,
    and handler panics caught at the PyO3 boundary.
    """

    pass


@dataclass(frozen=True)
class ConfigurationError(KafPyError):
    """Raised for configuration errors.

    Covers invalid config values, missing required fields,
    and Rust config build failures.
    """

    pass
```

**Why:** Frozen dataclasses enforce immutability (project rule) and provide `__repr__` for free. Inheritance from `Exception` is explicit per D-01. `KafPyError` itself inherits from `Exception` (required for Python exception protocol), but `ConsumerError`, `HandlerError`, `ConfigurationError` inherit from `KafPyError` only (not `Exception` directly) per D-01/D-03.

### Anti-Patterns to Avoid
- **Inheriting `Exception` directly for subclasses:** All framework exceptions must inherit from `KafPyError` only, not `Exception` (D-01/D-03)
- **Raising bare Rust strings as Python exceptions:** All Rust errors must be mapped to typed Python exceptions with structured context (D-05/D-06)

### Rust-to-Python Translation Pattern (PyO3 Boundary)
**What:** Rust errors are caught at `PyErr::new()` calls in `pyconsumer.rs` and `lib.rs` and converted to typed Python exceptions.

**Location:** `src/pyconsumer.rs` already uses `PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string())` for `ConsumerRunner::new` failures. The pattern is established.

**Key insight from `src/errors.rs`:**
```rust
#[derive(Error, Debug)]
pub enum PyError {
    #[error("consumer error: {0}")]
    Consumer(String),
    #[error("producer error: {0}")]
    Producer(String),
    #[error("configuration error: {0}")]
    Config(String),
}
```

**Pattern to implement:**
```python
# In kafpy/exceptions.py — extend PyError mapping
RUST_TO_PYTHON: dict[str, type] = {
    "Consumer": ConsumerError,
    "Producer": ConsumerError,  # or ProducerError if added
    "Config": ConfigurationError,
}

def translate_rust_error(module: str, error_string: str) -> KafPyError:
    """Translate a Rust error string to the appropriate Python exception.

    Rust errors are logged with context-preserving messages (D-06/D-10).
    We parse the error string to extract error_code and topic/partition info.
    """
    # D-10 format: "Consumer error: NOT_LEADER (error 6) on topic-0@partition 3"
    # Parse error_code, topic, partition from the string
    import re
    # Extract Kafka error code: e.g., "NOT_LEADER (error 6)"
    code_match = re.search(r'\(error (\d+)\)', error_string)
    error_code = int(code_match.group(1)) if code_match else None
    # Extract topic@partition: e.g., "on topic-0@partition 3"
    topic_match = re.search(r'on ([^@]+)@partition (\d+)', error_string)
    topic = topic_match.group(1) if topic_match else None
    partition = int(topic_match.group(2)) if topic_match else None

    exc_type = RUST_TO_PYTHON.get(module, KafPyError)
    return exc_type(
        message=error_string,
        error_code=error_code,
        partition=partition,
        topic=topic,
    )
```

**Rust-side changes** (`src/errors.rs` or new `src/python/errors.rs`):
- Add structured fields to `PyError` variants so context is preserved before string formatting
- Extract `KafkaError` error code and topic/partition from `rdkafka::error::KafkaError`

**PyO3 module changes** (`src/lib.rs` or `src/pyconsumer.rs`):
- Instead of `PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string())`, use typed exceptions:
```rust
// Instead of:
.map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?

// Use:
.map_err(|e| {
    PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
        format!("Consumer error: {} (error {:?})", e, e.error_code())
    ))
})?
```

**Key insight:** The `PyError` enum uses `thiserror` with `#[error("...")]` format strings. The formatted string already contains the error name and code. The translation layer parses this string to reconstruct structured attributes. This preserves D-10/D-11 format while enabling programmatic access to error fields.

### KafkaMessage Access Errors Pattern
**What:** `KafkaMessage` (Python class from `kafpy/handlers.py`) raises `HandlerError` for wrong-type access.

**Current state:** `KafkaMessage` in Python (`kafpy/__init__.py` stub from `_kafpy.pyi`) is not yet a full Python class — messages are passed as `PyDict` to Python handlers in `src/python/handler.rs`.

**D-08/D-09 requirements:**
- `.key` returns `None` silently when key is absent (no exception)
- Wrong-type access raises `HandlerError`

**Implementation path:**
1. `KafkaMessage` in `kafpy/handlers.py` becomes a real frozen dataclass (not just stubs from `_kafpy.pyi`)
2. Properties like `.key` are methods or `@property` that return `None` when absent
3. Type-checking on access raises `HandlerError` for wrong-type access

```python
# In kafpy/handlers.py
@dataclass(frozen=True)
class KafkaMessage:
    """Kafka message with typed field access.

    Missing fields return None (D-08). Wrong-type access raises HandlerError (D-09).
    """

    topic: str
    partition: int
    offset: int
    key: bytes | None
    payload: bytes | None
    headers: list[tuple[str, bytes | None]]

    @property
    def key(self) -> bytes | None:
        """Return key or None if absent. Never raises."""
        return self.key  # Already bytes | None

    def get_key_as_string(self) -> str | None:
        """Decode key as UTF-8 string. Raises HandlerError if wrong type."""
        if self.key is None:
            return None
        try:
            return self.key.decode("utf-8")
        except UnicodeDecodeError as e:
            raise HandlerError(
                message=f"key is not valid UTF-8: {e}",
                error_code=None,
                partition=self.partition,
                topic=self.topic,
            ) from e

    # Similar pattern for payload, headers
```

**Important:** The `PythonHandler::invoke` in `src/python/handler.rs` passes a `PyDict` to Python, not a `KafkaMessage` instance. This means the Python `KafkaMessage` class needs to be constructed from that dict before calling the handler. Two options:
- Option A: Python-side constructor `KafkaMessage.from_dict(dict)` in `kafpy/handlers.py`
- Option B: PyO3 returns `KafkaMessage` instances instead of `PyDict`

Decision depends on whether the existing `PythonHandler.invoke` dict-passing pattern is changed or wrapped.

### Public API Boundaries (ERR-05)
**What:** Only `kafpy.exceptions` exposes exception classes. Internal modules (`_kafpy`, `kafpy._rust`) must not leak exceptions.

**Current `kafpy/__init__.py` structure:**
```python
try:
    from .exceptions import (
        KafPyError,
        ConsumerError,
        HandlerError,
        ConfigurationError,
    )
except ImportError:
    KafPyError = None
    ConsumerError = None
    HandlerError = None
    ConfigurationError = None
```

**After Phase 36:** Remove the try/except and add `from .exceptions import *` directly. Ensure `exceptions.py` has `__all__` defined.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Exception hierarchy | Custom exception base class | Python built-in `Exception` as base for `KafPyError` | Standard Python pattern, needed for exception protocol |
| Rust error propagation | String-only error passing | Typed Rust enums (`thiserror`) + Python mapping | Preserves error codes and structured data |
| Kafka error code translation | Hardcode error code mappings | Parse from `rdkafka::error::KafkaError.error()` string | Avoids maintaining a duplicate error code table |

**Key insight:** `rdkafka` already provides error name and code in its `Display` implementation. The formatted error string from `KafkaError` (used in `thiserror` `#[error(...)]`) contains exactly what D-10 requires: error name + error code. We parse it rather than duplicating the error code table.

## Common Pitfalls

### Pitfall 1: Exception __str__ Not Called
**What goes wrong:** `print(e)` shows the dataclass repr, not the human-readable message.
**Why it happens:** Frozen dataclasses have `__repr__` but not `__str__` by default. When Python formats an exception in a traceback, it calls `str(e)` which falls back to the repr if `__str__` is not defined.
**How to avoid:** Define `__str__(self) -> str` returning `self.message` in `KafPyError` and override appropriately in subclasses.

### Pitfall 2: HandlerError Raised in Wrong Context
**What goes wrong:** `HandlerError` raised from within `PythonHandler::invoke` (Rust) gets caught as a generic Python exception and re-classified.
**Why it happens:** The `DefaultFailureClassifier` in `src/python/handler.rs` catches `PyErr` and extracts the exception name, but it classifies ALL Python exceptions as handler errors, not just `HandlerError` class instances.
**How to avoid:** D-09 specifies wrong-type access in Python-level `KafkaMessage` raises `HandlerError`. This is a Python-level concern — the Python handler code should raise `HandlerError` explicitly. The Rust side already handles panics in `invoke`.

### Pitfall 3: Missing error_code on non-Kafka Errors
**What goes wrong:** `ConsumerError` with `error_code=None` loses the structured context for non-Kafka errors.
**Why it happens:** Not all `PyError` variants carry Kafka error codes. `Subscription`, `Receive`, `Serialization` are string-based.
**How to avoid:** Make `error_code` an `int | None` field with `None` default. The message string carries the full context even when code is absent.

### Pitfall 4: pyconsumer.rs PyErr::new Uses Wrong Exception Type
**What goes wrong:** `PyErr::new::<pyo3::exceptions::PyRuntimeError, _>` loses type specificity — all errors become `RuntimeError`.
**Why it happens:** The existing code in `pyconsumer.rs` uses `PyRuntimeError` for all consumer errors.
**How to avoid:** Replace `PyRuntimeError` with the specific Python exception class at the PyO3 boundary. Requires importing the Python exception class into Rust via PyO3 (not trivial). Alternative: keep `PyRuntimeError` at boundary but ensure `exceptions.py` maps by parsing the error string (D-05/D-06 approach).

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| All errors as `PyRuntimeError` | Typed Python exceptions with structured context | Phase 36 | Better user experience, programmatic error inspection |
| No error_code field | Structured `error_code`, `partition`, `topic` fields on exceptions | Phase 36 | Enables error routing, logging, and user handling |
| Raw strings from Rust | Parsed structured messages with D-10 format | Phase 36 | Human-readable AND machine-parseable |

**Deprecated/outdated:**
- None — Phase 36 is net-new functionality.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | The `PyDict` passed to Python handlers in `src/python/handler.rs` will be converted to `KafkaMessage` instances by wrapping in Phase 36 or 37 | KafkaMessage Access Errors | If the dict pattern is kept, D-09 (wrong-type access HandlerError) requires different implementation |
| A2 | `ConsumerError` is the appropriate Python exception for `Producer` errors (no separate `ProducerError` needed) | Standard Stack | If `ProducerError` is needed later, a breaking change to `ERR-01`/`ERR-03` is required |
| A3 | `error_code` field uses integer Kafka error codes from `rdkafka` (e.g., 6 for `NOT_LEADER`) | Architecture Patterns | If Kafka error codes differ or are unavailable, the field type changes |

## Open Questions (RESOLVED)

1. **Producer errors:** Should `ProducerError` be a separate class or lumped into `ConsumerError`?
   - What we know: `PyError::Producer` variant exists in `src/errors.rs`
   - What's unclear: Whether a separate `ProducerError` is needed or if `ConsumerError` covers both
   - **Resolution:** Use `ConsumerError` for now; add `ProducerError` only if producer-side errors need distinct handling (decision: lump into `ConsumerError`)

2. **PythonHandler dict vs KafkaMessage:** Should `invoke` pass `KafkaMessage` instances instead of `PyDict`?
   - What we know: Current `invoke` builds `PyDict` and passes to Python callback
   - What's unclear: Whether Phase 36 or 37 changes this to typed `KafkaMessage`
   - **Resolution:** Create `KafkaMessage` class in `kafpy/handlers.py` and add a `from_dict` classmethod for Phase 36; actual dict-to-object conversion deferred to Phase 37

3. **PyO3 import of Python exception classes:**
   - What we know: `pyconsumer.rs` uses `PyErr::new::<pyo3::exceptions::PyRuntimeError, _>`
   - What's unclear: Whether we can import `ConsumerError` etc. from Python into Rust for typed PyErr at boundary
   - **Resolution:** Use string-parsing approach (D-05/D-06) to map errors rather than importing Python classes into Rust — simpler and less brittle

## Environment Availability

> Step 2.6: SKIPPED — Phase 36 is pure Python + Rust code changes with no external dependencies beyond what is already in the project.

**External dependencies:** None beyond `rdkafka` (system library, already in project), `pyo3` (Rust, already in project), `thiserror` (Rust, already in project).

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | pytest |
| Config file | `pytest.ini` or `pyproject.toml` [pytest] section |
| Quick run command | `pytest tests/test_exceptions.py -x -v` |
| Full suite command | `pytest tests/ -x -v` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command |
|--------|----------|-----------|-------------------|
| ERR-01 | `kafpy.exceptions` imports `KafPyError`, `ConsumerError`, `HandlerError`, `ConfigurationError` | unit | `pytest tests/test_exceptions.py::test_exceptions_module_exports -x` |
| ERR-02 | All exceptions inherit from `KafPyError`, not `Exception` directly | unit | `pytest tests/test_exceptions.py::test_exception_inheritance_chain -x` |
| ERR-03 | Rust consumer error string maps to `ConsumerError` with error_code | unit | `pytest tests/test_exceptions.py::test_rust_consumer_error_translation -x` |
| ERR-04 | Wrong-type field access on `KafkaMessage` raises `HandlerError` | unit | `pytest tests/test_exceptions.py::test_kafka_message_wrong_type_raises_handler_error -x` |
| ERR-05 | Importing from `_kafpy` does not expose exception classes | unit | `pytest tests/test_exceptions.py::test_private_module_no_exception_leak -x` |

### Sampling Rate
- **Per task commit:** `pytest tests/test_exceptions.py -x`
- **Per wave merge:** `pytest tests/ -x`
- **Phase gate:** Full suite green before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `tests/test_exceptions.py` — covers ERR-01 through ERR-05
- [ ] `tests/conftest.py` — shared fixtures (if not already present)
- [ ] Framework install: `pip install pytest` — if not detected, add to pyproject.toml

## Sources

### Primary (HIGH confidence)
- `src/errors.rs` — existing `PyError` enum with `Consumer`, `Producer`, `Config` variants
- `src/consumer/error.rs` — existing `ConsumerError` enum with Kafka variants
- `src/dispatcher/error.rs` — existing `DispatchError` enum
- `src/coordinator/error.rs` — existing `CoordinatorError` enum
- `kafpy/__init__.py` — stub imports with try/except pattern
- `kafpy/config.py` — frozen dataclass pattern for Python-side types
- `src/python/handler.rs` — `PythonHandler::invoke` dict-passing pattern

### Secondary (MEDIUM confidence)
- PEP 249 (Python DB API 2.0) — exception hierarchy pattern (similar structure: base + specific subclasses)
- PyO3 exception handling docs — `PyErr::new` pattern at boundary

### Tertiary (LOW confidence)
- None — all claims verified against codebase

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — only uses existing project dependencies
- Architecture: HIGH — builds on established patterns (frozen dataclasses, PyO3 boundary)
- Pitfalls: MEDIUM — D-09 implementation path partially assumes dict-to-object wrapping

**Research date:** 2026-04-20
**Valid until:** 2026-05-20 (30 days — stable domain, Python exception patterns are well-established)
