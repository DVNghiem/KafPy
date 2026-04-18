# Feature Research

**Domain:** Kafka message processing -- batch and async Python handlers
**Researched:** 2026-04-18
**Confidence:** MEDIUM

## Feature Landscape

### Table Stakes (Users Expect These)

Features users assume exist. Missing these = product feels incomplete.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Batch message accumulation | Standard throughput optimization; users expect batch=size OR batch=timeout | MEDIUM | Accumulator in execution layer, not in rdkafka consumer core |
| Batch handler invocation | Natural extension of batch accumulation; same Python handler called with list | MEDIUM | Handler receives `List[PyDict]` not `PyDict`; result is batch-level |
| Per-handler batch config | Different handlers may need different batch sizes; e.g., bulk DB inserts vs. real-time | LOW | Add `max_batch_size: usize`, `max_batch_wait_ms: u64` to `PythonHandler` config |
| Handler execution mode enum | Single-sync (existing) vs. single-async vs. batch-sync vs. batch-async -- all must be representable | MEDIUM | `enum class HandlerMode` in Python, `enum HandlerMode` in Rust |
| Async Python handler invocation | Users with async Python code expect `async def handler(messages)` to work | HIGH | Requires `pyo3-async-runtimes` crate; `into_future()` for Python-to-Rust, `future_into_py()` for Rust-to-Python |
| GIL minimal usage (no GIL across Rust orchestration) | Core design principle from PROJECT.md; must hold GIL only inside Python execution | HIGH | sync handlers: `spawn_blocking` (existing pattern); async handlers: event-loop integration via pyo3-async-runtimes tokio adapter |

### Differentiators (Competitive Advantage)

Features that set the product apart. Not required, but valuable.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Unified handler mode API | One Python API surface for all 4 modes; user just passes callable, KafPy detects and routes | LOW | Detect `asyncio.iscoroutinefunction` vs `callable`; batch vs single via config flag |
| Batch result partial failure | When batch partially succeeds, provide per-message outcome; avoid full retry | MEDIUM | Return `List[ExecutionResult]` alongside batch result; map back to offsets |
| Tokio-native async Python handlers | No thread blocking during async Python I/O; scales to many concurrent async handlers | HIGH | Requires event loop lifecycle management; Python event loop runs on a dedicated tokio task |

### Anti-Features (Commonly Requested, Often Problematic)

Features that seem good but create problems.

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| Per-message retry within a batch | "Some messages failed, retry just those" | Batch atomicity expectations conflict with partial retry; offset commit semantics become complex | Full-batch retry on any failure; individual messages go to DLQ after exhausted retries |
| Automatic batch detection | "Figure out if handler wants batch or single" | Heuristics are fragile; user intent is unclear | Explicit via config: `handler_mode="batch"` |
| GIL-free Rust orchestration | "Never hold GIL anywhere" | Implies all message processing is async and non-blocking at Python boundary; extremely complex | GIL held only inside Python handler invocation (single message or batch) -- this is already minimal |

## Feature Dependencies

```
[BatchAccumulator]
    └──requires──> [HandlerMode enum]
                       └──requires──> [BatchHandlerImpl]

[AsyncPythonHandler]
    └──requires──> [pyo3-async-runtimes]
                       └──requires──> [TokioEventLoop integration]
                                           └──requires──> [BatchAccumulator] (for async batch)

[HandlerMode enum]
    └──enhances──> [PythonHandler invoke signature]

[PythonHandler.invoke]
    ├──sync path──> [spawn_blocking] (existing, unchanged)
    └──async path──> [pyo3_async_runtimes::into_future()]
```

### Dependency Notes

- **BatchAccumulator requires HandlerMode enum:** The accumulator must know whether to deliver single messages or batches; this is determined by handler mode config.
- **AsyncPythonHandler requires pyo3-async-runtimes:** This crate provides `into_future()` to convert Python coroutines to Rust Futures that tokio can await. Without it, async Python handlers cannot be integrated into the tokio executor.
- **pyo3-async-runtimes requires TokioEventLoop integration:** The Python asyncio event loop must be created and managed within a tokio context. This is non-trivial because the Python event loop is not thread-safe with tokio tasks.
- **spawn_blocking (existing) handles sync batch:** The existing `spawn_blocking` pattern for sync handlers does not change for batch mode -- only the Python callable signature changes (receives a list).

