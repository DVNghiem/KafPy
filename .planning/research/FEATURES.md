# v1.1 Feature Research: Async Handler Patterns

**Context**: KafPy v1.1 milestone — Rust-core, Python-logic Kafka consumer framework
**New features**: Streaming handler patterns, handler middleware/chain, async handler timeout, async fan-in/fan-out
**Research date**: 2026-04-29

---

## 1. Feature Categories

### Table Stakes Features (Users Expect)

These are baseline capabilities users anticipate when they hear "async handler patterns." Missing them makes the framework feel incomplete.

| Feature | Why Expected | Complexity | Dependencies |
|---------|--------------|------------|--------------|
| **Async handler timeout** | Long-running handlers block the poll cycle, causing heartbeats to miss and triggering rebalances. Users need a way to abort slow handlers. | Low | Existing `invoke_mode_with_timeout` wraps in `tokio::time::timeout`; already partially implemented. |
| **Handler middleware (logging)** | Users expect request/response logging without manually wrapping every handler. A middleware chain avoids handler boilerplate. | Medium | Existing `ExecutionContext` and `TracingSink`; could add before/after hooks. |
| **Handler middleware (metrics)** | Per-handler latency histograms, throughput counters. Users want this as a reusable layer, not per-handler instrumentation. | Medium | Existing `MetricsSink`; middleware would annotate span with handler timing. |

### Differentiators (Competitive Advantage)

These set KafPy apart from generic Kafka clients (confluent-kafka-python, aiokafka).

| Feature | Value Proposition | Complexity | Dependencies |
|---------|-------------------|------------|--------------|
| **Streaming handler patterns** (`@stream_handler`) | Persistent handler that receives a stream of messages (for WebSocket push, SSE, long-lived background tasks). Not batch (finishes) — streaming (stays alive). | High | New handler mode; requires lifecycle management (start, stop, error recovery). |
| **Handler middleware chain (extensible)** | User-defined middleware for logging, metrics, retries, auth. Composable via decorator chaining, not hardcoded. | High | Requires a `Middleware` trait and `ChainBuilder`; must integrate with `HandlerMode`. |
| **Async fan-out** | One message triggers multiple async handlers/sinks in parallel (e.g., process + audit + notify). | High | Requires `JoinSet` or similar for parallel async execution; must track all results. |
| **Async fan-in** | Multiple async sources (Kafka topics, async iterables) merged into one handler. | High | Requires async merge combinator; must handle source lifecycle and backpressure. |

---

## 2. Streaming Handler Patterns

### What It Is

A `@stream_handler` is a long-lived async Python function that receives an **async iterable** of messages, rather than single messages. It stays alive indefinitely (or until stopped) and processes messages as they arrive.

```python
# Streaming handler — stays alive, processes messages continuously
@consumer.stream_handler(topic="events", mode="streaming")
async def event_stream(stream):
    async for msg in stream:
        await process_event(msg)
```

**vs Batch handler**: Batch processes N messages then finishes. Streaming processes messages continuously until shutdown.

**vs Regular handler**: Regular handler receives one message per invocation. Streaming handler receives a stream object.

### Why Users Want It

- **WebSocket/SSE push**: Push Kafka messages to connected WebSocket clients in real-time
- **Background aggregation**: Continuously accumulate state from a stream without batch windows
- **Live dashboards**: Stream data to monitoring/analytics endpoints
- **Event sourcing**: Long-running processes that react to event streams

### Complexity Drivers

| Driver | Why Complex |
|--------|------------|
| Lifecycle management | Handler must start (subscribe), stay alive (loop), and stop (drain + cleanup) |
| Backpressure propagation | Stream must apply backpressure when consumer is overwhelmed |
| Error recovery | Restart stream on errors without losing partition assignment |
| Multiple concurrent streams | Multiple stream handlers may run simultaneously |

### Integration with Existing Code

The existing codebase already has:
- `HandlerMode::SingleAsync` — async single-message handler
- `BatchPolicy` — for batch accumulation
- `PythonAsyncFuture` — for driving Python coroutines

