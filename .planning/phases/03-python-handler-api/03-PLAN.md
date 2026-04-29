# Phase 3: Python Handler API — Plan

**Date**: 2026-04-29
**Wave**: 1 + 2
**Phase**: 03-python-handler-api
**Depends on**: Phase 1 (CORE-01..CORE-05, CORE-07, MSG-01, MSG-02, MSG-04, MSG-05, OFF-01..OFF-03, CONF-01, CONF-02, CONF-04)
**Confidence**: MEDIUM-HIGH

## Overview

Phase 3 delivers the ergonomic Python handler API. The FFI machinery (sync/async via spawn_blocking, PythonAsyncFuture, HandlerMode, ExecutionContext) already exists in `src/python/handler.rs` and `src/python/context.rs`. The research confirmed three gaps: (1) `inject_trace_context()` is a no-op stub, (2) `ExecutionContext` lacks trace_id/span_id fields, (3) per-handler `Arc<Semaphore>` concurrency control not wired, (4) no `__enter__`/`__exit__` context manager on `PyConsumer`.

This plan splits into two waves:
- **Wave 1** (03.1): trace context injection + per-handler concurrency via `Arc<Semaphore>`
- **Wave 2** (03.2): context manager (`with Consumer as c:`) + decorator `concurrency` parameter

---

## Wave 1 — trace context + per-handler concurrency

### Frontmatter

```yaml
phase: "03"
slug: python-handler-api
plan: "01"
type: execute
wave: 1
depends_on: []
files_modified:
  - src/observability/tracing.rs
  - src/python/context.rs
  - src/worker_pool/concurrency.rs
  - src/pyconsumer.rs
  - src/worker_pool/worker.rs
  - src/worker_pool/pool.rs
  - kafpy/runtime.py
autonomous: true
requirements:
  - PY-04
  - PY-06
```

### Objective

Implement W3C trace context injection (`inject_trace_context()`) so that Kafka message headers carrying `traceparent` are parsed and the `trace_id`/`span_id` are injected into the Python handler's `ExecutionContext`. Also implement per-handler concurrency limits via `Arc<Semaphore>` so that when `concurrency=N` is configured, at most N instances of that handler run simultaneously.

### Context

@src/observability/tracing.rs
@src/python/context.rs
@src/python/handler.rs
@src/worker_pool/pool.rs
@src/worker_pool/worker.rs
@src/pyconsumer.rs
@kafpy/runtime.py

### Tasks

<task type="auto">
  <name>Task 1: Implement inject_trace_context() in src/observability/tracing.rs</name>
  <files>src/observability/tracing.rs</files>
  <read_first>src/observability/tracing.rs, src/python/handler.rs, src/python/context.rs</read_first>
  <action>
    Replace the no-op `inject_trace_context()` in `src/observability/tracing.rs` with a real W3C traceparent parser:

    ```rust
    /// Parse W3C traceparent header and inject trace_id + span_id into the output map.
    ///
    /// Format: 00-{trace_id:32}-{span_id:16}-{flags:2}
    /// Examples:
    ///   00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01
    ///   00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01
    pub fn inject_trace_context(
        headers: &std::collections::HashMap<String, String>,
        out: &mut std::collections::HashMap<String, String>,
    ) {
        // W3C traceparent header name
        if let Some(traceparent) = headers.get("traceparent") {
            let parts: Vec<&str> = traceparent.split('-').collect();
            if parts.len() == 4 {
                // parts[0] = version (always "00" for this spec)
                // parts[1] = trace_id (32 hex chars)
                // parts[2] = span_id (16 hex chars)
                // parts[3] = flags (e.g., "01")
                out.insert("trace_id".to_string(), parts[1].to_string());
                out.insert("span_id".to_string(), parts[2].to_string());
                out.insert("trace_flags".to_string(), parts[3].to_string());
            }
        }
        // Also handle tracestate if present
        if let Some(tracestate) = headers.get("tracestate") {
            out.insert("tracestate".to_string(), tracestate.clone());
        }
    }
    ```

    IMPORTANT: `message_to_pydict()` in `handler.rs` already calls `inject_trace_context(&mut trace_context)` before crossing the GIL boundary. The fix is only in tracing.rs — the caller code is already correct.
  </action>
  <verify>
    <automated>cargo build --lib 2>&1 | head -30</automated>
  </verify>
  <done>
    inject_trace_context() parses traceparent header and injects trace_id, span_id into HashMap
  </done>