## MVP Definition

### Launch With (v1.6)

Minimum viable product -- what is needed to validate the concept.

- [ ] **HandlerMode enum** -- Represent single-sync, single-async, batch-sync, batch-async as a Rust enum and Python class. This is the core abstraction that gates all other work.
- [ ] **Batch accumulation logic** -- Accumulator struct holding messages with a size limit (max_batch_size) and time limit (max_batch_wait_ms). Triggers on whichever limit is hit first. Lives in execution layer above PythonHandler.
- [ ] **Batch handler invoke (sync)** -- Extend `PythonHandler.invoke` to accept `Vec<OwnedMessage>`. The sync Python handler receives a list of PyDict messages. Result is a single `ExecutionResult` for the batch (all-or-nothing success).
- [ ] **Handler execution mode abstraction** -- `WorkerPool` selects the appropriate invoke path based on handler mode. Mode is set at handler registration time.
- [ ] **GIL minimal usage (verified)** -- Confirm GIL is only held inside `Python::with_gil` blocks for sync handlers and inside `into_future().await` for async. Rust-side orchestration (dispatch, queue management, offset tracking) holds no GIL.

### Add After Validation (v1.x)

- [ ] **Async Python handler invoke** -- Use `pyo3_async_runtimes::tokio::future_into_py()` and `into_future()` to bridge Python coroutines and tokio Futures. Async batch handlers accumulate messages same as sync but call `async def handler(messages)` via the async runtime bridge.
- [ ] **Partial failure handling for batch** -- When a batch returns mixed results, track per-message outcomes and route individual messages to retry/DLQ rather than whole batch.
- [ ] **Python-side API for handler registration** -- `consumer.on_topic("events", handler, mode="batch", batch_size=100, batch_timeout_ms=500)` unified registration surface.

### Future Consideration (v2+)

- [ ] **Streaming batch (windowed)** -- Sliding window or session window over messages; useful for aggregations.
- [ ] **Schema validation at batch boundary** -- Validate entire batch against Avro/JSON schema before processing.

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| Batch accumulation logic | HIGH | MEDIUM | P1 |
| HandlerMode enum | HIGH | LOW | P1 |
| Batch handler invoke (sync) | HIGH | MEDIUM | P1 |
| Handler execution mode abstraction | HIGH | MEDIUM | P1 |
| GIL minimal usage verification | HIGH | LOW | P1 |
| Async Python handler via pyo3-async-runtimes | HIGH | HIGH | P2 |
| Partial failure per-message tracking | MEDIUM | HIGH | P2 |
| Python-side unified registration API | MEDIUM | LOW | P2 |
| Streaming windowed batches | LOW | VERY HIGH | P3 |

**Priority key:**
- P1: Must have for v1.6 launch
- P2: Should have, add when possible
- P3: Nice to have, future consideration

## Competitor Feature Analysis

| Feature | rdkafka (Python) | aiokafka | Faust | Our Approach |
|---------|-----------------|----------|-------|--------------|
| Batch accumulation | Application-level (user implements) | Application-level | Agent has `Agent.getmany()` yielding batches | Accumulator struct in execution layer; size OR timeout trigger |
| Batch handler | User-defined loop | User-defined loop | `@app.agent(topic, batch=True)` decorator | `PythonHandler.invoke_batch` + config flag |
| Async Python | N/A (rdkafka is sync) | `async def` consumer methods via `await consumer.getone()` | `@app.agent` is async by default | pyo3-async-runtimas bridge with `into_future()` |
| Per-message retry within batch | Manual | Manual | Manual | Full-batch retry on failure; per-message DLQ after exhaustion |
| GIL held during orchestration | N/A (C extension) | N/A (Python) | N/A (Python) | GIL only inside Python handler invoke |

