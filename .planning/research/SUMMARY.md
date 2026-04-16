# Project Research Summary

**Project:** KafPy v1.2 Python Execution Lane
**Domain:** PyO3 Rust Kafka client - Python callback invocation layer
**Researched:** 2026-04-16
**Confidence:** HIGH

## Executive Summary

The Python execution lane bridges Tokio async workers with Python callable invocation while minimizing GIL hold time. Research confirms the existing stack (PyO3 0.27.2, pyo3-async-runtimes 0.27.0, Tokio 1.40, parking_lot 0.12) is sufficient - no new crate dependencies are needed. The key architectural addition is a WorkerPool that pulls from mpsc channels and invokes Python via `spawn_blocking`, keeping GIL acquisition brief and Tokio threads released during Python execution.

The recommended build order is: `ExecutionResult` + `Executor` trait (no dependencies) -> `PythonHandler` (depends on trait) -> `WorkerPool` (depends on handler) -> Integration with `ConsumerDispatcher`. This minimizes circular dependencies and allows each layer to be tested in isolation. Critical pitfalls center on GIL management: never hold GIL across async boundaries, never reconstruct `Py<PyAny>` from raw pointers, and always convert `Bound<'_, PyAny>` to `Py<PyAny>` before storing.

## Key Findings

### Recommended Stack

The existing stack is complete. No new crates are required for v1.2 scope.

**Core technologies (all existing):**
- **PyO3 0.27.2** — Python-Rust FFI; `Py<PyAny>` for GIL-independent callback storage (cheap to clone, Send+Sync when copied correctly)
- **Tokio 1.40** — Async runtime; `spawn_blocking` for releasing Tokio threads during GIL acquisition
- **parking_lot 0.12** — Fast mutex without poison semantics; used for handler map access
- **pyo3-async-runtimes 0.27.0** — `future_into_py` for Python async boundary bridging

**Key pattern: Py<PyAny> storage:**
```rust
// Storage: Arc<parking_lot::Mutex<HashMap<String, Py<PyAny>>>>
// Clone via Py::clone(&handler) before entering async block
// Access: lock map -> get Py<PyAny> -> drop lock -> spawn_blocking -> Python call
```

**Key pattern: spawn_blocking GIL release:**
```rust
let handler = Py::clone(&handler);
let py_msg = py_msg.clone();
tokio::task::spawn_blocking(move || {
    Python::with_gil(|py| {
        handler.call1(py, (py_msg,))
    })
}).await
```

### Expected Features

**Must have (table stakes - MVP):**
- **WorkerPool with configurable N workers** — Tokio JoinSet managing concurrent workers pulling from mpsc::Receiver
- **Py<PyAny> callback storage** — GIL-independent, Send+Sync; not `&PyAny` or `Bound<'_, PyAny>`
- **spawn_blocking GIL acquisition** — Minimal window; GIL only held during actual Python call
- **ExecutionResult normalization** — `Ok`, `Error { exception, traceback }`, `Rejected { reason }` variants
- **DefaultExecutor (fire-and-forget)** — No retry, no commit; logs outcome and moves on
- **Worker lifecycle logging** — Start/stop/message pickup/success/failure visibility

**Should have (production readiness - v1.x):**
- **RetryExecutor** — Exponential backoff retry with configurable attempts and delay
- **CommitExecutor** — Batches offsets and commits after N messages or T timeout
- **Shutdown drain** — Graceful stop completing in-flight messages before worker exit

**Defer (v2+):**
- **AsyncExecutor** — Detects Python coroutines, runs on Python asyncio via pyo3-async-runtimes
- **BatchExecutor** — Accumulates N messages, invokes single Python callback with list
- **DLQ (Dead Letter Queue)** — Rejected messages routed to error topic after max retries

### Architecture Approach

The Python execution lane introduces 4 components: `ExecutionResult` (normalized outcome enum), `PythonHandler` (callback invoker with mpsc receiver), `WorkerPool` (concurrency manager via JoinSet), and `Executor` trait (pluggable retry/commit/ack policies). Data flows: `ConsumerRunner` -> `ConsumerDispatcher.send()` -> per-topic `mpsc::Receiver` -> `WorkerPool` workers -> `PythonHandler::process_one()` -> `spawn_blocking` -> Python callback -> `ExecutionResult` -> `Executor::execute()` -> `ExecutorOutcome` (Ack/Retry/Rejected).

**New module structure:**
```
src/python/
    mod.rs
    execution_result.rs   # ExecutionResult enum (no PyO3 deps)
    executor.rs           # Executor trait + ExecutorOutcome + DefaultExecutor
    handler.rs            # PythonHandler, spawn_blocking invocation
    worker_pool.rs        # WorkerPool with JoinSet
```

**Build order rationale:** `ExecutionResult` has zero dependencies on other new modules. `Executor` depends only on `ExecutionResult` and existing `OwnedMessage`. `PythonHandler` depends on the `Executor` trait. `WorkerPool` depends on `PythonHandler` and `Executor`. This creates a linear dependency chain with no cycles.

### Critical Pitfalls

1. **Raw pointer double-free** — Converting `Py<PyAny>` to `*mut PyObject`, shipping across async boundary, reconstructing via `from_owned_ptr` doubles the reference count. Always use `Py::clone(&handler)` instead of raw pointer tricks.

