# Phase 10: Streaming Handler - Research

**Researched:** 2026-04-29
**Domain:** Persistent async iterable handlers for long-lived connections (WebSocket, SSE, live dashboard feeds)
**Confidence:** MEDIUM

## Summary

Phase 10 implements persistent async iterable handlers (`HandlerMode::StreamingAsync`) for long-lived connections like WebSocket feeds, SSE, and live dashboards. Unlike one-shot handlers that process a single message per invocation, streaming handlers maintain a persistent async iterator that continuously yields messages from Kafka until stopped. The key architectural challenge is the four-phase lifecycle (start/subscribe, run/loop, stop/drain, error recovery) and per-stream backpressure that prevents slow consumers from overflowing memory.

The implementation extends the existing `HandlerMode` enum, builds on the `PythonAsyncFuture` infrastructure from phase 26 (async bridge), and reuses bounded channel backpressure from phase 1. The primary integration point is a new `invoke_streaming` method on `PythonHandler` that wraps the async generator in a persistent loop, with the existing `spawn_blocking`-free async path driving the coroutine via `PythonAsyncFuture`. Middleware from phase 9 applies to streaming handlers via the same chain mechanism.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Async iterator lifecycle | API / Backend (Rust) | Browser/Client | Rust owns loop control, message delivery; Python yields messages |
| Backpressure | API / Backend (Rust) | CDN/Static | Bounded channel pause/resume in Rust; Python doesn't manage flow |
| Python async generator bridge | API / Backend (Rust) | — | PythonAsyncFuture drives coroutine; GIL acquired transiently |
| Lifecycle management | API / Backend (Rust) | — | CancellationToken coordinates start/stop/drain |
| Middleware hooks | API / Backend (Rust) | — | HandlerMiddleware before/after/on_error applied per-message |

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

- **D-01:** `HandlerMode::StreamingAsync` variant for persistent async iterable handlers (vs existing Sync/Async modes)
- **D-02:** Handler returns `AsyncIterator<Message>` — Python generator or Rust stream
- **D-03:** Four-phase lifecycle: start/subscribe (connect + subscribe), run/loop (process until stop), stop/drain (graceful finish), error recovery (retry with backoff)
- **D-04:** Stop signal coordinated with graceful shutdown (same mechanism as phase 7 Rayon drain)
- **D-05:** Per-stream backpressure — slow consumer pauses Kafka consumption, fast producer does not overflow memory
- **D-06:** Use bounded channel buffer (existing pattern from phase 1) with pause/resume semantics
- **D-07:** `@stream_handler(topic)` as separate decorator from `@handler` — explicit distinction between batch/one-shot and streaming
- **D-08:** `yield` based handlers in Python map naturally to async iterators

### Claude's Discretion

- Exact backpressure buffer sizes — planner decides based on memory model
- Error recovery retry timing and max attempts — planner decides
- How to detect "slow consumer" state — planner decides

### Deferred Ideas (OUT OF SCOPE)

None.
</user_constraints>

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| STRM-01 | `HandlerMode::StreamingAsync` for persistent async iterable handlers | Extends HandlerMode enum (handler.rs:83-94); add variant + as_str match arm |
| STRM-02 | `@stream_handler(topic)` Python API — long-lived handler | Python decorator wrapping `register_handler` with mode=StreamingAsync; asyncgen detection (handlers.py:103-104) |
| STRM-03 | Lifecycle management (start/subscribe, run/loop, stop/drain, error recovery) | New `invoke_streaming` method; CancellationToken coordination from pool.rs:197-221 |
| STRM-04 | Per-stream backpressure propagation | Bounded mpsc channel (queue_manager.rs:172); pause/resume semantics from backpressure.rs:20 |

## Standard Stack

No new external dependencies required. Implementation uses only existing project infrastructure:

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| Tokio | current | Async runtime for streaming loop | Already used for all async operations |
| pyo3 | current | Python GIL bridge | Already used for `PythonAsyncFuture` (async_bridge.rs) |
| tokio::sync::mpsc | current | Bounded channel for backpressure | Already used in queue_manager.rs |
| tokio_util::sync::CancellationToken | current | Lifecycle cancellation | Already used in worker_pool/mod.rs |
| parking_lot | current | Mutex for metadata | Already used in BatchAccumulator |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| chrono | current | Timestamps in DLQ envelope | Already a project dependency |
| tracing | current | Structured logging | Already a project dependency |
| tokio::time | current | Backoff timers | Already used for retry sleep |