## How Batch Processing Works (Detailed)

### rdkafka Level

rdkafka fetches from brokers in batches controlled by broker config:
- `fetch.min.bytes` (default 1) -- minimum bytes to fetch from broker
- `fetch.max.wait.ms` (default 500ms) -- maximum wait time for broker to fill a batch
- `queued.min.messages` (default 100000) -- minimum messages to queue before triggering a fetch

The `StreamConsumer.recv()` in rdkafka yields one message at a time. The batching happens at the network/protocol level internally -- **not** as a batch API. KafPy's application-level batching is orthogonal: we accumulate messages after they are already fetched one-by-one from rdkafka.

### Application-Level Batching

The execution layer accumulates `OwnedMessage` values in a buffer:
- Buffer has `max_batch_size` (number of messages) and `max_batch_wait_ms` (timeout)
- When either limit is reached, the batch is delivered to the Python handler
- The accumulator is per-handler-queue worker; each worker has its own accumulator state
- Timer resets after each batch dispatch

**Implementation sketch:**
```rust
struct BatchAccumulator {
    messages: Vec<OwnedMessage>,
    max_size: usize,
    max_wait: Duration,
    created_at: Instant,
}

impl BatchAccumulator {
    fn push(&mut self, msg: OwnedMessage) -> Option<Vec<OwnedMessage>> {
        self.messages.push(msg);
        if self.messages.len() >= self.max_size {
            Some(self.messages.drain(..).collect())
        } else if self.created_at.elapsed() >= self.max_wait && !self.messages.is_empty() {
            Some(self.messages.drain(..).collect())
        } else {
            None
        }
    }
}
```

### Sync vs Async Handler Invocation

**Sync handler (existing pattern):**
- Worker calls `PythonHandler.invoke(ctx, message)`
- `invoke` wraps Python callable in `spawn_blocking`; GIL acquired inside
- Pattern: `tokio::task::spawn_blocking(move || Python::with_gil(|py| callback.call1(py, (py_msg,))))`
- Result: `ExecutionResult` (single)

**Async handler (new pattern):**
- Worker calls `PythonHandler.invoke_async(ctx, messages)`
- Uses `pyo3_async_runtimes::tokio::into_future()` to convert Python coroutine to Rust Future
- The Rust Future is awaited directly on the tokio executor (no `spawn_blocking` needed)
- GIL acquired transiently by pyo3's future polling; not held across await points
- Requires Python event loop running on a dedicated tokio task

**Batch sync handler:**
- Worker calls `PythonHandler.invoke_batch(ctx, messages)`
- `messages: Vec<OwnedMessage>` converted to `Vec<PyDict>` inside `spawn_blocking`
- Python callable receives `List[Dict]` (or tuple of dicts)
- Returns single `ExecutionResult::Ok` or `ExecutionResult::Error` (batch-level)
- On error: entire batch enters retry/DLQ flow

**Async batch handler:**
- Same as batch sync but callable is `async def handler(messages: List[Dict]) -> None`
- Bridged via `into_future()` same as single async

### Per-Message Outcome Tracking

When batch result model supports partial failure:
- Python handler returns `List[ExecutionResult]` or `Dict[str, ExecutionResult]` (indexed by offset)
- Worker loop maps each outcome back to the corresponding `OwnedMessage`
- Each message individually enters retry/DLQ pipeline
- Offset commit only advances when all prior offsets are resolved (existing behavior unchanged)

## Sources

- rdkafka StreamConsumer API -- message yielded one-at-a-time via `recv()`, not batched at API level
- pyo3-async-runtimes (`into_future()`, `future_into_py()`) -- bridges Python coroutines and tokio Futures
- aiokafka batch API -- application-level accumulation, user-defined handler loops
- Faust streaming/faust -- `@app.agent` with `batch=True` option, `getmany()` method
- Confluent Python client docs -- producer/consumer API surface, not rdkafka internals
