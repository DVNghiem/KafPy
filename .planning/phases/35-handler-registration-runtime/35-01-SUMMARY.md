# Phase 35: Handler Registration & Runtime — Summary

**Completed:** 2026-04-20
**Plans:** 1/1 (35-01-PLAN.md)
**Status:** Complete

## Files Created/Modified

| File | Change |
|------|--------|
| `kafpy/handlers.py` | **Created** — HandlerContext, HandlerResult stubs + register_handler |
| `kafpy/consumer.py` | **Created** — Consumer wrapper around _kafpy.Consumer |
| `kafpy/runtime.py` | **Created** — KafPy class with start/stop/run + @app.handler decorator |
| `kafpy/__init__.py` | **Modified** — Imports from new modules, updated docstring |

## Decisions Applied

| ID | Decision | Source |
|----|----------|--------|
| D-01 | `kafpy.Consumer(config)` → Rust Consumer; `kafpy.KafPy(consumer)` → Python wrapper | 35-CONTEXT.md |
| D-02 | Callable type detection via inspect.iscoroutinefunction/isasyncgenfunction/isgeneratorfunction | 35-CONTEXT.md |
| D-03 | HandlerContext stub: frozen dataclass with topic/partition/offset/timestamp/headers | 35-CONTEXT.md |
| D-04 | HandlerResult stub: frozen dataclass with action field | 35-CONTEXT.md |
| D-05 | app.start(), app.stop(), app.run() lifecycle | 35-CONTEXT.md |
| D-06 | Module layout: kafpy/handlers.py, kafpy/consumer.py, kafpy/runtime.py | 35-CONTEXT.md |

## Success Criteria Status

| Criterion | Status |
|-----------|--------|
| @app.handler(topic="...", routing=...) decorates callable as handler | ✓ |
| app.register_handler(topic, handler_fn, routing=...) explicit registration | ✓ |
| Handler signature: def handler(msg: KafkaMessage, ctx: HandlerContext) -> HandlerResult | ✓ |
| Sync/async/batch all use same registration API (type detection) | ✓ |
| app.start() begins consuming; app.stop() initiates graceful drain; app.run() blocks | ✓ |

## Notes

- HandlerContext and HandlerResult are **stubs** — full implementation in Phase 36 (Error Handling) or later
- The `run()` method is a simple blocking loop stub — proper async event loop integration comes later
- The Consumer class wraps `_kafpy.Consumer` and delegates all calls to the Rust implementation
- Handler type detection is implemented but not yet wired to Rust consumer dispatch
