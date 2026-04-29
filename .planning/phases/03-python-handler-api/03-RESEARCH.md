# Phase 3: Python Handler API — Research

**Researched:** 2026-04-29
**Phase:** 03-python-handler-api
**Requirements:** PY-01, PY-02, PY-03, PY-04, PY-06, CONF-05
**Confidence:** MEDIUM-HIGH

## Summary

Phase 3 delivers ergonomic Python APIs on top of the Phase 1 Rust core. The existing codebase already has the core FFI machinery: `PythonHandler` (sync via `spawn_blocking`, async via `PythonAsyncFuture`), `HandlerMode` enum (SingleSync/SingleAsync/BatchSync/BatchAsync), `ExecutionContext` (topic/partition/offset/worker_id), and the `@handler` decorator already exists in `kafpy/runtime.py` with callable inspection. What remains: (1) formalize the `@handler` decorator API with per-handler concurrency control, (2) expose `ExecutionContext` with W3C trace context propagation, (3) implement context manager (`with Consumer(...) as c:`) for CONF-05, and (4) wire the handler registry into the Rust-side `RuntimeBuilder` via `PyConsumer.add_handler()`.

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| PY-01 | @handler decorator for registering topic handlers | Already implemented in `kafpy/runtime.py`; needs Rust registry integration |
| PY-02 | Sync Python handler support via spawn_blocking | `PythonHandler::invoke()` already uses `tokio::task::spawn_blocking` |
| PY-03 | Async Python handler support via pyo3-async-runtimes | `PythonAsyncFuture` + `PythonHandler::invoke_async()` already implemented |
| PY-04 | ExecutionContext passed to handlers with trace context | `ExecutionContext` exists; W3C trace context injection via `inject_trace_context()` stub |
| PY-06 | Handler concurrency configuration per handler | `Arc<Semaphore>` pattern needed per-handler; not yet wired to Python decorator |
| CONF-05 | Context manager support (with statement for clean shutdown | Needs `__enter__`/`__exit__` on PyConsumer |

---

## Implementation Approaches

### PY-01: @handler Decorator

#### Approach A: Python-side wrapper (existing, works)

**What:** The decorator is entirely in Python (`kafpy/runtime.py`), wrapping the callable and calling `consumer.add_handler()`.

**Pros:** Simple, no new PyO3 code needed.
**Cons:** The Rust side receives a `Py<PyAny>` wrapped in a closure — fine, but the decorator signature is not declarative (no way to specify concurrency in the decorator itself).

**Implementation (existing):**
```python
# kafpy/runtime.py
def handler(self, topic: str, *, timeout_ms: int | None = None) -> Callable[[Callable], Callable]:
    def decorator(fn: Callable) -> Callable:
        self.register_handler(topic, fn, timeout_ms=timeout_ms)
        return fn
    return decorator
```

#### Approach B: Rust-side decorator via pyo3 `#[pyclass]` with `__call__`

**What:** A Rust struct `HandlerDecorator` that is instantiated via `#[new]` and wraps a Python callable. Its `__call__` method registers the callable in a registry.

**Pros:** Type-safe decorator parameters, decorator can be subclassed.
**Cons:** More PyO3 code; callable wrapping is redundant since Python already does this.

**Recommendation:** Use **Approach A** (existing) — Python decorator is sufficient and simpler. The `handler()` method already registers with the Rust consumer via `add_handler()`. The only enhancement needed is per-handler concurrency (PY-06) and explicit `__enter__`/`__exit__` for context manager support.

---

### PY-02 / PY-03: Sync and Async Handler Execution

Both are **already implemented** in `src/python/handler.rs`:

| Mode | Function | Mechanism |
|------|----------|-----------|
| SingleSync | `PythonHandler::invoke()` | `tokio::task::spawn_blocking` + `Python::attach` |
| SingleAsync | `PythonHandler::invoke_async()` | `PythonAsyncFuture` (custom GIL-bridge Future) |
| BatchSync | `PythonHandler::invoke_batch()` | `spawn_blocking` with `Vec<OwnedMessage>` |
| BatchAsync | `PythonHandler::invoke_batch_async()` | `PythonAsyncFuture` with batch |

**Key patterns:**

```rust
// Sync: GIL acquired only inside spawn_blocking
pub async fn invoke(&self, ctx: &ExecutionContext, message: OwnedMessage) -> ExecutionResult {
    let callback = Arc::clone(&self.callback);
    tokio::task::spawn_blocking(move || {
        Python::attach(|py| {
            let py_msg = message_to_pydict(py, &message, None);
            let py_ctx = ctx_to_pydict(py, ctx, &message);
            callback.call(py, (py_msg, py_ctx), None)
        })
    }).await?
}
```

```rust
// Async: coroutine created inside with_gil, driven by PythonAsyncFuture
pub async fn invoke_async(&self, _ctx: &ExecutionContext, message: OwnedMessage) -> ExecutionResult {
    let coro: Py<PyAny> = Python::attach(|py| {
        let py_msg = message_to_pydict(py, &message, None);
        callback.call1(py, (py_msg,)).expect("coroutine function")
    });
    PythonAsyncFuture::from(coro).await
}
```

**No changes needed** — the FFI machinery is in place. The remaining work is wiring the per-handler concurrency (PY-06).

---

### PY-04: ExecutionContext with Trace Context

**Already implemented:**
- `ExecutionContext` struct in `src/python/context.rs`: topic, partition, offset, worker_id
- `message_to_pydict()` injects `_trace_context` dict into the Python message dict
- `inject_trace_context()` in `src/observability/tracing.rs` is a stub (no-op)

**What needs work:** The `inject_trace_context()` function is currently a no-op. For OBS-02 (W3C trace context propagation), this needs to parse `traceparent` and `tracestate` headers from Kafka message headers and inject into the HashMap.

**Implementation path:**
```rust
// src/observability/tracing.rs — implement properly
pub fn inject_trace_context(headers: &std::collections::HashMap<String, String>, out: &mut HashMap<String, String>) {
    // Parse traceparent: 00-{trace_id}-{span_id}-{flags}
    if let Some(tp) = headers.get("traceparent") {
        // split and extract trace_id, span_id
        out.insert("trace_id", trace_id);
        out.insert("span_id", span_id);
    }
}
```

This is Phase 4 work (OBS-02). For Phase 3, the stub is sufficient — the `ExecutionContext` is already passed to handlers.

---

### PY-06: Per-Handler Concurrency Configuration

**Goal:** Limit how many concurrent executions of the same handler can run simultaneously (e.g., concurrency=5 means at most 5 instances of handler for topic "my-topic" running at once).

#### Approach A: Arc\<Semaphore\> per handler key (recommended)

**What:** Store `Arc<Semaphore>` in the handler registry. Each `PythonHandler` acquisition is a permit. On drop, the permit is released.

**Implementation:**
```rust
// src/worker_pool/pool.rs or new src/handler/concurrency.rs
use std::sync::Arc;
use tokio::sync::Semaphore;

pub struct HandlerConcurrency {
    semaphores: HashMap<String, Arc<Semaphore>>,
    default_limit: usize,
}

impl HandlerConcurrency {
    pub fn new(default_limit: usize) -> Self { ... }
    pub fn permit_for(&self, handler_id: &str) -> Arc<Semaphore> { ... }
    pub async fn acquire(&self, handler_id: &str) -> tokio::sync::OwnedSemaphorePermit<'_> {
        self.semaphores
            .entry(handler_id.to_string())
            .or_insert_with(|| Arc::new(Semaphore::new(self.default_limit)))
            .acquire()
            .await
            .unwrap()
    }
}
```

**Usage in worker loop:**
```rust
// In worker_loop.rs — acquire permit before invoking handler
let permit = concurrency_control.acquire(&handler_id).await;
let result = handler.invoke_mode_with_timeout(ctx, message).await;
drop(permit); // permit released on drop
```

**Python API:**
```python
# Option 1: decorator parameter
@app.handler(topic="my-topic", concurrency=10)
def handle(msg): ...

# Option 2: global config
config = ConsumerConfig(..., default_concurrency=5)
```

#### Approach B: Channel-based backpressure

**What:** Use a bounded channel per handler; full channel = backpressure signal.

**Cons:** Already implemented (per-handler bounded queues in Phase 1). Concurrency semaphore is more precise for per-handler limits.

**Recommendation:** Use **Approach A** (`Arc<Semaphore>` per handler key). This is explicit, composable, and composes with existing bounded queues.

---

### CONF-05: Context Manager Support

**Goal:** `with Consumer(config) as c:` pattern for deterministic cleanup.

**Implementation:** Add `__enter__` and `__exit__` to `PyConsumer`:

```rust
// src/pyconsumer.rs
#[pymethods]
impl PyConsumer {
    fn __enter__(&mut self) -> PyResult<Py<PyAny>> {
        Ok(self.into_py_any())
    }

    fn __exit__(&mut self, exc_type: Option<&PyType>, exc_val: Option<&PyAny>, traceback: Option<&PyAny>) -> PyResult<bool> {
        self.stop();
        Ok(false) // don't suppress exceptions
    }
}
```

**Usage:**
```python
config = ConsumerConfig(...)
with Consumer(config) as consumer:
    consumer.add_handler("topic", handler)
    consumer.start()
# Automatic stop() on exit
```

**Alternative:** `KafPy` class already has `stop()` — add `__enter__`/`__exit__` there too.

---

## Key Files to Modify

| File | Changes | Requirements |
|------|---------|--------------|
| `kafpy/runtime.py` | Add `concurrency` parameter to `handler()` decorator; propagate to `add_handler()` | PY-01, PY-06 |
| `kafpy/handlers.py` | Add `concurrency` field to handler registration; update type stubs | PY-01, PY-06 |
| `kafpy/consumer.py` | Add `__enter__`/`__exit__` for context manager support | CONF-05 |
| `src/pyconsumer.rs` | Add `__enter__`/`__exit__` methods; pass concurrency to handler metadata | CONF-05, PY-06 |
| `src/python/handler.rs` | Accept concurrency limit; integrate with Arc\<Semaphore\> | PY-06 |
| `src/worker_pool/pool.rs` | Add HandlerConcurrency with Arc\<Semaphore\> per handler | PY-06 |
| `src/python/context.rs` | Extend ExecutionContext with trace_id, span_id fields | PY-04 |
| `src/observability/tracing.rs` | Implement inject_trace_context() (currently stub) | PY-04, OBS-02 |
| `src/runtime/builder.rs` | Wire concurrency config from Python handler registration | PY-06 |
| `kafpy/_kafpy.pyi` | Update type stubs for new parameters | PY-01, PY-06 |

---

## Pyo3 FFI Patterns

### Pattern 1: Receiving a Python callable in Rust

```rust
// src/pyconsumer.rs — add_handler receives Bound<'_, PyAny>
#[pyo3(signature = (topic, callback, mode=None, batch_max_size=None, batch_max_wait_ms=None, timeout_ms=None))]
pub fn add_handler(
    &mut self,
    topic: String,
    callback: Bound<'_, PyAny>,
    mode: Option<String>,
    batch_max_size: Option<usize>,
    batch_max_wait_ms: Option<u64>,
    timeout_ms: Option<u64>,
) {
    let meta = HandlerMetadata {
        callback: Arc::new(callback.unbind()),  // Arc<Py<PyAny>> — GIL-independent, Send+Sync
        mode: HandlerMode::from_opt_str(mode.as_deref()),
        batch_max_size,
        batch_max_wait_ms,
        timeout_ms,
    };
    self.handlers.lock().unwrap().insert(topic, meta);
}
```

**Key insight:** `Arc<Py<PyAny>>` is GIL-independent and can be cloned and sent across thread boundaries. The GIL is only acquired inside `Python::attach` or `Python::with_gil`.

### Pattern 2: spawn_blocking for sync Python calls

```rust
// src/python/handler.rs — invoke()
tokio::task::spawn_blocking(move || {
    Python::attach(|py| {
        // GIL acquired here; safe to call Python objects
        let result = callback.call(py, (py_msg, py_ctx), None);
        // Convert to ExecutionResult
        match result {
            Ok(_) => ExecutionResult::Ok,
            Err(e) => ExecutionResult::Error { ... },
        }
    })
}).await
```

**GIL lifecycle:** Acquired on `Python::attach`, held for the duration of the closure, released immediately after. The Tokio thread is blocked only during this window — other Tokio tasks continue running.

### Pattern 3: PythonAsyncFuture for async Python calls

```rust
// src/python/async_bridge.rs
pub struct PythonAsyncFuture {
    coro: Py<PyAny>,  // GIL-independent
}

impl Future for PythonAsyncFuture {
    type Output = ExecutionResult;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Python::attach(|py| {
            let result = self.coro.call_method1(py, "send", (py.None(),));
            match result {
                Ok(_) => { cx.waker().wake_by_ref(); Poll::Pending }
                Err(py_err) if py_err.is_instance_of::<PyStopAsyncIteration>(py) => Poll::Ready(ExecutionResult::Ok),
                Err(py_err) => Poll::Ready(ExecutionResult::Error { ... }),
            }
        })
    }
}
```

**Key insight:** GIL acquired only during `coro.send(None)`, immediately released. The coroutine advances on each poll. Waker is called when the coroutine yields (not on every poll).

### Pattern 4: Decorator implemented as Python wrapper (PY-01)

```python
# kafpy/runtime.py — existing pattern
class KafPy:
    def handler(self, topic: str, *, concurrency: int | None = None, timeout_ms: int | None = None):
        def decorator(fn):
            self.register_handler(topic, fn, concurrency=concurrency, timeout_ms=timeout_ms)
            return fn
        return decorator
```

No new PyO3 code required. The decorator is pure Python.

### Pattern 5: Semaphore per handler for concurrency (PY-06)

```rust
// src/worker_pool/concurrency.rs — new file
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Semaphore;

pub struct HandlerConcurrency {
    semaphores: HashMap<String, Arc<Semaphore>>,
    default_limit: usize,
}

impl HandlerConcurrency {
    pub async fn acquire(&self, handler_id: &str) -> tokio::sync::OwnedSemaphorePermit<'_> {
        let sem = self.semaphores
            .entry(handler_id.to_string())
            .or_insert_with(|| Arc::new(Semaphore::new(self.default_limit)))
            .clone();
        sem.acquire().await.expect("semaphore closed")
    }
}
```

---

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust `#[test]` + `#[tokio::test]` + Python `pytest` |
| Quick run | `cargo test --lib && python -m pytest` |
| Full suite | `cargo test --all && python -m pytest tests/` |

### Phase Requirements to Test Map

| Req ID | Behavior | Test Type | File |
|--------|----------|-----------|------|
| PY-01 | @handler decorator registers callable and topic | unit | `kafpy/test_handlers.py` (new) |
| PY-02 | sync handler executed via spawn_blocking returns Ok | unit | `src/python/handler.rs` existing |
| PY-02 | sync handler raises exception returns Error with reason | unit | `src/python/handler.rs` existing |
| PY-03 | async handler executed via PythonAsyncFuture returns Ok | unit | `src/python/async_bridge.rs` existing |
| PY-04 | ExecutionContext passed to handler contains trace context | unit | `src/python/context.rs` (extend) |
| PY-04 | W3C traceparent header parsed and injected | unit | `src/observability/tracing.rs` (implement) |
| PY-06 | Semaphore limits concurrent handler invocations | unit | `src/worker_pool/concurrency.rs` (new) |
| PY-06 | concurrency parameter in @handler decorator | integration | `kafpy/test_handlers.py` (new) |
| CONF-05 | `with Consumer(config) as c:` calls stop() on exit | unit | `src/pyconsumer.rs` |

### Wave 0 Gaps
- [ ] `src/worker_pool/concurrency.rs` — HandlerConcurrency with Arc\<Semaphore\> per handler; covers PY-06
- [ ] `kafpy/test_handlers.py` — Python handler tests (decorator, concurrency, context manager); covers PY-01, PY-06, CONF-05
- [ ] `src/observability/tracing.rs` — implement inject_trace_context(); covers PY-04 (partial, full OBS-02 in Phase 4)
- [ ] `src/python/context.rs` — extend ExecutionContext with trace_id, span_id; covers PY-04
- [ ] `kafpy/_kafpy.pyi` — update type stubs; covers PY-01, PY-06

---

## Assumptions Log

| # | Claim | Risk if Wrong |
|---|-------|---------------|
| A1 | `PythonHandler::invoke()` and `invoke_async()` work as expected | Need integration test |
| A2 | `Arc<Py<PyAny>>` for callback is Send+Sync (confirmed in codebase) | Already verified |
| A3 | Python decorator can pass `concurrency` through to Rust via existing `add_handler` path | No new PyO3 needed |
| A4 | `__enter__`/`__exit__` on PyConsumer is sufficient for context manager | Python wrapper also needs it |

---

## Open Questions

1. **Trace context injection timing**: `inject_trace_context()` is called before `spawn_blocking` in `handler.rs`. This is correct — extract W3C headers before crossing GIL boundary.

2. **Per-handler vs global concurrency**: Should concurrency be per-topic-handler (e.g., 10 concurrent for topic A, 5 for topic B) or a global pool size? Per-handler is more flexible — recommend per-handler via decorator parameter.

3. **Bounded queues vs semaphores**: Phase 1 already has bounded queues per handler. The semaphore adds explicit concurrency limits on top. These compose — queue for buffering, semaphore for parallelism.

---

## Sources

### Primary (HIGH confidence)
- `src/python/handler.rs` — PythonHandler with spawn_blocking and PythonAsyncFuture
- `src/python/async_bridge.rs` — PythonAsyncFuture GIL-bridge Future implementation
- `src/python/context.rs` — ExecutionContext (topic, partition, offset, worker_id)
- `src/observability/tracing.rs` — inject_trace_context stub
- `src/pyconsumer.rs` — PyConsumer.add_handler() receiving Py<PyAny>
- `kafpy/runtime.py` — KafPy.handler() decorator (existing)
- `kafpy/consumer.py` — Consumer wrapper (needs __enter__/__exit__)
- `kafpy/handlers.py` — KafkaMessage, HandlerContext types

### Secondary (MEDIUM confidence)
- `src/runtime/builder.rs` — RuntimeBuilder assembly
- `src/worker_pool/pool.rs` — WorkerPool (needs concurrency integration)
- Phase 2 research for patterns on rebalance/wiring gaps

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — pyo3 0.27.2, pyo3-async-runtimes 0.27.0 confirmed in Cargo.toml
- Architecture: MEDIUM-HIGH — core FFI machinery exists; remaining work is integration
- Pitfalls: MEDIUM — GIL patterns well-understood; need to verify Arc\<Semaphore\> integration with worker loop

**Research date:** 2026-04-29
**Valid until:** 2026-05-29 (30 days)