**Streaming extends this**: Instead of one-shot async invocation, streaming requires a persistent async loop that yields messages from a Rust async iterator.

### Proposed Architecture

```python
# Python API (proposed)
@consumer.stream_handler(topic="events", max_concurrent=5)
async def event_stream(stream):
    """
    stream is an async iterable that yields message dicts.
    Handler stays alive until consumer shutdown or error.
    """
    async for msg in stream:
        await process(msg)
```

```rust
// Rust handler mode (proposed addition to HandlerMode)
pub enum HandlerMode {
    // ... existing modes ...
    StreamingAsync,
}

// PythonHandler invokes a streaming handler by:
// 1. Creating an async iterator in Rust (AsyncMessageStream)
// 2. Passing it to Python as an async iterable via PythonAsyncFuture
// 3. Python's async for loop polls the iterator
```

### Feature Flag

- **Complexity**: High
- **Estimated effort**: 3-4 phases
- **Risk**: Stream lifecycle management is nontrivial; backpressure across async boundaries needs careful design.

---

## 3. Handler Middleware Chain

### What It Is

A composable chain of middleware functions that wrap handler execution. Each middleware can execute logic before and after the handler, modify arguments, or short-circuit on error.

```python
# Middleware chain — composed via decorator
@consumer.middleware(logging=True, metrics=True, retry_on_timeout=True)
@consumer.handler(topic="events")
async def handle_event(msg, ctx):
    await process(msg)
```

**Alternatives considered**:
- Explicit wrapping: `logged_handler = with_logging(with_metrics(basic_handler))` — verbose, hard to read
- Configuration-based: `handler(middleware=["logging", "metrics"])` — less flexible, magic strings

### Why Users Want It

- **Avoid boilerplate**: Logging, metrics, retry logic shouldn't be repeated in every handler
- **Separation of concerns**: Business logic vs cross-cutting concerns are separate
- **Reusability**: Same middleware can apply to multiple handlers

### Middleware Types

| Middleware | Before | After | On Error |
|-----------|--------|-------|----------|
| **Logging** | Log handler start with topic/partition/offset | Log handler completion with duration | Log exception with traceback |
| **Metrics** | Record handler start timestamp | Record latency histogram + throughput counter | Record error counter |
| **Retry** | — | — | Retry with backoff if TransientError |
| **Auth** | Check API key / JWT validity | — | Return 401 |
| **Validation** | Validate message schema | — | Return 400 + validation errors |

### Integration with Existing Code

Existing architecture already has:
- `ExecutionContext` — passed to handlers, could carry middleware-specific data
- `TracingSink` — already does span-based logging
- `MetricsSink` — already has throughput/latency metrics

**Middleware would wrap `PythonHandler::invoke`**: Before calling the actual handler, run middleware `before()`. After, run `after()`. On error, run `on_error()`.

### Proposed Architecture

```rust
// Middleware trait (proposed)
pub trait HandlerMiddleware: Send + Sync {
    fn name(&self) -> &str;
    async fn before(&self, ctx: &ExecutionContext, msg: &OwnedMessage);
    async fn after(&self, ctx: &ExecutionContext, msg: &OwnedMessage, result: &ExecutionResult);
    async fn on_error(&self, ctx: &ExecutionContext, msg: &OwnedMessage, error: &PyErr);
}

// Chain of middleware
pub struct MiddlewareChain {
    middlewares: Vec<Box<dyn HandlerMiddleware>>,
}

// Applied in PythonHandler::invoke before/after actual callback
impl PythonHandler {
    async fn invoke_with_middleware(&self, ctx: &ExecutionContext, msg: OwnedMessage) -> ExecutionResult {
        for mw in &self.middlewares {
            mw.before(ctx, &msg).await;
        }
        let result = self.invoke(ctx, msg).await;
        for mw in self.middlewares.iter().rev() {
            mw.after(ctx, &msg, &result).await;
        }
        result
    }
}
```

### Feature Flag