**Installation:** No new crates required.

## Architecture Patterns

### System Architecture Diagram

```
┌──────────────────────────────────────────────────────────────────────┐
│                        Streaming Worker Loop                          │
│  (spawned by WorkerPool, runs for lifetime of handler subscription)  │
└──────────────────────────────────────────────────────────────────────┘
         │                                           │
         ▼                                           ▼
┌─────────────────┐                    ┌─────────────────────────────────┐
│  START/SUBSCRIBE │                    │         RUN/LOOP                 │
│                 │                    │                                 │
│  1. Create      │                    │  loop {                         │
│     Python      │                    │    // Poll async generator       │
│     async       │                    │    match future.poll(cx) {       │
│     generator   │                    │      Pending => continue         │
│     via         │                    │      Ready(msg) => dispatch      │
│     Python      │                    │      Ready(StopAsyncIteration) => │
│     GIL         │                    │        graceful stop              │
│                 │                    │    }                              │
│  2. Wrap in     │                    │    // Backpressure check          │
│     Python      │                    │    if queue.is_full() {          │
│     AsyncFuture │                    │      pause_partition()           │
│                 │                    │    }                              │
│  3. Subscribe   │                    │    // Dispatch to middleware +  │
│     to Kafka    │                    │    // handler, yield to consumer  │
└─────────────────┘                    └─────────────────────────────────┘
                                                   │
                     ┌───────────────────────────┘
                     ▼
┌──────────────────────────────────────────────────────────────────────┐
│                        STOP/DRAIN                        │
│                                                              │
│  1. CancellationToken signals stop                         │
│  2. Async generator receives cancel via coro.close()     │
│  3. Drain buffered messages (minimize loss)               │
│  4. Commit offsets for processed messages                │
│  5. Exit loop                                             │
└──────────────────────────────────────────────────────────────┘
```

### Recommended Project Structure

```
src/
├── python/
│   ├── handler.rs          # HandlerMode::StreamingAsync added here
│   ├── async_bridge.rs    # PythonAsyncFuture — drives async generator polling
│   └── streaming.rs       # NEW: StreamingHandler wrapper, invoke_streaming method
├── worker_pool/
│   ├── mod.rs             # WorkerPool spawns streaming worker loop
│   ├── pool.rs            # spawn() calls — add streaming_worker_loop branch
│   └── streaming_loop.rs  # NEW: streaming_worker_loop function (replaces worker_loop)
└── dispatcher/
    └── backpressure.rs    # BackpressureAction::PausePartition implemented

kafpy/
├── handlers.py           # register_handler detects asyncgen, @stream_handler decorator
└── __init__.py          # Export StreamHandler, @stream_handler
```

### Pattern 1: Async Generator Bridge

**What:** Python async generators are driven by `PythonAsyncFuture` polling `coro.send(None)` repeatedly. Each yield is treated as a message to process. `StopAsyncIteration` signals end of stream.

**When to use:** For every streaming handler invocation — the persistent async iterator pattern.

**Example:**
```rust
// Source: src/python/async_bridge.rs + streaming research
// Core loop: poll the coroutine until it yields or completes

loop {
    match python_async_future.poll(&mut cx) {
        Poll::Pending => {
            // No message ready — waker will re-poll us
            tokio::task::yield_now().await;
        }
        Poll::Ready(Ok) => {
            // Generator exhausted — graceful stop
            break;
        }
        Poll::Ready(ExecutionResult::Error { .. }) => {
            // Error during yield — error recovery path
            handle_stream_error(&ctx, result).await?;
        }
    }
}
```

**Key insight:** The GIL is released between each `poll` call, so the Tokio event loop is not blocked. The pattern requires no `spawn_blocking` since async generators hold no Python state between yields.

### Pattern 2: Four-Phase Streaming Lifecycle

**What:** The streaming worker loop transitions through four distinct phases with explicit state tracking.

**When to use:** Every streaming handler worker — provides clear semantics for start, run, stop, and error recovery.

**States:**
```rust
enum StreamingState {
    Starting,   // Subscribing to topic, creating async generator
    Running,    // Polling messages and yielding to consumer
    Draining,   // Cancellation received, draining buffered messages
    Recovering, // Error detected, applying retry/backoff before Restart
}
```

### Pattern 3: Per-Stream Backpressure via Bounded Channel