2. **GIL hold window creep** — Calling Python directly inside `ConsumerDispatcher::run` async loop holds the GIL for the entire Python execution duration, blocking the Tokio runtime. Always use `spawn_blocking` for Python calls.

3. **Storing `Bound<'_, PyAny>` instead of `Py<PyAny>`** — `Bound` carries a lifetime tied to a specific `Python` token. Cannot be stored across async boundaries. Use `callback.unbind()` at the PyO3 boundary to convert to owned `Py<PyAny>`.

4. **Fake parallelism from GIL serialization** — Multiple Tokio workers all competing for Python GIL produce no throughput improvement for CPU-bound callbacks. Use a `Semaphore` to limit concurrent Python executions to GIL-serializable count.

5. **Python exceptions escaping async context** — `PyErr` is bound to a specific `Python` token. Always catch and convert to Rust errors INSIDE the `Python::with` block; never propagate bare `PyErr` out of `spawn_blocking`.

## Implications for Roadmap

Based on research, a 2-phase structure is recommended: Foundation (types) + Worker Pool Integration. This aligns with the linear build order discovered in architecture research.

### Phase 1: Foundation Types
**Rationale:** No dependencies between modules in this phase. Can be implemented and tested in isolation without any integration concerns.
**Delivers:** `ExecutionResult` enum, `Executor` trait, `ExecutorOutcome`, `DefaultExecutor`, `PythonHandler` (receiver polling + spawn_blocking invocation)
**Addresses:** All P1 features from FEATURES.md - ExecutionResult normalization, Py<PyAny> storage pattern, spawn_blocking pattern
**Avoids:** Pitfalls 1 (raw pointer), 2 (GIL hold), 3 (Bound storage), 5 (exception escaping) - all addressed in the foundational types

### Phase 2: Worker Pool Integration
**Rationale:** Depends on Phase 1 components. Integration requires wiring WorkerPool to ConsumerDispatcher via `register_handler()` return value.
**Delivers:** `WorkerPool` struct with JoinSet workers, graceful shutdown coordination, `CancellationToken` propagation to workers, `HandlerMetadata::ack()` integration
**Uses:** `spawn_blocking` (from tokio), `Py::clone` (from pyo3), `Executor` trait (from Phase 1)
**Implements:** `WorkerPool::run()` spawning N Tokio tasks, each running `PythonHandler::process_one()` loop
**Avoids:** Pitfalls 4 (fake parallelism - via Semaphore from DISP-15), 6 (CancellationToken propagation), 7 (OwnedMessage Send+Sync - verified at compile time)

### Phase Ordering Rationale

1. **Foundation before integration** — Type definitions (`ExecutionResult`, `Executor`) have zero external dependencies. Implementing them first establishes the API contract before integration complexity.

2. **Linear dependency chain** — `ExecutionResult` -> `Executor` -> `PythonHandler` -> `WorkerPool` -> `ConsumerDispatcher` wiring. No parallel work is possible without creating circular dependencies.

3. **Pitfall prevention through phase structure** — Each phase includes its own pitfall mitigation. Phase 1 establishes correct PyO3 patterns (`Py<PyAny>`, `spawn_blocking`, exception handling). Phase 2 applies these patterns in the concurrent worker context.

### Research Flags

**Phases needing deeper research during planning:**
- **Phase 2 (Worker Pool Integration):** CancellationToken propagation to Python workers - complex shutdown coordination; needs integration test design before implementation

**Phases with standard patterns (skip research-phase):**
- **Phase 1 (Foundation Types):** All patterns (spawn_blocking, Py<PyAny> storage, Executor trait) are well-documented in PyO3/Tokio sources

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | PyO3 0.27.2 API analysis + existing codebase patterns; no new crates needed |
| Features | MEDIUM | Based on codebase analysis + PyO3 patterns; no external web search available |
| Architecture | HIGH | Clear component boundaries, linear dependency chain, verified against existing v1.1 dispatcher |
| Pitfalls | MEDIUM | Based on PyO3 internals, Rust async patterns, and existing pyconsumer.rs analysis |

**Overall confidence:** HIGH

### Gaps to Address

- **Executor retry policy details:** Retry backoff algorithm (exponential vs linear), max retry count defaults, retryable exception classification - defer to v1.x implementation
- **Concurrency limit calibration:** Optimal worker count depends on Python callback characteristics (I/O-bound vs CPU-bound) - needs profiling with real workloads
- **Shutdown timeout grace period:** How long to wait for in-flight Python calls to complete before force-killing

## Sources

### Primary (HIGH confidence)
- PyO3 0.27.2 source code (extension-module, generate-import-lib features)
- Tokio 1.40 `spawn_blocking` and `JoinSet` API documentation
- Existing codebase: `pyconsumer.rs` (Py<PyAny> storage), `dispatcher/mod.rs` (mpsc pattern), `queue_manager.rs` (ack tracking)

### Secondary (MEDIUM confidence)
- pyo3-async-runtimes 0.27.0 documentation for `future_into_py` GIL boundary
- Architecture patterns from `common/patterns.md` (Repository/Service layer idiom adapted to Rust)

---

*Research completed: 2026-04-16*
*Ready for roadmap: yes*