- **Complexity**: Medium (if limited to logging/metrics) to High (if extensible user middleware)
- **Estimated effort**: 2 phases
- **Risk**: Middleware ordering matters; user-defined middleware must be safe (no blocking, no GIL issues).

---

## 4. Async Handler Timeout

### What It Is

A mechanism to abort handler execution if it exceeds a configured time threshold. Timed-out handlers are classified as `Terminal(HandlerPanic)` and routed to DLQ.

### Current State

The codebase already has **partial implementation**:
- `handler_timeout: Option<Duration>` field in `PythonHandler` (line 146)
- `invoke_mode_with_timeout` wraps invocation in `tokio::time::timeout` (lines 273-310)
- On timeout, returns `ExecutionResult::Error` with `Terminal(HandlerPanic)` and descriptive traceback

**What's missing**:
- No Python API to configure timeout per handler
- No timeout metadata propagated to DLQ
- No way to distinguish hard timeout (process killed) from expected timeout behavior

### Proposed Python API

```python
@consumer.handler(topic="events", timeout=30.0)  # 30 seconds
async def handle_event(msg, ctx):
    await process(msg)

@consumer.handler(topic="slow-ops", timeout=300.0)  # 5 minutes for slow handlers
async def handle_slow(msg, ctx):
    await heavy_computation(msg)
```

### Timeout Behavior Options

| Option | Behavior | Use Case |
|--------|----------|----------|
| **Hard timeout** | Abort and DLQ immediately after threshold | Critical data — don't trust slow handlers |
| **Graceful timeout** | Send cancellation signal, wait for cleanup, then abort | Handlers that respect cancellation |
| **Reset timeout on activity** | Timeout only triggers if no messages processed for X seconds | Long-running but active handlers |

### Integration

- **DLQ metadata**: Timeout metadata should include `timeout_duration` and `last_processed_offset`
- **Metrics**: Timeout count metric per handler
- **Tracing**: Timeout should create a span event with `timeout` attribute

### Feature Flag

- **Complexity**: Low
- **Estimated effort**: 1 phase (mostly wiring existing implementation to Python API)
- **Risk**: Low — existing code is sound; just needs Python-facing API and DLQ metadata enrichment.

---

## 5. Async Fan-Out

### What It Is

A single message triggers **multiple async handlers or sinks** in parallel. The message is processed by multiple handlers simultaneously, and the framework waits for all results before committing.

```python
# Fan-out: one message → process + audit + notify
@consumer.handler(topic="orders", fan_out=["audit_topic", "notification_topic"])
async def handle_order(msg, ctx):
    await process_order(msg)
    # Audit and notification run in parallel via fan-out
```

### Why Users Want It

- **Audit trails**: Every order processed should also be logged to an audit topic
- **Multi-target updates**: Update database + invalidate cache + send notification
- **Parallel enrichment**: Enrich message with data from multiple async sources

### Architecture Options

| Pattern | Description | Trade-off |
|---------|-------------|-----------|
| **Fire-and-forget** | Spawn fan-out tasks, don't wait for results | Fast, but no guarantee of completion |
| **Wait-for-all** | Fan out in parallel via `JoinSet`, commit only after all succeed | Slower but guarantees all complete |
| **Wait-for-first** | Fan out, use first successful result, cancel others | For read-only enrichments |
| **Partial success** | Fan out, succeed if X% complete within timeout | Best effort with SLA |

### Integration with Existing Code

- Existing `Arc<Semaphore>` per handler key already limits concurrency
- `tokio::task::join_set::JoinSet` provides parallel task tracking
- `ExecutionResult` already supports success/error classification

**Proposed fan-out would use JoinSet**: Create N tasks, add to JoinSet, await all, aggregate results.

### Feature Flag

- **Complexity**: High
- **Estimated effort**: 2-3 phases
- **Risk**: 
  - Ordering: If handler A modifies state and handler B reads state, fan-out order matters
  - Partial failure: What if 2 of 3 fan-out targets succeed? Commit or rollback?
  - Complexity: Fan-out with full semantics (wait-for-all, partial success, cancellation) is substantial