</task>

<task type="auto">
  <name>Task 2: Extend ExecutionContext with trace_id and span_id fields</name>
  <files>src/python/context.rs</files>
  <read_first>src/python/context.rs, src/python/handler.rs</read_first>
  <action>
    Extend `ExecutionContext` in `src/python/context.rs` to carry trace context:

    ```rust
    #[derive(Debug, Clone)]
    pub struct ExecutionContext {
        pub topic: String,
        pub partition: i32,
        pub offset: i64,
        pub worker_id: usize,
        /// W3C trace_id (32 hex chars from traceparent)
        pub trace_id: Option<String>,
        /// W3C span_id (16 hex chars from traceparent)
        pub span_id: Option<String>,
        /// W3C trace flags (e.g., "01")
        pub trace_flags: Option<String>,
    }
    ```

    Update `ExecutionContext::new()` to accept these new fields (with `None` defaults for backward compatibility):
    ```rust
    pub fn new(
        topic: String,
        partition: i32,
        offset: i64,
        worker_id: usize,
        trace_id: Option<String> = None,
        span_id: Option<String> = None,
        trace_flags: Option<String> = None,
    ) -> Self { ... }
    ```

    Also update `ctx_to_pydict()` in `handler.rs` to inject these into the Python context dict so handlers can access them via `ctx._trace_context`:
    ```rust
    if let Some(ref tid) = ctx.trace_id {
        let _ = py_ctx.set_item("trace_id", tid);
    }
    if let Some(ref sid) = ctx.span_id {
        let _ = py_ctx.set_item("span_id", sid);
    }
    ```

    Also update the places in handler.rs that construct ExecutionContext (e.g., invoke_batch and invoke_async) to pass trace context.
  </action>
  <verify>
    <automated>cargo build --lib 2>&1 | head -30</automated>
  </verify>
  <done>
    ExecutionContext has trace_id, span_id, trace_flags fields; ctx_to_pydict injects them into Python context dict
  </done>
</task>

<task type="auto">
  <name>Task 3: Wire trace context from Kafka message headers into ExecutionContext</name>
  <files>src/python/handler.rs, src/worker_pool/worker.rs</files>
  <read_first>src/python/handler.rs, src/worker_pool/worker.rs</read_first>
  <action>
    In `src/python/handler.rs`, modify the `invoke()` and `invoke_batch()` methods to extract trace context from `OwnedMessage.headers` before calling `inject_trace_context`.

    Currently `invoke()` does:
    ```rust
    let mut trace_context = std::collections::HashMap::new();
    inject_trace_context(&mut trace_context);
    ```
    This should be changed to pass the message headers so `inject_trace_context` has the actual header data:

    ```rust
    // Build header map from message headers for trace context injection
    let header_map: std::collections::HashMap<String, String> = message
        .headers
        .iter()
        .filter_map(|(k, v)| {
            v.as_ref().map(|bytes| {
                String::from_utf8_lossy(bytes).to_string()
            }).map(|val| (k.clone(), val))
        })
        .collect();
    let mut trace_context = std::collections::HashMap::new();
    inject_trace_context(&header_map, &mut trace_context);
    ```

    Update `ctx_to_pydict()` to receive trace context fields:
    ```rust
    fn ctx_to_pydict<'py>(
        py: Python<'py>,
        ctx: &ExecutionContext,
        msg: &OwnedMessage,
        trace_context: &std::collections::HashMap<String, String>,
    ) -> Py<PyAny> {
        let py_ctx = PyDict::new(py);
        let _ = py_ctx.set_item("topic", &ctx.topic);
        let _ = py_ctx.set_item("partition", ctx.partition);
        let _ = py_ctx.set_item("offset", ctx.offset);
        // trace fields
        if let Some(ref tid) = ctx.trace_id {
            let _ = py_ctx.set_item("trace_id", tid);
        }
        if let Some(ref sid) = ctx.span_id {
            let _ = py_ctx.set_item("span_id", sid);
        }
        ...
    }
    ```

    Then modify each invoke site to pass `&trace_context` to `ctx_to_pydict`.
  </action>
  <verify>
    <automated>cargo build --lib 2>&1 | head -30</automated>
  </verify>
  <done>
    invoke() and invoke_batch() extract trace context from message headers, pass to ctx_to_pydict, ExecutionContext has trace fields
  </done>
