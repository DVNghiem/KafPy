# Stack Research

**Domain:** PyO3 Native Extension - Kafka consumer with async Python handler execution
**Researched:** 2026-04-18
**Confidence:** HIGH

## Recommended Stack

### Core Technologies

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `pyo3` | `0.27.2` | Python/Rust bindings | Already in use. Stable GIL API with `Bound<PyAny>` and `Python::with_gil`. |
| `pyo3-async-runtimes` | `0.27.0` | Tokio + Python asyncio bridge | Already in use. Provides `into_future` for calling Python coroutines from Rust. GIL released during await. |
| `tokio` | `1.40` | Async runtime | Already in use. Native rdkafka compat, mpsc channels, `spawn_blocking`. |

### Supporting Libraries

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `inspect` module (stdlib) | - | Detect coroutine functions | Check if a Python callback is `async def` before deciding sync vs async invoke path |
| `asyncio` module (stdlib) | - | Python asyncio event loop | Only if user-level `asyncio.run()` is needed; not required for bridge |
| `tokio::task::spawn_blocking` | - | GIL-bound sync calls | Sync Python handlers (current `PythonHandler::invoke`) |
| `tokio::task::JoinSet` | - | Multi-worker management | Already used in `WorkerPool`. Unchanged for async. |

### Development Tools

| Tool | Purpose | Notes |
|------|---------|-------|
| `maturin` | Build PyO3 extensions | Already in use |
| `cargo clippy` | Lint async code | Check for `Send` future issues |
| `pytest` | Python-side async testing | Verify async handlers work from Python |

---

## What IS Needed for v1.6

### 1. `pyo3_async_runtimes::tokio::into_future` (already available)

**Purpose:** Convert a Python awaitable/coroutine into a Rust `Future<Output = PyResult<Py<PyAny>>> + Send` that can be awaited on Tokio.

```rust
// Signature (from docs.rs 0.27.0):
pub fn into_future(
    awaitable: Bound<'_, PyAny>,
) -> PyResult<impl Future<Output = PyResult<Py<PyAny>>> + Send>
```

**Usage pattern for async Python handlers:**
```rust
// Inside PythonHandler::invoke_async (new method):
let py_future = Python::with_gil(|py| {
    let coroutine = callback.call1(py, (py_msg,))?;  // Returns coroutine/awaitable
    // Verify it is awaitable (check hasattr "__await__")
    pyo3_async_runtimes::tokio::into_future(coroutine)
})?;
let result = py_future.await;  // GIL released during await
```

**Key properties:**
- GIL is released during await (`run_coroutine_threadsafe` internally)
- Returns `Send + 'static` future (can move between Tokio tasks)
- Output is `PyResult<Py<PyAny>>` (owned Python result, no GIL lifetime)

### 2. Coroutine Detection (Python stdlib `inspect`)

**Purpose:** Determine at registration or invoke time whether a callback is `async def`.

```rust
Python::with_gil(|py| {
    let is_coro = callback
        .getattr(py, pyo3::intern!(py, "__code__"))
        .and_then(|code| {
            let is_coro: bool = code.getattr(py, "co_flags")?
                .getattr(py, "CO_COROUTINE")?;
            Ok(is_coro)
        })
        .unwrap_or(false);
    // OR simpler: inspect.iscoroutinefunction(callback)
});
```

Alternative: Use `inspect.iscoroutinefunction` from Python side at registration time.

### 3. Async Variant of `PythonHandler::invoke`

**Architecture:**
- Add `invoke_async(&self, ctx, message)` method alongside existing `invoke`
- Both return `ExecutionResult` (normalized, same as sync path)
- `spawn_blocking` path: existing `invoke` (sync callables)
- `into_future` path: new `invoke_async` (async callables)

```rust
pub async fn invoke_async(
    &self,
    ctx: &ExecutionContext,
    message: OwnedMessage,
) -> ExecutionResult {
    let callback = Arc::clone(&self.callback);
    // ... build py_msg dict ...

    let result = Python::with_gil(|py| {
        let coroutine = callback.call1(py, (py_msg,))?;
        pyo3_async_runtimes::tokio::into_future(coroutine)
    })?;

    result.await.unwrap_or_else(|_| ExecutionResult::Error {
        reason: FailureReason::Terminal(TerminalKind::HandlerPanic),
        exception: "Panic".to_string(),
        traceback: "async handler panicked".to_string(),
    })
}
```

### 4. Handler Mode Abstraction