---

## 6. Async Fan-In

### What It Is

Multiple async sources (Kafka topics, async iterables, queues) are **merged into a single handler**. The handler sees a unified stream from multiple sources.

```python
# Fan-in: merge multiple topics into one handler
@consumer.handler(topics=["events", "alerts", "system"], fan_in=True)
async def unified_handler(msg, ctx):
    source = ctx.source_topic  # Know which topic msg came from
    await process(msg)
```

### Why Users Want It

- **Unified processing**: Single handler for multiple topics (e.g., a dashboard that aggregates all events)
- **Cross-topic joins**: Combine events from multiple topics into one processing context
- **Migration**: Slowly move from multiple handlers to one without changing semantics

### Architecture Options

| Pattern | Description | Trade-off |
|---------|-------------|-----------|
| **Round-robin** | Interleave messages from sources | Simple, preserves ordering within source |
| **Priority** | Prefer one source over another | Use when one source is more time-sensitive |
| **Full merge** | All sources merged into one async stream | Complex, must handle backpressure from multiple sources |

### Integration with Existing Code

- Existing `QueueManager` has per-handler bounded channels
- `ConsumerRunner::next()` returns `OwnedMessage` from Kafka
- `PythonAsyncFuture` drives async Python coroutines

**Fan-in would extend QueueManager**: Instead of one source per handler, multiple sources feed one handler. Requires a merge strategy and per-source backpressure.

### Feature Flag

- **Complexity**: High
- **Estimated effort**: 2-3 phases
- **Risk**:
  - Partition ordering across topics is not guaranteed (each topic has independent partition ordering)
  - Backpressure from one source must not block other sources
  - Rebalance handling becomes more complex (multiple topic subscriptions)

---

## 7. Anti-Features

Features explicitly NOT building, with rationale.

| Anti-Feature | Why Avoid | Alternative |
|--------------|-----------|-------------|
| **Blocking middleware in async context** | GIL hold during middleware blocks Tokio event loop | Middleware must be async and release GIL on await |
| **Fan-out with distributed transactions** | Two-phase commit across handlers is not at-least-once semantics | Use idempotent handlers + at-least-once delivery |
| **Streaming handler with exactly-once** | Streaming + exactly-once requires checkpointing state, complexity explodes | Streaming is at-least-once; users add idempotency |
| **Middleware that mutates messages** | Hidden mutations break debugging and predictability | Middleware can validate, reject, log — not mutate |
| **Unbounded fan-out** | No limit on parallel handlers causes resource exhaustion | Configurable max fan-out degree with backpressure |

---

## 8. Feature Dependencies

```
Existing: HandlerMode, PythonHandler, invoke_mode_with_timeout
    │
    ├──► [1] Async Timeout ──────────► Table stakes (low complexity, wiring work)
    │
    ├──► [2] Streaming Handler ──────► Differentiator (high complexity, new lifecycle)
    │
    ├──► [3] Middleware Chain ────────► Differentiator (medium complexity, composable)
    │         │
    │         ├── logging middleware
    │         ├── metrics middleware
    │         └── retry middleware
    │
    ├──► [4] Fan-Out ─────────────────► Differentiator (high complexity, parallel execution)
    │         │
    │         └── depends on: timeout (for timeout on fan-out tasks)
    │
    └──► [5] Fan-In ─────────────────► Differentiator (high complexity, source merging)
              │
              └── depends on: streaming handler (shares async iterator infra)
```

**Critical path for streaming and fan-in share infrastructure**:
- `PythonAsyncFuture` (already exists)
- Async message iterator (new, needed for streaming)
- Merge combinator (new, needed for fan-in)

---

## 9. MVP Recommendation

### Phase 1: Async Timeout (Table Stakes)

**Priority**: HIGH — unblocks production safety issue (slow handlers blocking poll cycle)

**Deliverables**:
- Python API: `@handler(topic, timeout=30.0)`
- Timeout metadata in DLQ envelope
- Timeout metric (count per handler)
- Timeout span event in tracing