</task>

<task type="auto">
  <name>Task 4: Create src/worker_pool/concurrency.rs with HandlerConcurrency</name>
  <files>src/worker_pool/concurrency.rs</files>
  <read_first>src/worker_pool/pool.rs, src/worker_pool/worker.rs, src/worker_pool/mod.rs</read_first>
  <action>
    Create `src/worker_pool/concurrency.rs` with `HandlerConcurrency` struct using `Arc<Semaphore>` per handler:

    ```rust
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::Semaphore;

    /// Per-handler concurrency control via Arc<Semaphore>.
    ///
    /// Limits how many concurrent executions of the same handler can run simultaneously.
    /// Each handler gets its own semaphore with a configurable limit.
    /// The default limit applies to handlers that don't have an explicit concurrency setting.
    #[derive(Debug, Clone)]
    pub struct HandlerConcurrency {
        semaphores: Arc<parking_lot::Mutex<HashMap<String, Arc<Semaphore>>>>,
        default_limit: usize,
    }

    impl HandlerConcurrency {
        /// Create a new HandlerConcurrency with the given default limit.
        pub fn new(default_limit: usize) -> Self {
            Self {
                semaphores: Arc::new(parking_lot::Mutex::new(HashMap::new())),
                default_limit,
            }
        }

        /// Set the default limit for handlers without an explicit concurrency setting.
        pub fn with_default_limit(mut self, limit: usize) -> Self {
            self.default_limit = limit;
            self
        }

        /// Acquire a permit for the given handler_id.
        /// If no semaphore exists for this handler, creates one with the default limit.
        /// Returns an OwnedSemaphorePermit that is dropped to release the permit.
        pub async fn acquire(&self, handler_id: &str) -> tokio::sync::OwnedSemaphorePermit<'_> {
            let sem = {
                let mut guard = self.semaphores.lock();
                guard
                    .entry(handler_id.to_string())
                    .or_insert_with(|| Arc::new(Semaphore::new(self.default_limit)))
                    .clone()
            };
            sem.acquire().await.expect("semaphore closed")
        }

        /// Get the current number of available permits for a handler.
        /// Returns None if the handler is not registered.
        pub fn available(&self, handler_id: &str) -> Option<usize> {
            let guard = self.semaphores.lock();
            guard.get(handler_id).map(|s| s.available_permits())
        }
    }
    ```

    Add to `src/worker_pool/mod.rs`:
    ```rust
    pub mod concurrency;
    pub use concurrency::HandlerConcurrency;
    ```
  </action>
  <verify>
    <automated>cargo build --lib 2>&1 | head -30</automated>
  </verify>
  <done>
    HandlerConcurrency struct exists in src/worker_pool/concurrency.rs with Arc<Semaphore> per handler and acquire() method
  </done>
</task>

<task type="auto">
  <name>Task 5: Add concurrency field to HandlerMetadata and wire into add_handler</name>
  <files>src/pyconsumer.rs</files>
  <read_first>src/pyconsumer.rs</read_first>
  <action>
    Add `concurrency: Option<usize>` to `HandlerMetadata` in `src/pyconsumer.rs`:

    ```rust
    #[derive(Debug, Clone)]
    pub struct HandlerMetadata {
        pub callback: Arc<Py<PyAny>>,
        pub mode: HandlerMode,
        pub batch_max_size: Option<usize>,
        pub batch_max_wait_ms: Option<u64>,
        pub timeout_ms: Option<u64>,
        /// Maximum concurrent executions for this handler. None = use default.
        pub concurrency: Option<usize>,
    }
    ```

    Update `add_handler` signature to accept `concurrency`:
    ```rust
    #[pyo3(signature = (topic, callback, mode=None, batch_max_size=None, batch_max_wait_ms=None, timeout_ms=None, concurrency=None))]
    pub fn add_handler(
        &mut self,
        topic: String,
        callback: Bound<'_, PyAny>,
        mode: Option<String>,
        batch_max_size: Option<usize>,
        batch_max_wait_ms: Option<u64>,
        timeout_ms: Option<u64>,
        concurrency: Option<usize>,
    ) {
        ...
        let meta = HandlerMetadata {
            callback: Arc::new(callback.unbind()),
            mode,
            batch_max_size,
            batch_max_wait_ms,
            timeout_ms,
            concurrency,
        };
        ...
    }
    ```
  </action>
  <verify>
    <automated>cargo build --lib 2>&1 | head -30</automated>
  </verify>
  <done>
    HandlerMetadata has concurrency field, add_handler accepts and stores concurrency parameter
  </done>
