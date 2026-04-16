# Pitfalls Research: Python Execution in Rust Kafka Framework

**Domain:** PyO3 async integration with Python callback execution
**Researched:** 2026-04-16
**Confidence:** MEDIUM — Based on PyO3 internals, Rust async patterns, and analysis of existing `pyconsumer.rs`

---

## Critical Pitfalls

### Pitfall 1: Raw Pointer Escape — Cloning `Py<PyAny>` Across Async Boundary

**What goes wrong:**
Converting a `Py<PyAny>` to a raw `*mut PyObject` pointer, shipping the pointer across the async boundary, then reconstructing via `from_owned_ptr` doubles the reference count. The original `Py<PyAny>` still owns its reference; the reconstructed one claims a second. When both drop, the Python object is double-freed.

**Why it happens:**
`Py<PyAny>` does not implement `Clone` in the usual sense — it does a reference count increment. But the raw pointer trick bypasses this: you extract the pointer (no ref count change), ship it, then reconstruct with `from_owned_ptr` which creates a new owned reference without the source object knowing. Two owned references to one Python object.

**How to avoid:**
Use `Py::clone(&handler)` when you need a clone across any boundary. In the async block:
```rust
// WRONG — raw pointer escape, double reference
let handler_ptr: *mut pyo3::ffi::PyObject = { /* ... */.as_ptr() };
// ... later in async block ...
let handler: Py<PyAny> = unsafe { Py::from_owned_ptr(py, handler_ptr) };

// CORRECT — explicit clone preserves ref count semantics
let handler = Py::clone(&handler); // increments reference count
handler.call1(py, (py_msg_clone,))?;
```

**Warning signs:**
- `from_owned_ptr` in any code path that did not receive the pointer via a owning function (e.g., `new`, `from_owned_ptr`, `from_borrowed_ptr`)
- `as_ptr()` called on a Python object that is not consumed
- `unsafe` block around PyO3 reconstruction that does not document the ref-count invariant

**Phase to address:** Phase 8 (ConsumerRunner Integration) — before wiring Python callbacks to the dispatcher worker pool

---

### Pitfall 2: GIL Hold Window Creep — Python Execution Blocking the Consumer Loop

**What goes wrong:**
If the Python callback is called directly in the async message loop (inside `ConsumerDispatcher::run`), the Tokio task holds the GIL for the entire Python execution duration. This blocks the consumer from polling Kafka during that time, causing message lag to accumulate invisibly.

**Why it happens:**
The consumer loop is `async fn run(&self)` and must poll `stream.next()` to stay responsive. If you call `handler.call1(py, (msg,))` directly in that loop, the Tokio task is still running — but the GIL is held, so no other Python code can execute. More critically: if the callback is slow (network call, disk I/O, complex computation), the Tokio executor thread is parked waiting, not doing useful work.

**How to avoid:**
Always route Python execution through `tokio::task::spawn_blocking`:
```rust
// CORRECT — release Tokio thread while waiting for GIL
let handler = Py::clone(&handler);
let py_msg_clone = py_msg.clone();
let result = tokio::task::spawn_blocking(move || {
    Python::with(|py| {
        handler.call1(py, (py_msg_clone,))
    })
}).await;
```
The Tokio thread is released back to the executor; Python runs on a blocking thread pool where GIL acquisition happens.

**Warning signs:**
- `Python::with` or `pyo3::Python` calls directly inside `ConsumerDispatcher::run`
- No `tokio::task::spawn_blocking` between `stream.next()` and Python callback invocation
- Consumer lag metrics increasing under load despite CPU being idle

**Phase to address:** Phase 8 — worker pool design must include `spawn_blocking` for Python calls

---

### Pitfall 3: Storing `Bound<'_, PyAny>` Instead of `Py<PyAny>`

**What goes wrong:**
`Bound<'_, PyAny>` carries a lifetime tied to a specific `Python` token. It cannot be stored and used later with a different `Python` instance. If you store `Bound<'_, PyAny>` in a `HashMap` and retrieve it in an async context, you get a lifetime mismatch error at best, or undefined behavior at worst.

