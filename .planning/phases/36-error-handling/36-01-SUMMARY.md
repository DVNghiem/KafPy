# Phase 36-01: Error Handling - Implementation Summary

## Overview
Implemented Python exception hierarchy for KafPy v1.8 per ERR-01 through ERR-05 and D-08, D-09, D-10 requirements.

## Files Created
- `/home/nghiem/project/KafPy/kafpy/exceptions.py` — Exception class hierarchy (KafPyError, ConsumerError, HandlerError, ConfigurationError) + `translate_rust_error()`
- `/home/nghiem/project/KafPy/tests/test_exceptions.py` — 20 tests covering ERR-01 through ERR-05

## Files Modified
- `/home/nghiem/project/KafPy/kafpy/handlers.py` — Added `KafkaMessage` class with D-08/D-09 field access
- `/home/nghiem/project/KafPy/kafpy/__init__.py` — Direct exception imports (no try/except), added `KafkaMessage`, guarded `_kafpy` import

## Key Implementation Details

### Exception Hierarchy (ERR-01, ERR-02)
- `KafPyError` inherits from `Exception` (required for Python exception protocol)
- `ConsumerError`, `HandlerError`, `ConfigurationError` inherit from `KafPyError` only (not `Exception` directly)
- All classes are `frozen=True` dataclasses with structured fields: `message`, `error_code`, `partition`, `topic`

### D-10 Format
- `ConsumerError.__str__` appends `on {topic}@{partition}` when both are present

### Rust Error Translation (ERR-03)
- `translate_rust_error()` parses D-10 formatted error strings to extract `error_code` and topic/partition
- Maps `Consumer`/`Producer` modules to `ConsumerError`, `Config` to `ConfigurationError`

### KafkaMessage D-08/D-09 (ERR-04)
- `msg.key` returns `None` silently when key is `None` (no exception)
- `get_key_as_string()` and `get_payload_as_string()` raise `HandlerError` on UTF-8 decode failure

### Public API Boundaries (ERR-05)
- All exception types importable from `kafpy` directly and from `kafpy.exceptions`
- `kafpy.__all__` includes all exception types and `KafkaMessage`

### Test Results
All 20 tests pass:
```
tests/test_exceptions.py::TestExceptionHierarchy ........................... 3 passed
tests/test_exceptions.py::TestExceptionInheritanceChain .................... 2 passed
tests/test_exceptions.py::TestStructuredAttributes ........................... 3 passed
tests/test_exceptions.py::TestConsumerErrorStrFormat ........................ 2 passed
tests/test_exceptions.py::TestRustErrorTranslation ........................... 3 passed
tests/test_exceptions.py::TestKafkaMessageAccessErrors ...................... 4 passed
tests/test_exceptions.py::TestPublicApiBoundaries ........................... 3 passed
============================== 20 passed in 0.06s ==============================
```