</task>

<task type="auto">
  <name>Task 6: Integrate HandlerConcurrency into WorkerPool and worker loop</name>
  <files>src/worker_pool/pool.rs, src/worker_pool/worker.rs</files>
  <read_first>src/worker_pool/pool.rs, src/worker_pool/worker.rs</read_first>
  <action>
    In `src/worker_pool/pool.rs`, add `HandlerConcurrency` to `WorkerPool`:

    ```rust
    pub struct WorkerPool {
        workers: Vec<tokio::task::JoinHandle<()>>,
        queue_manager: Arc<QueueManager>,
        dlq_producer: Arc<SharedDlqProducer>,
        failure_classifier: Arc<dyn FailureClassifier>,
        handler_concurrency: HandlerConcurrency,
        ...
    }
    ```

    Add `handler_concurrency: HandlerConcurrency` to `WorkerPool::new()` parameters. Store it in the struct.

    In `src/worker_pool/worker.rs`, modify `worker_loop()` to accept `handler_concurrency: HandlerConcurrency` and `handler_id: String` (the topic), then acquire a permit before calling `handler.invoke_mode_with_timeout()`:

    ```rust
    pub async fn worker_loop(
        receiver: Arc<Mutex<Receiver<(OwnedMessage, HandlerId, ExecutionContext)>>>,
        handler: Arc<PythonHandler>,
        handler_concurrency: HandlerConcurrency,
        handler_id: String,
        ...
    ) {
        loop {
            let msg_opt = {
                let guard = receiver.lock().await;
                guard.recv().await
            };
            let (message, handler_id, ctx) = match msg_opt {
                Some((msg, hid, ctxt)) => (msg, hid, ctxt),
                None => break,
            };

            // Acquire concurrency permit — holds until end of loop iteration
            let _permit = handler_concurrency.acquire(&handler_id).await;

            // Now invoke handler (semaphore enforces max concurrency)
            let result = handler.invoke_mode_with_timeout(&ctx, message).await;
            ...
        }
    }
    ```

    In `WorkerPool::run()`, pass `handler_concurrency.clone()` and the handler's topic as `handler_id` to each `worker_loop` call.
  </action>
  <verify>
    <automated>cargo build --lib 2>&1 | head -30</automated>
  </verify>
  <done>
    WorkerPool stores HandlerConcurrency, worker_loop acquires permit before handler invocation, semaphore limits concurrent executions
  </done>
</task>

