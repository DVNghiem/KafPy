# Phase 33: Public API Conventions - Discussion Log

> **Audit trail only.** Decisions are captured in CONTEXT.md.

**Date:** 2026-04-20
**Phase:** 33-public-api-conventions
**Areas discussed:** __all__ Strategy, PyO3 pub boundary, kafpy init exports

---

## __all__ Strategy

| Option | Description | Selected |
|--------|-------------|----------|
| Every module (Recommended) | `__all__` in every .py file — explicit and defensive | ✓ |
| Only package init | `__all__` only in kafpy/__init__.py | |

**User's choice:** Every module

---

## PyO3 pub Boundary

| Option | Description | Selected |
|--------|-------------|----------|
| Principle of least exposure (Recommended) | pub only when Python MUST access; rest is pub(crate) or private | ✓ |
| Broad pub | Mark pub broadly in Rust, restrict at Python __all__ level | |

**User's choice:** Principle of least exposure

---

## kafpy init Exports

| Option | Description | Selected |
|--------|-------------|----------|
| Minimal surface (Recommended) | Only KafPy, ConsumerConfig, exceptions, KafkaMessage | |
| Full surface (Recommended) | Re-export everything from submodules — more discoverable | ✓ |

**User's choice:** Full surface

---

## Deferred Ideas

None
