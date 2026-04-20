# Phase 35: Handler Registration & Runtime - Context

**Gathered:** 2026-04-20
**Status:** Ready for planning

<domain>
## Phase Boundary

Decorator-based and explicit handler registration, KafkaMessage/HandlerContext/HandlerResult types, KafPy runtime with start/stop/run lifecycle.
</domain>

<decisions>
## Implementation Decisions

### Registration Architecture (D-01)
Dual Consumer + KafPy wrapper model:
- `consumer = kafpy.Consumer(config)` — returns Rust Consumer (from _kafpy)
- `app = kafpy.KafPy(consumer)` — Python KafPy wrapper owning the Rust consumer
- Handlers registered via `app.handler(topic="...", routing=...)` decorator or `app.register_handler(topic, handler_fn, routing=...)`

### Handler Type Detection (D-02)
Callable type detection at registration:
- `inspect.iscoroutinefunction(fn)` → async handler
- `inspect.isasyncgenfunction(fn)` → batch handler
- Otherwise → sync handler

### HandlerContext (D-03)
Stub frozen dataclass for Phase 35:
- Fields: topic, partition, offset, timestamp (placeholders)
- Full context propagation comes later

### HandlerResult (D-04)
Stub frozen dataclass for Phase 35:
- Field: action (placeholder for return value)
- Full routing action comes later

### Lifecycle Methods (D-05)
- `app.start()` → begins consuming
- `app.stop()` → graceful drain and shutdown
- `app.run()` → start() + block until stop()

### Module Layout (D-06)
- `kafpy/handlers.py` — KafkaMessage (from Rust), HandlerContext, HandlerResult, registration functions
- `kafpy/consumer.py` — Consumer class (Rust wrapper)
- `kafpy/runtime.py` — KafPy class with lifecycle + registration
</decisions>

<canonical_refs>
## Canonical References

- `.planning/REQUIREMENTS.md` — REG-01 through REG-07
- `.planning/ROADMAP.md` Phase 35
- `.planning/phases/33-public-api-conventions/33-CONTEXT.md`
- `.planning/phases/34-configuration-model/34-CONTEXT.md`
</canonical_refs>

<deferred>
## Deferred Ideas

None
</deferred>