<task type="auto">
  <name>Task 7: Add concurrency parameter to Python handler() decorator in kafpy/runtime.py</name>
  <files>kafpy/runtime.py</files>
  <read_first>kafpy/runtime.py, kafpy/consumer.py, kafpy/handlers.py</read_first>
  <action>
    Add `concurrency` parameter to `handler()` decorator in `kafpy/runtime.py`:

    ```python
    def handler(
        self,
        topic: str,
        *,
        routing: object | None = None,
        timeout_ms: int | None = None,
        concurrency: int | None = None,
    ) -> Callable[[Callable], Callable]:
        """Decorator to register a handler for a topic.

        Args:
            topic: The Kafka topic to handle.
            routing: Optional routing configuration.
            timeout_ms: Per-handler execution timeout in milliseconds.
            concurrency: Maximum concurrent executions of this handler.
                None means no limit (default).
        """
        def decorator(fn: Callable) -> Callable:
            self.register_handler(
                topic, fn,
                routing=routing,
                timeout_ms=timeout_ms,
                concurrency=concurrency,
            )
            return fn
        return decorator
    ```

    Update `register_handler()` in `kafpy/runtime.py` to accept and pass `concurrency`:
    ```python
    def register_handler(
        self,
        topic: str,
        handler_fn: Callable,
        *,
        routing: object | None = None,
        handler_mode: str | None = None,
        batch_max_size: int | None = None,
        batch_max_wait_ms: int | None = None,
        timeout_ms: int | None = None,
        concurrency: int | None = None,
    ) -> None:
        ...
        # Pass concurrency to Rust consumer
        self._consumer.add_handler(
            topic, wrapper,
            mode=handler_type if handler_type != "sync" else None,
            batch_max_size=batch_max_size,
            batch_max_wait_ms=batch_max_wait_ms,
            timeout_ms=timeout_ms,
            concurrency=concurrency,
        )
    ```

    Also update `Consumer.add_handler()` in `kafpy/consumer.py` to accept and forward `concurrency`:
    ```python
    def add_handler(
        self,
        topic: str,
        handler: Callable[[object], None],
        *,
        mode: str | None = None,
        batch_max_size: int | None = None,
        batch_max_wait_ms: int | None = None,
        timeout_ms: int | None = None,
        concurrency: int | None = None,
    ) -> None:
        self._consumer.add_handler(
            topic, handler, mode, batch_max_size,
            batch_max_wait_ms, timeout_ms, concurrency,
        )
    ```
  </action>
  <verify>
    <automated>python -c "import kafpy; print('import ok')" 2>&1</automated>
  </verify>
  <done>
    handler() decorator accepts concurrency parameter, passes to add_handler, passes to Rust Consumer
  </done>
</task>

<task type="auto">
  <name>Task 8: Update kafpy/_kafpy.pyi type stubs for concurrency parameter</name>
  <files>kafpy/_kafpy.pyi</files>
  <read_first>kafpy/_kafpy.pyi, kafpy/consumer.py</read_first>
  <action>
    Read `kafpy/_kafpy.pyi` and update type stubs:

    1. Add `concurrency: int | None` parameter to `Consumer.add_handler()` signature
    2. Add `concurrency: int | None` parameter to `KafPy.handler()` and `KafPy.register_handler()` signatures

    The type stubs should match the Python implementation exactly.
  </action>
  <verify>
    <automated>python -m mypy kafpy --ignore-missing-imports 2>&1 | head -20</automated>
  </verify>
  <done>
    Type stubs updated with concurrency parameter on all relevant signatures
  </done>
</task>

</tasks>

### Must-Haves

 truths:
   - "inject_trace_context() parses traceparent header format 00-{trace_id:32}-{span_id:16}-{flags:2}"
   - "ExecutionContext carries trace_id and span_id from parsed traceparent"
   - "Arc<Semaphore> per handler key acquired before invoke_mode_with_timeout()"
   - "handler() decorator passes concurrency=N to add_handler() which stores in HandlerMetadata"
 artifacts:
   - path: "src/worker_pool/concurrency.rs"
     provides: "HandlerConcurrency with Arc<Semaphore> per handler key and acquire() method"
     min_lines: 50
   - path: "src/observability/tracing.rs"
     provides: "inject_trace_context() that parses W3C traceparent header"
     min_lines: 30
   - path: "src/python/context.rs"
     provides: "ExecutionContext with trace_id, span_id, trace_flags fields"
     min_lines: 25
 key_links:
   - from: "src/python/handler.rs"
     to: "src/observability/tracing.rs"
     via: "inject_trace_context(&header_map, &mut trace_context) called before spawn_blocking"
     pattern: "inject_trace_context"
   - from: "src/worker_pool/worker.rs"
     to: "src/worker_pool/concurrency.rs"
     via: "handler_concurrency.acquire(&handler_id).await before invoke_mode_with_timeout"
     pattern: "acquire.*handler_id"
   - from: "kafpy/runtime.py"
     to: "src/pyconsumer.rs"
     via: "add_handler(topic, callback, ..., concurrency=concurrency)"
     pattern: "concurrency"