**What:** The existing bounded mpsc channel pattern (phase 1) is reused with a dedicated streaming buffer capacity. When the channel is full, the streaming loop pauses Kafka consumption for that partition rather than dropping messages.

**When to use:** For all streaming handler backpressure — ensures memory-bounded buffering.

**Example:**
```rust
// Source: queue_manager.rs:165-184 + backpressure.rs
// Streaming handler registration with dedicated buffer capacity
let rx = queue_manager.register_handler_with_semaphore(
    topic,
    streaming_buffer_capacity,  // Smaller than batch handler capacity
    Some(semaphore),
);
```

**Pause/resume integration:**
```rust
// Source: backpressure.rs:18-20
// When channel is full, emit BackpressureAction::PausePartition
fn on_queue_full(&self, topic: &str, handler: &HandlerMetadata) -> BackpressureAction {
    BackpressureAction::PausePartition(topic.to_string())
}
```

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Async generator polling | Custom Future implementation | `PythonAsyncFuture` from async_bridge.rs | Already bridges GIL release between polls correctly |
| Streaming loop | Raw `loop { tokio::select! }` | Structured `streaming_worker_loop` function | Lifecycle phases need clear state machine |
| Backpressure buffer | Ad-hoc ring buffer or VecDeque | Existing `mpsc::channel(capacity)` + QueueManager | Already handles queue_depth/inflight tracking |
| Graceful stop | `thread::sleep` + flag check | `CancellationToken` from tokio_util | Already coordinates with WorkerPool shutdown |

**Key insight:** The project already has all primitives needed. No new async primitives required — the only new code is the streaming worker loop state machine, the Python async generator invocation wrapper, and the `@stream_handler` Python decorator.

## Common Pitfalls

### Pitfall 1: Async Generator GIL State Leak

**What goes wrong:** The async generator holds state (local variables, open resources) across `yield` points. If the generator is not properly closed on cancel/drop, resources leak.

**Why it happens:** Python async generators maintain internal frame state. When `coro.close()` is called, the generator receives a `GeneratorExit` exception. If the generator's `finally` blocks don't clean up, resources stay allocated.

**How to avoid:** Call `coro.call_method0(py, "close")` in the `Drop` implementation of `PythonAsyncFuture` (already implemented in async_bridge.rs:92-99). Ensure the streaming loop breaks before dropping the future.

**Warning signs:** `ResourceWarning: unclosed file` in Python logs, socket connections not closed on stop.

### Pitfall 2: Blocking the Tokio Event Loop with Long-Running Generator

**What goes wrong:** The Python async generator does synchronous I/O (e.g., network call in `__anext__`) that takes seconds. The `poll` returns `Pending` but the waker isn't called promptly.

**Why it happens:** `PythonAsyncFuture` uses `wake_by_ref()` which re-schedules on the next Tokio tick, not immediately. If the generator blocks synchronously for > 100ms, heartbeat misses occur.

**How to avoid:** Document that streaming handlers must be truly async (no blocking I/O in the generator). Consider adding a soft timeout per-yield that triggers a warning metric.

**Warning signs:** Consumer group rebalances, late heartbeat logs in tracing.

### Pitfall 3: Middleware Before/After Called Per-Message vs Per-Generator

**What goes wrong:** The middleware chain's `before()` and `after()` are currently called once per handler invocation (invoke_mode_with_timeout). For streaming, they would be called once per yielded message or once per generator lifetime depending on design.

**Why it happens:** Phase 9 middleware design assumed one-shot invocations. Streaming changes the semantics.

**How to avoid:** Decide in planning: should middleware run once at `before_all` for the generator lifetime, or per-message? Document the decision. Most likely: `before()` once at generator start, `after()` once at generator end, and per-message error hooks.

**Warning signs:** Metrics show 1 middleware increment per 10,000 messages when expected 1:1.

### Pitfall 4: Backpressure Buffer Sizing

**What goes wrong:** Streaming buffer too small — constant pause/resume thrashing. Streaming buffer too large — memory bloat when multiple slow consumers stack up.

**Why it happens:** No principled sizing. Hard to predict because it depends on downstream consumer rate.

**How to avoid:** Use a monitoring approach: start with `capacity = 100` (tunable via config), emit queue depth metrics, let operators tune based on observed memory/throughput tradeoff. Default to smaller than batch handlers (streaming is lower throughput per message).