**Why it happens:**
`Bound<'_, PyAny>` is a zero-cost wrapper over a borrowed reference with a `'lua` lifetime. It is designed for immediate use within a `Python` context. The moment you need to store a Python object beyond the current `Python` scope, you need `Py<PyAny>` (an owned reference with no lifetime).

**How to avoid:**
Use `callback.unbind()` to convert `Bound<'_, PyAny>` to `Py<PyAny>` at the boundary, then store `Py<PyAny>`:
```rust
// In pyconsumer.rs — correct pattern
handlers: Arc<Mutex<HashMap<String, Py<PyAny>>>>,

pub fn add_handler(&mut self, topic: String, callback: Bound<'_, PyAny>) {
    let mut handlers = self.handlers.lock().unwrap();
    handlers.insert(topic, callback.unbind()); // converts to owned reference
}
```

**Warning signs:**
- Compilation errors: "missing lifetime specifier" or "cannot return value referencing local data"
- `Bound<'_, PyAny>` appearing in struct fields, especially inside `Arc` or `Mutex`
- Lifetime errors that appear only in async contexts

**Phase to address:** Phase 6 (Dispatcher Core) — the `PyConsumer` struct must store `Py<PyAny>`, not `Bound`

---

### Pitfall 4: Fake Parallelism — Pretending Tokio Tasks Provide Python Parallelism

**What goes wrong:**
Spawning multiple `tokio::spawn` tasks that all call Python creates the appearance of parallelism, but Python's GIL forces serialization. Only one `Python::with` block can execute at a time across all threads. The result: more Tokio tasks, same single-threaded Python execution, plus scheduling overhead.

**Why it happens:**
Python's GIL is a global lock. `tokio::spawn` creates new Tokio tasks, but when those tasks reach `Python::with` or `spawn_blocking` to acquire the GIL, they queue behind each other. You get no throughput improvement from GIL-bound Python code.

**How to avoid:**
Acknowledge GIL constraints explicitly:
1. Use a `Semaphore` (Tokio-level) to limit concurrent Python executions to `N` where `N` is chosen based on the nature of the Python code (I/O-bound can tolerate more; CPU-bound should be 1)
2. Profile with `tokio-console` to see task wait times at the GIL
3. Consider Python `multiprocessing` or `concurrent.futures.ProcessPoolExecutor` for true parallelism of CPU-bound Python work

The dispatcher semaphore (DISP-15) already provides this mechanism — wire it to Python workers:
```rust
// Worker pool with semaphore matching Python's actual parallelism capacity
let semaphore = Arc::new(tokio::sync::Semaphore::new(max_python_concurrency));
let permit = semaphore.acquire().await?; // limits concurrent Python calls
```

**Warning signs:**
- Throughput does not improve when adding more consumer handler threads
- CPU usage on the Python process stays at ~100% with only one core despite multiple Tokio tasks
- `tokio::spawn` calls multiplying without throughput gains

**Phase to address:** Phase 8 — worker pool concurrency limits must be calibrated to GIL characteristics

---

### Pitfall 5: Python Exceptions Escaping Async Context

**What goes wrong:**
If a Python callback raises an exception and that exception propagates out of the `Python::with` block in an async context, it either causes a crash or silently drops the message. The async runtime does not know how to translate a Python exception into a Rust error that can be propagated cleanly.

**Why it happens:**
PyO3 exceptions are bound to a specific `Python` interpreter instance. When a Python exception escapes the `Python` context (e.g., into `?` or `Result` propagation that crosses an async boundary), the exception state is lost because the `Python` token is scoped to the `Python::with` block.

**How to avoid:**
Always handle Python exceptions within the `Python::with` scope:
```rust
let result = Python::with(|py| {
    handler.call1(py, (msg,))
});
match result {
    Ok(_) => { /* success */ }
    Err(e) => {
        // e is PyErr — convert to Rust error INSIDE Python context
        let msg = Python::with(|py| e.to_string());
        tracing::error!("Python callback failed: {}", msg);
        // Decide: skip message, retry, or halt
    }
}
```
Never use `?` to propagate a bare `PyErr` out of the `Python::with` block into an async function signature.