### Verification

```bash
cargo build --lib
cargo test --lib -- concurrency inject trace_context ExecutionContext
python -c "from kafpy import Consumer, KafPy; print('import ok')"
```

### Success Criteria

- [ ] `grep -n "trace_id" src/python/context.rs` shows trace_id field in ExecutionContext
- [ ] `grep -n "inject_trace_context" src/observability/tracing.rs` shows real W3C parsing (not no-op)
- [ ] `grep -n "Arc<Semaphore>" src/worker_pool/concurrency.rs` shows HandlerConcurrency implementation
- [ ] `grep -n "concurrency" kafpy/runtime.py` shows concurrency parameter in handler() decorator
- [ ] `grep -n "concurrency" src/pyconsumer.rs` shows HandlerMetadata.concurrency field and add_handler parameter
- [ ] `cargo test --lib -- concurrency` passes
- [ ] Python `from kafpy import Consumer, KafPy` imports without error

### Output

After completion, create `.planning/phases/03-python-handler-api/03.1-SUMMARY.md`

---

## Wave 2 — context manager + decorator enhancement

### Frontmatter

```yaml
phase: "03"
slug: python-handler-api
plan: "02"
type: execute
wave: 2
depends_on:
  - "03.1"
files_modified:
  - src/pyconsumer.rs
  - kafpy/consumer.py
autonomous: true
requirements:
  - CONF-05
  - PY-01
```

### Objective

Add context manager support (`with Consumer(config) as c:`) so that exiting the `with` block triggers graceful shutdown. Also verify the `@handler` decorator works correctly with the newly added `concurrency` parameter.

### Context

@src/pyconsumer.rs
@kafpy/consumer.py
@kafpy/runtime.py

### Tasks

<task type="auto">
  <name>Task 1: Add __enter__/__exit__ to PyConsumer in src/pyconsumer.rs</name>
  <files>src/pyconsumer.rs</files>
  <read_first>src/pyconsumer.rs</read_first>
  <action>
    Add context manager methods to `PyConsumer` in `src/pyconsumer.rs`:

    ```rust
    #[pymethods]
    impl PyConsumer {
        /// Enter the context manager — returns self.
        fn __enter__(mut self) -> PyResult<Py<PyAny>> {
            Ok(self.into_py_any())
        }

        /// Exit the context manager — calls stop() to trigger graceful shutdown.
        fn __exit__(
            &mut self,
            exc_type: Option<&PyAny>,
            exc_val: Option<&PyAny>,
            traceback: Option<&PyAny>,
        ) -> PyResult<bool> {
            // Log exception info if an error occurred inside the with block
            if exc_type.is_some() {
                tracing::info!(
                    "Consumer context exit with exception, initiating graceful shutdown"
                );
            }
            self.stop();
            Ok(false) // don't suppress exceptions
        }
    }
    ```

    Add a helper method:
    ```rust
    fn into_py_any(&self) -> Py<PyAny> {
        // Convert &PyConsumer to Py<PyAny> for return from __enter__
        // Use Python::into_py(self) pattern
        todo!() // see below
    }
    ```

    Actually, the correct pattern in PyO3 for `__enter__` that returns `self` is:
    ```rust
    fn __enter__(slf: PyRefMut<'_, Self>) -> PyResult<PyRefMut<'_, Self>> {
        Ok(slf)
    }
    ```

    And for `__exit__`:
    ```rust
    fn __exit__(&mut self, _exc_type: Option<&PyAny>, _exc_val: Option<&PyAny>, _traceback: Option<&PyAny>) -> PyResult<bool> {
        self.stop();
        Ok(false)
    }
    ```

    Update the `#[pymethods]` impl to include both `__enter__` and `__exit__`.
  </action>
  <verify>
    <automated>cargo build --lib 2>&1 | head -30</automated>
  </verify>
  <done>
    PyConsumer implements __enter__ returning self and __exit__ calling stop()
  </done>
</task>

