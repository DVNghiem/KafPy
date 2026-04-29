# Phase 10: Streaming Handler - Context

**Gathered:** 2026-04-29
**Status:** Ready for planning

<domain>
## Phase Boundary

Persistent async iterable handlers maintain long-lived connections (WebSocket, SSE, live dashboard feeds) with proper lifecycle and backpressure.

**Requirements:** STRM-01 (HandlerMode::StreamingAsync), STRM-02 (@stream_handler Python API), STRM-03 (lifecycle management), STRM-04 (per-stream backpressure)

</domain>

<decisions>
## Implementation Decisions

### Streaming Pattern
- **D-01:** `HandlerMode::StreamingAsync` variant for persistent async iterable handlers (vs existing Sync/Async modes)
- **D-02:** Handler returns `AsyncIterator<Message>` — Python generator or Rust stream

### Lifecycle Management
- **D-03:** Four-phase lifecycle: start/subscribe (connect + subscribe), run/loop (process until stop), stop/drain (graceful finish), error recovery (retry with backoff)
- **D-04:** Stop signal coordinated with graceful shutdown (same mechanism as phase 7 Rayon drain)

### Backpressure
- **D-05:** Per-stream backpressure — slow consumer pauses Kafka consumption, fast producer does not overflow memory
- **D-06:** Use bounded channel buffer (existing pattern from phase 1) with pause/resume semantics

### Python API
- **D-07:** `@stream_handler(topic)` as separate decorator from `@handler` — explicit distinction between batch/one-shot and streaming
- **D-08:** `yield` based handlers in Python map naturally to async iterators

### Claude's Discretion
- Exact backpressure buffer sizes — planner decides based on memory model
- Error recovery retry timing and max attempts — planner decides
- How to detect "slow consumer" state — planner decides

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Requirements
- `.planning/REQUIREMENTS.md` §STRM — STRM-01, STRM-02, STRM-03, STRM-04

### Prior Phases
- `.planning/phases/07-thread-pool/07-CONTEXT.md` — shutdown coordination pattern
- `.planning/phases/08-async-timeout/08-CONTEXT.md` — timeout and JoinHandle pattern
- `.planning/phases/09-handler-middleware/09-CONTEXT.md` — HandlerMiddleware trait and PythonHandler invoke chain

### Implementation
- `src/python/handler.rs` — existing handler invocation pattern
- `src/worker_pool/mod.rs` — existing backpressure (pause/resume) pattern
- `src/dlq/metadata.rs` — existing metadata envelope pattern

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `HandlerMode` enum in handler.rs: can add `StreamingAsync` variant
- `PythonHandler::invoke_mode_with_timeout`: existing invocation wrapping pattern
- Pause/resume partition logic in worker_pool: existing backpressure mechanism

### Established Patterns
- Bounded channels from phase 1: use for stream buffer
- Graceful shutdown coordination from phase 7: reuse for stream drain
- Middleware chain from phase 9: can apply to streaming handlers

### Integration Points
- Streaming handler registers via PythonHandler similar to existing handlers
- Backpressure hooks into existing queue depth monitoring
- DLQ metadata envelope extended for stream-specific fields (if needed)

</code_context>

<specifics>
## Specific Ideas

No external specs referenced. Requirements fully captured above.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 10-streaming-handler*
*Context gathered: 2026-04-29*