**Warning signs:**
- `PyErr` appearing in return types of async functions
- `map_err(|e| PyErr::new::<...>(...))` used to convert Python exceptions across async boundaries
- Silent message drops with no error log when Python callbacks fail

**Phase to address:** Phase 8 — error handling at the Python callback boundary must convert PyErr to Rust errors within the Python scope

---

### Pitfall 6: `CancellationToken` Not Propagated to Python Workers

**What goes wrong:**
When `stop()` is called on `PyConsumer`, the `CancellationToken` stops the consumer loop. But if Python callbacks are running in `spawn_blocking` tasks, those tasks keep running until the Python call completes. The consumer is stopped but Python workers continue until all in-flight messages are processed.

**Why it happens:**
`spawn_blocking` tasks are independent Tokio tasks. They hold their own future state and will run to completion (or cancellation via `shutdown` on the `JoinHandle`) independently of the `CancellationToken` used in the consumer loop.

**How to avoid:**
Track in-flight Python executions and join them on shutdown:
```rust
// Track active Python worker tasks
let active_workers = Arc::new(AtomicUsize::new(0));

// Increment on spawn, decrement on completion
active_workers.fetch_add(1, Ordering::Relaxed);
let worker = tokio::spawn(async move {
    let result = spawn_blocking(|| { /* Python call */ }).await;
    active_workers.fetch_sub(1, Ordering::Relaxed);
    result
});

// On shutdown: cancel token, then wait for active_workers to reach 0
```

**Warning signs:**
- `stop()` returns but Python callbacks are still running
- Messages continue to be processed after `stop()` is called
- `shutdown()` does not actually stop Python handlers

**Phase to address:** Phase 8 — shutdown coordination must include Python worker tasks

---

### Pitfall 7: `OwnedMessage` Not Actually Send+Sync When It Contains `Py<PyAny>`

**What goes wrong:**
`OwnedMessage` is designed to be `Send + Sync` so it can cross thread boundaries in the Tokio runtime. But if you embed `Py<PyAny>` inside `OwnedMessage` (to carry a callback alongside the message), `Py<PyAny>` is NOT `Send + Sync` because Python objects are not thread-safe by default.

**Why it happens:**
`Py<PyAny>` wraps a `PyObject*` and the GIL is not a thread-safety mechanism — Python objects are not guaranteed to be safe to share across threads without the GIL. PyO3 marks `Py<PyAny>` as not `Send` or not `Sync` accordingly.

**How to avoid:**
Keep `Py<PyAny>` in a separate structure from the message. The dispatcher sends `OwnedMessage` through pure-Rust channels; the Python callback lives in the worker task that pulls from the receiver:
```
ConsumerRunner --mpsc--> Dispatcher --mpsc--> WorkerPool --spawn_blocking--> Python
                               ^                      |
                               |                      |
                          No Py<PyAny>          Py<PyAny> stays
                          in this lane           in worker lane
```

The `mpsc::Receiver<OwnedMessage>` sits in the Python worker lane (registered via `register_handler`); `OwnedMessage` never carries Python references.

**Warning signs:**
- Compilation error: `OwnedMessage` does not implement `Send`
- `#[derive(Clone)]` on a struct that contains `Py<PyAny>`
- Thread-safety errors that appear only when the dispatcher routes to multiple workers

**Phase to address:** Phase 8 — the worker pool must receive `OwnedMessage` on a separate thread from the consumer loop, requiring `OwnedMessage` to remain `Send + Sync`

---

### Pitfall 8: Blocking the Tokio Thread with Synchronous Python API Calls

**What goes wrong:**
Using synchronous Python APIs (e.g., `callback.call1(...)` without `spawn_blocking`) inside an async Tokio task blocks that task's thread. If enough tasks block this way, the Tokio thread pool starves — no threads available to poll the Kafka consumer, causing a complete hang.

**Why it happens:**
`pyo3_async_runtimes::tokio::future_into_py` runs the async block on a Tokio runtime thread. If you call Python synchronously (without `spawn_blocking`), the Tokio thread is occupied for the entire Python call duration. With a limited thread pool (default: num_cpus), a few slow Python calls can exhaust the pool.