<task type="auto">
  <name>Task 2: Add __enter__/__exit__ to Python Consumer class in kafpy/consumer.py</name>
  <files>kafpy/consumer.py</files>
  <read_first>kafpy/consumer.py</read_first>
  <action>
    Add context manager support to the Python `Consumer` class in `kafpy/consumer.py`:

    ```python
    def __enter__(self) -> "Consumer":
        """Enter the context manager."""
        return self

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc_val: BaseException | None,
        traceback: Any,
    ) -> bool:
        """Exit the context manager — stops the consumer gracefully.

        Args:
            exc_type: Exception type if an error occurred inside the with block.
            exc_val: Exception value if an error occurred.
            traceback: Traceback object if an error occurred.

        Returns:
            False — exceptions are NOT suppressed and propagate normally.
        """
        if exc_type is not None:
            import logging
            logging.getLogger("kafpy").info(
                f"Consumer context exit with exception {exc_type.__name__}, initiating graceful shutdown"
            )
        self.stop()
        return False  # don't suppress exceptions
    ```
  </action>
  <verify>
    <automated>python -c "from kafpy import Consumer; c = Consumer; print(hasattr(c, '__enter__'), hasattr(c, '__exit__'))"</automated>
  </verify>
  <done>
    Consumer class implements __enter__ returning self and __exit__ calling stop()
  </done>
</task>

<task type="auto">
  <name>Task 3: Write tests for context manager and @handler decorator with concurrency</name>
  <files>kafpy/test_handlers.py</files>
  <read_first>kafpy/consumer.py, kafpy/runtime.py</read_first>
  <action>
    Create `kafpy/test_handlers.py` (or add to existing test file) to test:

    1. Context manager — `with Consumer(config) as c:` calls stop() on exit:
       ```python
       import unittest
       from kafpy import Consumer, ConsumerConfig

       class TestContextManager(unittest.TestCase):
           def test_consumer_context_manager_calls_stop_on_exit(self):
               config = ConsumerConfig(
                   bootstrap_servers="localhost:9092",
                   group_id="test-group",
                   topics=["test-topic"],
               )
               consumer = Consumer(config)
               # Verify stop was called (mock or track call)
               # Using context manager should call stop on exit
               with consumer as c:
                   pass  # no exception
               # Consumer.stop() should have been called
           ```

    2. @handler decorator with concurrency parameter:
       ```python
       def test_handler_decorator_accepts_concurrency(self):
           app = KafPy(consumer)
           @app.handler(topic="test-topic", concurrency=10)
           def handle(msg, ctx):
               return HandlerResult(action="ack")
           # Verify concurrency was stored and passed to Rust
       ```

    3. Verify concurrency=5 actually limits to 5 concurrent executions:
       (This is an integration test — can use a mock or real handler with counters)

    Run the tests:
    ```bash
    python -m pytest kafpy/test_handlers.py -v
    ```
  </action>
  <verify>
    <automated>python -m pytest kafpy/test_handlers.py -v 2>&1 | tail -30</automated>
  </verify>
  <done>
    Tests for context manager (stop called on exit) and @handler with concurrency parameter pass
  </done>
</task>

<task type="auto">
  <name>Task 4: Final verification — compile, test, type-check all Phase 3 files</name>
  <files>src/observability/tracing.rs, src/python/context.rs, src/worker_pool/concurrency.rs, src/pyconsumer.rs, kafpy/consumer.py, kafpy/runtime.py</files>
  <read_first>src/observability/tracing.rs, src/python/context.rs, src/worker_pool/concurrency.rs, src/pyconsumer.rs</read_first>
  <action>
    Run the full verification suite:

    ```bash
    # Rust compilation
    cargo build --lib 2>&1 | tail -10

    # Clippy
    cargo clippy --lib -- -D warnings 2>&1 | tail -20

    # Format
    cargo fmt -- --check 2>&1

    # Unit tests
    cargo test --lib 2>&1 | tail -30

    # Python import
    python -c "from kafpy import Consumer, KafPy; print('import ok')"

    # Python type check
    python -m mypy kafpy --ignore-missing-imports 2>&1 | tail -20
    ```

    Fix any failures.
  </action>
  <verify>
    <automated>cargo build --lib && cargo test --lib 2>&1 | tail -30</automated>
  </verify>
  <done>
    All Phase 3 files compile, tests pass, type-check clean
  </done>
