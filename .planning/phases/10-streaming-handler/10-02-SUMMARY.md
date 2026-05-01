---
phase: 10-streaming-handler
plan: "02"
subsystem: python-api
tags: [streaming, decorator, handler, STRM-02]
requires: [STRM-02]
provides: [stream_handler]
affects: [kafpy/handlers.py, kafpy/__init__.py]
tech-stack:
  added: []
  patterns: [decorator, async-generator]
key-files:
  created: []
  modified:
    - kafpy/handlers.py
    - kafpy/__init__.py
key-decisions:
  - "Used inspect.isasyncgenfunction instead of iscoroutinefunction because async generators return False for the latter but True for the former"
  - "Reused existing _register_handler internal function rather than creating new registration logic"
dependencies:
  - "09-handler-middleware (middleware infrastructure)"
completeness: complete
---

# Phase 10-02 Plan Summary

**One-liner:** `@stream_handler(topic)` Python decorator for persistent async iterable handlers (STRM-02)

## What Was Built

Added `@stream_handler` Python decorator in `kafpy/handlers.py` and exported it from the `kafpy` package. This decorator registers async generator functions as streaming handlers with `mode="streaming_async"`.

**Key implementation details:**
- Decorator validates function is async generator (raises `TypeError` if not)
- Calls internal `_register_handler` with `mode="streaming_async"`
- Supports `topic`, `name`, `retries`, `timeout`, `middleware` parameters
- Async generators return `False` for `iscoroutinefunction` but `True` for `isasyncgenfunction` — fixed detection logic accordingly

## Tasks Completed

| Task | Name | Commit | Files |
|------|------|--------|-------|
| 1 | Add @stream_handler decorator | `35d7d35` | kafpy/handlers.py, kafpy/__init__.py |
| 2 | Export stream_handler from kafpy package | `35d7d35` | kafpy/__init__.py |

## Verification

```bash
python3 -c "from kafpy.handlers import stream_handler; print('OK')"  # PASS
python3 -c "from kafpy import stream_handler; print('OK')"           # PASS
cargo check                                                    # PASS (no errors)
```

## Decorator Validation Tests

- Non-async function raises `TypeError("@stream_handler requires an async generator function")` — PASS
- Async generator accepts decorator successfully — PASS
- `inspect.isasyncgenfunction` correctly identifies async generators — PASS

## Deviations from Plan

**Rule 1 - Bug Fix:** Fixed async generator detection to use `inspect.isasyncgenfunction` instead of `inspect.iscoroutinefunction`. Async generators are not coroutine functions — they are a distinct type. This bug would have caused all async generators to be rejected with a TypeError even though they are valid streaming handlers.

**Total deviations:** 1 auto-fixed bug.

## Commits

- `35d7d35` — `feat(10-02): add @stream_handler decorator for streaming async handlers`

## Duration

Started: 2026-04-29T13:58:13Z
Completed: ~2 minutes