**Dependencies**: None (uses existing `invoke_mode_with_timeout`)

**Estimated effort**: 1 week

---

### Phase 2: Middleware Chain — Logging + Metrics (Differentiator)

**Priority**: HIGH — common request, significant boilerplate reduction

**Deliverables**:
- `HandlerMiddleware` trait in Rust
- Built-in logging middleware (before/after span events)
- Built-in metrics middleware (latency histogram, throughput counter)
- Python API to attach middleware per handler: `@handler(middleware=[Logging(), Metrics()])`
- Chaining support: middleware applied in order, reversed on response

**Dependencies**: Phase 1 (timeout middleware could reuse same pattern)

**Estimated effort**: 2 weeks

---

### Phase 3: Streaming Handler Patterns (Differentiator)

**Priority**: MEDIUM — enables new use cases (WebSocket, SSE, live dashboards)

**Deliverables**:
- `HandlerMode::StreamingAsync` 
- `PythonStreamingHandler` — wraps async iterator
- `AsyncMessageStream` in Rust — yields messages from Kafka
- Python API: `@stream_handler(topic)`
- Lifecycle management: start (subscribe), run (loop), stop (drain)
- Error recovery: restart stream on handler error without losing partition

**Dependencies**: None (new mode, no existing infra)

**Estimated effort**: 3-4 weeks

---

### Phase 4: Fan-Out (Differentiator)

**Priority**: MEDIUM — enables multi-target patterns but complex

**Deliverables**:
- `FanOutPolicy` enum: FireAndForget, WaitForAll, WaitForFirst, PartialSuccess
- Python API: `@handler(topic, fan_out=["audit", "notify"], fan_out_policy=WaitForAll)`
- `JoinSet`-based parallel execution
- Timeout propagation to fan-out tasks (reuse Phase 1)
- Partial failure handling (for PartialSuccess policy)

**Dependencies**: Phase 1 (timeout), Phase 2 (middleware for logging fan-out)

**Estimated effort**: 3 weeks

---

### Phase 5: Fan-In (Differentiator)

**Priority**: MEDIUM — enables unified stream processing

**Deliverables**:
- `MultiSourceHandler` — merges multiple sources
- Merge strategy: RoundRobin (default), Priority, FullMerge
- Per-source backpressure (each source queue independent)
- Python API: `@handler(topics=["events", "alerts"], fan_in=True)`
- Source identification in ExecutionContext (`source_topic` field)

**Dependencies**: Phase 3 (streaming handler async iterator infrastructure)

**Estimated effort**: 3-4 weeks

---

## 10. Feature Prioritization Matrix

| Feature | Priority | Complexity | Effort | Risk | Reason |
|---------|----------|------------|--------|------|--------|
| Async Timeout | **HIGH** | Low | 1 week | Low | Unblocks production issue; existing code foundation |
| Middleware (logging + metrics) | **HIGH** | Medium | 2 weeks | Low | Common request; existing observability infra |
| Streaming Handler | MEDIUM | High | 3-4 weeks | Medium | New lifecycle; complex but valuable |
| Fan-Out | MEDIUM | High | 3 weeks | Medium | Complex semantics (partial failure); high value |
| Fan-In | MEDIUM | High | 3-4 weeks | Medium | Complex backpressure; shares infra with streaming |

---

## 11. Sources

**Evidence basis**:
- Existing `PythonHandler::invoke_mode_with_timeout` (src/python/handler.rs:273-310) — timeout implementation confirmed
- `HandlerMode` enum (src/python/handler.rs:82-117) — mode classification confirmed
- `BatchPolicy` struct (src/python/handler.rs:120-137) — batch accumulation pattern confirmed
- `PythonAsyncFuture` (src/python/async_bridge.rs) — async Rust/Python bridge confirmed
- PROJECT.md v1.1 Active items — milestone scope confirmed
- PITFALLS.md Section 3 — async/sync execution model pitfalls documented

**Confidence**: MEDIUM — features are well understood from codebase inspection; architectural patterns are standard for async Rust/Python frameworks.