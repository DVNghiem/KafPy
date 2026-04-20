---
phase: '36'
phase-slug: error-handling
date: '2026-04-20'
---

# Phase 36: Error Handling — Validation Strategy

## Test Framework

| Property | Value |
|----------|-------|
| Framework | pytest |
| Config file | `pytest.ini` or `pyproject.toml` [pytest] section |
| Quick run command | `pytest tests/test_exceptions.py -x -v` |
| Full suite command | `pytest tests/ -x -v` |

## Phase Requirements to Test Map

| Req ID | Behavior | Test Type | Automated Command |
|--------|----------|-----------|-----------------|
| ERR-01 | `kafpy.exceptions` imports `KafPyError`, `ConsumerError`, `HandlerError`, `ConfigurationError` | unit | `pytest tests/test_exceptions.py::test_exceptions_module_exports -x` |
| ERR-02 | All exceptions inherit from `KafPyError`, not `Exception` directly | unit | `pytest tests/test_exceptions.py::test_exception_inheritance_chain -x` |
| ERR-03 | Rust consumer error string maps to `ConsumerError` with error_code | unit | `pytest tests/test_exceptions.py::test_rust_consumer_error_translation -x` |
| ERR-04 | Wrong-type field access on `KafkaMessage` raises `HandlerError` | unit | `pytest tests/test_exceptions.py::test_kafka_message_wrong_type_raises_handler_error -x` |
| ERR-05 | Importing from `_kafpy` does not expose exception classes | unit | `pytest tests/test_exceptions.py::test_private_module_no_exception_leak -x` |

## Sampling Rate

- **Per task commit:** `pytest tests/test_exceptions.py -x`
- **Per wave merge:** `pytest tests/ -x`
- **Phase gate:** Full suite green before `/gsd-verify-work`

## Wave 0 Gaps

- [ ] `tests/test_exceptions.py` — covers ERR-01 through ERR-05
- [ ] `tests/conftest.py` — shared fixtures (if not already present)
- [ ] Framework install: `pip install pytest` — if not detected, add to pyproject.toml