**How to avoid:**
The golden rule: **Every Python call from an async context must go through `spawn_blocking`**:
```rust
// CORRECT — Tokio thread released during Python execution
tokio::task::spawn_blocking(|| {
    Python::with(|py| {
        handler.call1(py, (msg,))
    })
}).await?;
```

**Warning signs:**
- Deadlock under load — consumer stops processing, no errors logged
- Thread pool exhaustion visible in `tokio-console`
- System appears to hang with CPU at 0% — threads are all blocked on Python calls

**Phase to address:** Phase 8 — worker pool implementation must use `spawn_blocking` for all Python calls

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Using `as_ptr()` / `from_owned_ptr` to pass `Py<PyAny>` | Avoids `.clone()` call | Double-free risk, undefined behavior, impossible to audit | Never |
| Calling Python directly in async loop without `spawn_blocking` | Simpler code, fewer tasks | Tokio thread pool exhaustion, consumer hang | Never in production |
| Storing `Bound<'_, PyAny>` in struct | No `.unbind()` call needed | Lifetime errors, undefined behavior across async boundaries | Never |
| Ignoring `PyErr` from Python calls | Fewer match branches | Silent failures, message loss, debugging nightmare | Never |
| No semaphore on Python worker pool | Full parallelism | GIL contention, no throughput gain, CPU waste | Only for very lightweight Python callbacks; document the risk |
| `CancellationToken` not propagated to workers | Simpler shutdown | Python workers run after consumer reports "stopped" | Never |

---

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| PyO3 async runtime (`future_into_py`) | Assuming GIL is held across the async boundary | `future_into_py` creates a Python-aware async context, but GIL is only held inside `Python::with` blocks within that async block |
| Worker pool | Treating workers as Tokio tasks that can be cancelled by dropping the `JoinHandle` | Workers must be explicitly tracked and joined on shutdown; dropping `JoinHandle` only aborts, it does not cooperate with graceful shutdown |
| Dispatcher queue | Assuming `try_send` failure means the message is dropped | `try_send` failure triggers backpressure policy — the message may be retried, paused, or dropped depending on policy |
| Python callback lifetime | Assuming Python callback lives as long as `PyConsumer` | Python objects can be garbage collected if no Rust reference is kept — use `Py<PyAny>` in `Arc<Mutex<...>>` to match Python object's lifetime to `PyConsumer`'s lifetime |

---

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| GIL contention under high concurrency | Python CPU ~100% on one core, throughput plateaus regardless of worker count | Semaphore limiting workers to GIL-serializable count; separate CPU-bound Python work into `ProcessPoolExecutor` | Always for CPU-bound Python callbacks; I/O-bound can tolerate more workers |
| Large message copy into Python worker | Memory pressure, GC pressure in Python | `OwnedMessage` has `key`, `payload` as `Option<Vec<u8>>` — zero-copy until it enters Python; only copy when Python needs the data | At high message rates with large payloads (>100KB) |
| Blocking the consumer loop with backpressure | `send()` returns `Backpressure` and caller continues loop — but if backpressure is persistent, consumer loop spins wasting CPU | `FuturePausePartition` action pauses the Kafka partition; ensure DISP-18 pause/resume wiring is complete | At sustained high load |
| Tokio thread pool exhaustion | Deadlock — no threads to poll Kafka | Always use `spawn_blocking` for Python; size Tokio thread pool appropriately ( Tokio defaults to num_cpus, but if all are occupied by Python `spawn_blocking` calls, nothing can progress) | At moderate load with slow Python callbacks (e.g., database calls) |

---

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| Python callback exceptions revealing internal paths | Stack trace leaks to logs | Always catch `PyErr` and log a sanitized message; never log `e.debug()` or `e.pprint()` in production |
| Arbitrary Python code execution via user-provided callbacks | Code injection if user provides callback that imports untrusted modules | Sandbox Python environment; do not allow callback to import arbitrary modules |
| Python callbacks holding references beyond message lifecycle | Memory leak — Python object holds reference to Rust-owned memory | Ensure callback is scoped to message processing; do not store cross-message state in Python |