</task>

</tasks>

### Must-Haves

 truths:
   - "with Consumer(config) as c: ... triggers stop() on exiting the with block"
   - "@handler(topic='x', concurrency=5) stores concurrency=5 in HandlerMetadata"
   - "PyConsumer.__enter__ returns self; __exit__ calls stop()"
 artifacts:
   - path: "src/pyconsumer.rs"
     provides: "PyConsumer.__enter__ and __exit__ methods"
     min_lines: 15
   - path: "kafpy/consumer.py"
     provides: "Consumer.__enter__ and __exit__ methods"
     min_lines: 15
 key_links:
   - from: "kafpy/consumer.py"
     to: "src/pyconsumer.rs"
     via: "Consumer.__exit__ calls self._consumer.stop()"
     pattern: "__exit__"
   - from: "src/pyconsumer.rs"
     to: "src/shutdown/shutdown.rs"
     via: "__exit__ calls self.stop() which calls shutdown_token.cancel()"
     pattern: "stop"

### Verification

```bash
cargo build --lib
cargo test --lib
python -c "from kafpy import Consumer; c = Consumer.__new__(Consumer); print(hasattr(c, '__enter__'), hasattr(c, '__exit__'))"
```

### Success Criteria

- [ ] `grep -n "__enter__\|__exit__" src/pyconsumer.rs` shows both methods on PyConsumer
- [ ] `grep -n "__enter__\|__exit__" kafpy/consumer.py` shows both methods on Consumer class
- [ ] `grep -n "concurrency" kafpy/runtime.py` shows concurrency parameter in handler() decorator
- [ ] `python -m pytest kafpy/test_handlers.py` runs and passes (if file created)
- [ ] `cargo test --lib -- python context` passes
- [ ] `cargo build --lib` succeeds with no errors

### Output

After completion, create `.planning/phases/03-python-handler-api/03.2-SUMMARY.md`

---

## Cross-Wave Notes

### Dependency Between Waves

Wave 2 (context manager) depends on Wave 1 completing first because `HandlerMetadata` needs the `concurrency` field added in Wave 1 before the type stubs and decorator can use it.

### Files Modified Summary

| File | Wave | Changes |
|------|------|---------|
| `src/observability/tracing.rs` | 1 | Implement real `inject_trace_context()` W3C parser |
| `src/python/context.rs` | 1 | Add `trace_id`, `span_id`, `trace_flags` to ExecutionContext |
| `src/worker_pool/concurrency.rs` | 1 | New file — `HandlerConcurrency` with `Arc<Semaphore>` per handler |
| `src/worker_pool/worker.rs` | 1 | Acquire semaphore permit before `invoke_mode_with_timeout()` |
| `src/worker_pool/pool.rs` | 1 | Add `HandlerConcurrency` to WorkerPool struct |
| `src/pyconsumer.rs` | 1, 2 | Add `concurrency` to HandlerMetadata + add_handler; add `__enter__`/`__exit__` |
| `kafpy/runtime.py` | 1 | Add `concurrency` param to `handler()` and `register_handler()` |
| `kafpy/consumer.py` | 1, 2 | Add `concurrency` to `add_handler()`; add `__enter__`/`__exit__` |
| `kafpy/_kafpy.pyi` | 1 | Update type stubs with `concurrency` |
| `kafpy/test_handlers.py` | 2 | Tests for context manager and @handler with concurrency |

### Assumptions Log

| # | Claim | Risk if Wrong |
|---|-------|---------------|
| A1 | `OwnedMessage.headers` is `Vec<(String, Option<Vec<u8>>)>` — can iterate to build HashMap for trace context | Need to check message.rs header type — confirmed in research |
| A2 | `spawn_blocking` already wraps the GIL boundary — no change needed there, only in `inject_trace_context()` | Confirmed in handler.rs lines 298-305 |
| A3 | `Arc<Py<PyAny>>` for callback is Send+Sync — no GIL dependency for the semaphore | Confirmed in research |
| A4 | Python `Consumer.stop()` is safe to call twice (idempotent) | Yes — CancellationToken is cancel-once semantics |