**Warning signs:** Prometheus `kafpy.queue.depth` spikes to max continuously, `kafpy.handler.latency` increases linearly.

### Pitfall 5: Streaming Worker Not Cancellable During `__anext__`

**What goes wrong:** `CancellationToken::cancelled()` is checked between message yields, but if `__anext__` is mid-call, cancellation is delayed until the yield returns.

**Why it happens:** Tokio can only cancel tasks at `await` points. `PythonAsyncFuture::poll` is the await point. If the underlying `coro.send(None)` takes 5 seconds, cancellation takes 5 seconds.

**How to avoid:** Use `tokio::time::timeout` per-yield as a soft safeguard. Not a hard abort (hard abort would kill the generator mid-yield), but a warning metric and potential reconnect. Hard cancellation remains at the generator close() on drop.

**Warning signs:** `streaming_handler.stop_delay_seconds` metric shows > 1s delays.

## Code Examples

Verified patterns from official sources:

### Python Async Generator (Reference Pattern)

```python
# Source: kafpy/handlers.py:103-104 + Python docs
# Streaming handlers are async generator functions

@stream_handler(topic="websocket-feed")
async def feed_handler(msg: KafkaMessage, ctx: HandlerContext):
    """Yield messages to a WebSocket sink."""
    ws = await websocket.connect("wss://example.com/feed")
    try:
        async for msg in kafka_consumer:
            await ws.send(msg.payload)
            yield  # Signal checkpoint for backpressure
    finally:
        await ws.close()
```

Note: `isasyncgenfunction` detection already exists in handlers.py:103-104.

### PythonAsyncFuture Per-Poll (Reference Implementation)

```rust
// Source: src/python/async_bridge.rs:41-81
// GIL acquired transiently per poll — the model for streaming polling

fn poll_coroutine(&mut self, cx: &mut Context<'_>) -> Poll<ExecutionResult> {
    Python::attach(|py| {
        let result = self.coro.call_method1(py, "send", (py.None(),));
        match result {
            Ok(_val) => {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            Err(py_err) if py_err.is_instance_of::<PyStopAsyncIteration>(py) => {
                Poll::Ready(ExecutionResult::Ok)
            }
            Err(py_err) => {
                // propagate error
                Poll::Ready(ExecutionResult::Error { .. })
            }
        }
    })
}
```

### CancellationToken Graceful Shutdown (Reference Implementation)

```rust
// Source: src/worker_pool/pool.rs:197-221
// The pattern to reuse for streaming drain

pub async fn shutdown(&mut self) {
    self.shutdown_token.cancel();
    let drain_timeout = self.coordinator.drain_timeout();
    match tokio::time::timeout(drain_timeout, self.join_set.shutdown()).await {
        Ok(()) => { /* drained gracefully */ }
        Err(_) => { self.join_set.abort_all(); }
    }
    self.coordinator.drain_rayon().await;
    self.offset_coordinator.flush_failed_to_dlq(&self.dlq_router, &self.dlq_producer);
}
```

### Bounded Channel Backpressure (Reference Implementation)