---

## "Looks Done But Isn't" Checklist

- [ ] **Python boundary:** Messages are routed through dispatcher (pure Rust) and Python is called only via `spawn_blocking` worker tasks — verify no `Python::with` in `ConsumerDispatcher::run`
- [ ] **Py<PyAny> storage:** `PyConsumer` stores `Py<PyAny>` (not `Bound`), retrieved via `Py::clone(&handler)` when calling — verify no raw pointer reconstruction
- [ ] **GIL hold window:** Python callback execution happens in `spawn_blocking`, Tokio thread released during call — verify with `tokio-console` under load
- [ ] **Error handling:** Python exceptions are caught and converted to Rust errors INSIDE the `Python::with` block — no bare `PyErr` propagating out of `spawn_blocking`
- [ ] **Shutdown coordination:** `CancellationToken` cancellation triggers worker shutdown; all in-flight Python calls are joined before `stop()` returns
- [ ] **Executor trait:** The `BackpressurePolicy` trait handles all backpressure scenarios including `FuturePausePartition` — the consumer runner respects the policy signal
- [ ] **Message ownership:** `OwnedMessage` is `Send + Sync`; no `Py<PyAny>` embedded in it — verify with `static_assertions` at compile time

---

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Double-freed `Py<PyAny>` from raw pointer reconstruction | HIGH — memory corruption, non-deterministic crashes | Immediate: add `static_assertions` for `Py<PyAny>` ref count sanity; audit all `as_ptr()` / `from_owned_ptr` patterns; replace with `Py::clone()` |
| Tokio thread pool exhaustion from sync Python calls | HIGH — consumer hangs, requires restart | Immediate: add `spawn_blocking` wrapper around all Python calls; tune `spawn_blocking` thread pool size via Tokio config |
| GIL contention from over-parallel workers | MEDIUM — throughput plateau, not a crash | Reduce worker concurrency; profile with `tokio-console` to confirm GIL is the bottleneck; separate CPU-bound work |
| Python exceptions escaping async boundary | MEDIUM — crashes or silent message drops | Wrap all Python calls in `Python::with` with explicit error handling; add integration test that exercises callback that raises |

---

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| Raw pointer double-free | Phase 8 — WorkerPool wiring | `static_assertions` compile test: `Py<PyAny>` never reconstructed from raw pointer |
| GIL hold window creep | Phase 8 — WorkerPool | `tokio-console` observation: no Tokio threads blocked on Python calls |
| `Bound<'_, PyAny>` stored | Phase 6 — PyConsumer refactor | Compile error if `Bound` appears in struct fields |
| Fake parallelism | Phase 8 — Semaphore calibration | Load test: throughput must not degrade with worker count beyond GIL tolerance |
| Python exceptions escaping async | Phase 8 — Error boundary | Integration test: Python callback raises exception, error is logged, message is handled per policy |
| `CancellationToken` not propagated | Phase 8 — Shutdown coordination | Integration test: call `stop()`, verify all Python workers complete within grace period |
| `OwnedMessage` not `Send+Sync` | Phase 8 — Worker lane separation | `static_assertions` compile test: `OwnedMessage: Send + Sync` |
| Blocking Tokio thread | Phase 8 — `spawn_blocking` enforcement | Load test + `tokio-console`: no Tokio thread blocked > 1ms |

---

## Sources

- PyO3 documentation: Python boundary and GIL management (official)
- `pyo3-async-runtimes` source: how `future_into_py` manages the GIL
- Rust Tokio documentation: `spawn_blocking` contract and thread pool behavior
- Existing `pyconsumer.rs` analysis: raw pointer pattern identified at lines 91-106
- `dispatcher/mod.rs`: consumer-runner integration architecture confirms `OwnedMessage` must remain `Send + Sync`
- `consumer/runner.rs`: `ConsumerRunner::stream()` produces `OwnedMessage` — must not carry Python references

---
*Pitfalls research for: PyO3 Python callback execution in Rust async Kafka framework*
*Researched: 2026-04-16*