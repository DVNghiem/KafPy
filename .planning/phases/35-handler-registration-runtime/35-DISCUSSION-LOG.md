# Phase 35: Handler Registration & Runtime - Discussion Log

> **Audit trail only.** Decisions are captured in CONTEXT.md.

**Date:** 2026-04-20
**Phase:** 35-handler-registration-runtime
**Areas discussed:** Registration object, handler type detection, HandlerContext/HandlerResult

---

## Registration Object

| Option | Description | Selected |
|--------|-------------|----------|
| @app.handler on KafPy instance | @app.handler(topic="mytopic") — decorator on Python KafPy runtime | |
| @consumer.handler on separate Consumer | Separate Consumer object, decorator on it | |
| **Dual: Consumer + KafPy wrapper (Recommended)** | kafpy.Consumer(config) → kafpy.KafPy(consumer) → @kafpy.handler(...) | ✓ |

**User's choice:** Dual: `kafpy.Consumer(config)` returns Rust Consumer; `kafpy.KafPy(consumer)` wraps it; handlers registered on KafPy wrapper

---

## Handler Type Detection

| Option | Description | Selected |
|--------|-------------|----------|
| **Callable type detection (Recommended)** | inspect.iscoroutinefunction() for async, yield for batch | ✓ |
| Explicit mode parameter | handler(topic, fn, mode="sync"\|"async"\|"batch") | |
| Callable OR explicit parameter | Default to type detection, allow override | |

**User's choice:** Callable type detection — inspect callable type at registration time

---

## HandlerContext/HandlerResult

| Option | Description | Selected |
|--------|-------------|----------|
| **Stub Python classes (Recommended)** | Simple frozen dataclass stubs, full implementation later | ✓ |
| Full implementations | HandlerContext with topic/partition/offset/timestamp/headers; HandlerResult with action | |

**User's choice:** Stub classes — Phase 35 focuses on registration + lifecycle

---

## Deferred Ideas

None