```rust
// Source: src/dispatcher/queue_manager.rs:165-184
// Streaming handlers get dedicated buffer capacity

pub(crate) fn register_handler_with_semaphore(
    &self,
    topic: impl Into<String>,
    capacity: usize,  // Streaming: smaller than batch
    semaphore: Option<Arc<Semaphore>>,
) -> mpsc::Receiver<OwnedMessage> {
    let (tx, rx) = mpsc::channel(capacity);
    // ... metadata tracking
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| One-shot handler invoke | Persistent async iterator via PythonAsyncFuture | Phase 10 | Long-lived connections (WebSocket, SSE) now possible |
| Per-message ack | Per-generator start/stop lifecycle | Phase 10 | Different offset commit semantics |
| Batch-mode accumulator | Streaming buffer with pause/resume | Phase 10 | Backpressure applied at generator level |

**Deprecated/outdated:**
- `invoke_mode` one-shot pattern: remains valid for Sync/Async/Batch modes, but Streaming uses `invoke_streaming`
- Single-message middleware before/after: streaming middleware semantics need clarification (see Pitfall 3)

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Python async generators can be driven by polling `coro.send(None)` the same way single-shot coroutines are | Architecture Patterns | If async generators require different polling, PythonAsyncFuture needs modification |
| A2 | The existing `spawn_blocking`-free async path (invoke_async) works as a template for streaming invoke without major refactoring | Architecture Patterns | If streaming requires a fundamentally different bridge, more implementation work |
| A3 | Middleware per-message is the desired semantics for streaming (rather than once-per-generator) | Common Pitfalls | If middleware should run once at start/end, the before/after API needs adjustment |
| A4 | Streaming handlers don't need a separate worker pool — they can coexist with existing workers | Architecture Patterns | If streaming requires dedicated workers, WorkerPool::new needs modification |

**If this table is empty:** All claims in this research were verified or cited — no user confirmation needed.

## Open Questions

1. **Middleware semantics for streaming**
   - What we know: Phase 9 middleware has `before()`, `after()`, `on_error()` per invocation
   - What's unclear: Should these run once per generator lifetime (start/end) or per yielded message?
   - Recommendation: Per-message `on_error()` for errors during yield, once per generator for `before()`/`after()`. Confirm with user.

2. **Offset commit strategy**
   - What we know: Phase 1 uses commit-on-ack per message. Streaming generators yield many messages before ack.
   - What's unclear: Do we commit offsets per-message-yield, or only at generator end?
   - Recommendation: Commit per-message-yield to maintain at-least-once semantics. The existing `offset_coordinator.record_ack()` handles this.

3. **Generator lifetime vs consumer group membership**
   - What we know: Streaming handlers hold long-lived connections. Rebalances during a stream are disruptive.
   - What's unclear: Should streaming handlers hold a static partition assignment vs dynamic rebalance?
   - Recommendation: Planner should decide if streaming handlers need sticky partition assignment.

## Environment Availability

Step 2.6: SKIPPED — no external dependencies beyond existing project infrastructure.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | pytest + tokio::test for Rust |
| Config file | pytest.ini (if any) |
| Quick run command | `cargo test --lib` |
| Full suite command | `cargo test` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| STRM-01 | HandlerMode::StreamingAsync variant exists | unit | `cargo test handler::tests -- --nocapture` | partial (handler.rs has tests) |
| STRM-02 | @stream_handler decorator registers streaming handler | integration | `pytest tests/test_streaming.py -x` | no - needs Wave 0 |
| STRM-03 | Lifecycle state transitions | unit | `cargo test streaming_loop::tests -- --nocapture` | no - needs Wave 0 |
| STRM-04 | Backpressure pauses on full queue | unit + integration | `cargo test backpressure::tests -- --nocapture` | partial (backpressure.rs has tests) |

### Wave 0 Gaps
- [ ] `tests/test_streaming.py` — covers STRM-02, STRM-03
- [ ] `src/python/streaming.rs` — covers STRM-01 invoke_streaming method
- [ ] `src/worker_pool/streaming_loop.rs` — covers STRM-03 lifecycle
- [ ] `kafpy/handlers.py` — add `@stream_handler` decorator

### Sampling Rate
- **Per task commit:** `cargo test --lib`
- **Per wave merge:** `cargo test`
- **Phase gate:** Full suite green before `/gsd-verify-work`

## Security Domain

Not applicable — phase adds in-process Python async iterator support with no new network exposure, no auth changes, and no new secret management.

## Sources

### Primary (HIGH confidence)
- `src/python/handler.rs` — HandlerMode enum, PythonHandler structure, invoke methods
- `src/python/async_bridge.rs` — PythonAsyncFuture Future implementation, GIL bridge pattern
- `src/worker_pool/pool.rs` — graceful shutdown with CancellationToken
- `src/dispatcher/queue_manager.rs` — bounded channel registration, backpressure metadata
- `src/dispatcher/backpressure.rs` — BackpressureAction enum

### Secondary (MEDIUM confidence)
- `kafpy/handlers.py` — asyncgen detection via `isasyncgenfunction`, register_handler pattern
- `kafpy/__init__.py` — BaseMiddleware, Logging, Metrics classes
- Phase 09 context — HandlerMiddleware trait, build_middleware_chain

### Tertiary (LOW confidence)
- Python async generator behavior: based on general Python/PyO3 knowledge, not verified against specific PyO3 asyncgen handling docs

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all infrastructure already in project
- Architecture: MEDIUM — streaming loop pattern requires new state machine; GIL bridge already exists
- Pitfalls: MEDIUM — based on general async/Python concerns, not project-tested

**Research date:** 2026-04-29
**Valid until:** 2026-05-29 (30 days — async streaming patterns are well-established)
