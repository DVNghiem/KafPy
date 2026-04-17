# Phase 22: Python Integration - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-04-17
**Phase:** 22-python-integration
**Areas discussed:** Python callable signature, Python callable return type, Python routing errors

---

## Python callable signature — what does the callback receive?

| Option | Description | Selected |
|--------|-------------|----------|
| Dict: {topic, headers, key, payload} | Simple dict with topic(str), headers(dict), key(bytes), payload(bytes). Follows PythonHandler.invoke pattern. | ✓ |
| PyRoutingContext class (pyclass) | Custom Python class wrapping RoutingContext fields. More PyO3 boilerplate. | |
| Tuple: (topic, headers, key, payload) | Simple tuple. Less structured than dict. | |

**User's choice:** Dict — consistent with PythonHandler.invoke pattern

---

## Python callable return type — what does it return?

| Option | Description | Selected |
|--------|-------------|----------|
| String enum: 'route:handler_id', 'drop', 'reject:reason', 'defer' | Simple string parsing — easy to implement, clear semantics. | ✓ |
| Python Enum class (pyclass) | Define a Python RoutingDecision enum. More type-safe but requires PyO3 enum mapping. | |
| Dict: {'variant': 'route', 'handler_id': 'my-handler', ...} | Dict with keys. Flexible but verbose. | |

**User's choice:** String enum — simple, Pythonic

---

## Python routing errors — what happens if the callback throws?

| Option | Description | Selected |
|--------|-------------|----------|
| Reject(RoutingPythonError) — explicit failure | Treat exception as Reject(RoutingPythonError). DLQ routing follows. | ✓ |
| Defer — fall through to default handler | Treat Python errors as Defer. More resilient. | |
| Defer + log warning | Same as Defer but logs a warning. | |

**User's choice:** Reject(RoutingPythonError) — explicit failure, routing errors distinguishable from user Reject

---

## Claude's Discretion

All 3 decisions were user-driven. No areas delegated to Claude discretion.

## Deferred Ideas

- Schema-based routing (Avro/Protobuf) — deferred to post-v1.5
- Content-based routing (payload parsing) — Python fallback only; already handled via Python callable payload argument
- Multi-handler fan-out — single handler per message, deferred