**Four execution modes (per PROJECT.md EXEC-05):**

| Mode | Invocation | When to Use |
|------|-----------|-------------|
| `SingleSync` | `spawn_blocking` | `async def` + `await` in Python handler (legacy) |
| `SingleAsync` | `into_future` | `async def` handler, awaited on Tokio |
| `BatchSync` | `spawn_blocking` + batch accumulation | Multiple messages passed to sync callable |
| `BatchAsync` | `into_future` + batch accumulation | Multiple messages passed to async callable |

---

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| `async-std` runtime | Tokio already in use, would add second runtime | `tokio` only |
| `pyo3-asyncio` crate | Deprecated, merged into `pyo3-async-runtimes` | `pyo3-async-runtimes` |
| `futures::executor::block_on` | Blocking executor, not for PyO3 integration | `tokio::task::spawn_blocking` |
| Manual Python event loop management | `pyo3-async-runtimes` handles loop attach/detach | `into_future` / `future_into_py` |
| `asyncio.run()` in Python handler | Re-entrant event loop issues | Let Rust Tokio drive the Python coroutine via `into_future` |
| `spawn_blocking` for async handlers | Blocks a Tokio thread unnecessarily | `into_future` for non-blocking async |

---

## Version Compatibility

| Package | Version | Compatible With | Notes |
|---------|---------|-----------------|-------|
| `pyo3` | `0.27.2` | Python 3.11-3.14 | Already in use |
| `pyo3-async-runtimes` | `0.27.0` | `pyo3 >= 0.27`, Python 3.8+ | GIL released during await (v0.27.0 changelog) |
| `tokio` | `1.40` | All above | Already in use |
| Python stdlib `asyncio` | 3.11+ | Works with `into_future` | Improved in 3.11 (task groups, performance) |

**Compatibility Notes from pyo3-async-runtimes 0.27.0 changelog:**
- Futures passed to `future_into_py` must now implement `Send` (due to GIL release during finalization)
- `Runtime` trait now requires `spawn_blocking` function
- Minimum pyo3 version: 0.27

---

## Stack Patterns by Variant

**If sync Python handler (regular callable):**
- Use existing `PythonHandler::invoke` with `spawn_blocking`
- No changes to `into_future` code path needed

**If async Python handler (`async def` callable):**
- Use `pyo3_async_runtimes::tokio::into_future`
- GIL released during await - Tokio thread is NOT blocked
- Same `ExecutionResult` normalization on return

**If batch sync handler:**
- Accumulate messages in Rust buffer until `max_batch_size` OR `max_batch_wait_ms`
- Pass `Vec<OwnedMessage>` to handler via `spawn_blocking`
- Handler returns batch result (all ok / first failure / per-message results)

**If batch async handler:**
- Accumulate messages in Rust buffer until size OR timeout
- Pass `Vec<OwnedMessage>` to async handler via `into_future`
- Await the coroutine on Tokio (non-blocking on Rust side)

---

## Key Architectural Insight

**GIL management for async vs sync:**

| Handler Type | Rust Invocation | GIL Behavior | Tokio Thread State |
|-------------|-----------------|--------------|-------------------|
| Sync | `spawn_blocking` | GIL held only inside Python callable | Blocked during call (expected) |
| Async | `into_future` + `await` | GIL released during await | NOT blocked - can run other tasks |

**Both paths achieve "minimal GIL hold window" (EXEC-06):**
- Sync: GIL held only during actual Python execution in `spawn_blocking`
- Async: GIL released immediately after calling the coroutine, re-acquired only to get/set Python state during await points

---

## Sources

- [Context7: pyo3-async-runtimes](https://github.com/pyo3/pyo3-async-runtimes) — `into_future` / `future_into_py` API, GIL behavior, HIGH confidence
- [docs.rs: pyo3-async-runtimes 0.27.0](https://docs.rs/pyo3-async-runtimes/0.27.0/pyo3_async_runtimes/tokio/) — function signatures, HIGH confidence
- [GitHub CHANGELOG.md: pyo3-async-runtimes 0.27.0](https://github.com/pyo3/pyo3-async-runtimes/blob/main/CHANGELOG.md) — version compatibility, breaking changes, HIGH confidence
- [Cargo.toml: KafPy current dependencies](file:///home/nghiem/project/KafPy/Cargo.toml) — existing stack confirmed

---
*Stack research for: async Python handlers via pyo3-async-runtimes*
*Researched: 2026-04-18*
