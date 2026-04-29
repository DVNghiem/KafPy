# Pitfalls: Work-Stealing Thread Pool (Rayon) Integration for Tokio Kafka Consumer

**Domain:** Adding Rayon work-stealing thread pool to an existing Tokio-based Kafka consumer with PyO3 Python handlers
**Researched:** 2026-04-29
**Confidence:** MEDIUM-HIGH (based on Rust concurrency fundamentals + documented PyO3/Tokio/Rayon behavior)

---

## Executive Summary

KafPy v1.0 uses `spawn_blocking` for sync Python handlers, which works but treats all blocking work as fire-and-forget from Tokio's perspective. Adding Rayon (a work-stealing thread pool) enables true parallelism for CPU-intensive Rust preprocessing and parallel sync handler execution, but introduces threading complexity that does not exist with `spawn_blocking` alone.

**The core tension:** Tokio owns the async event loop threads; Rayon owns a separate pool of blocking threads. When the two intersect incorrectly, you get deadlocks, GIL corruption, or event-loop starvation.

---

## 1. Critical Pitfalls

### Pitfall 1.1: Rayon Callback Blocking the Tokio Event Loop

**What goes wrong:** A Rayon worker calls a function that synchronously invokes a Tokio primitive (e.g., sends to a `mpsc::Sender`, acquires a lock protected by a Tokio-watched Mutex, or awaits a future). This blocks the Rayon thread, but Tokio cannot steal work from that thread. If the blocked operation requires Tokio to make progress on another thread (e.g., the receiver is waiting on that same channel), you get a **deadlock**.

**Why it happens:** Tokio uses work-stealing on its own worker threads. Rayon has its own thread pool. These are separate pools. Tokio's scheduler cannot schedule work onto Rayon threads, and Rayon's scheduler does not understand Tokio tasks. When a Rayon worker blocks on a Tokio-watched primitive, the blocked operation may never complete because Tokio cannot run the receiver on a Rayon thread.

**Consequences:**
- Application deadlocks under load (no exception thrown, just frozen)
- `spawn_blocking` futures never complete
- Kafka consumer stops processing messages — heartbeat misses trigger rebalance

**Prevention:**
- **Isolate Rayon work from Tokio primitives.** All Tokio channels, mutexes, and futures must remain on Tokio threads. Rayon work communicates back to Tokio via the existing `spawn_blocking` boundary, not by directly invoking Tokio APIs.
- **Never call `tokio::spawn`, `mpsc::Sender::send`, or Tokio mutexes from within a Rayon `spawn` closure.** Use `spawn_blocking` to cross back to Tokio.
- Design: After Rayon processes a batch, return results via a channel owned by Tokio. Rayon pushes; Tokio polls.

**Detection:**
- Application freezes; `top` or `htop` shows Rayon threads at 100% CPU but no message throughput
- No new offsets committed; Kafka broker logs show consumer silence

**Phase:** Architecture + Implementation

---

### Pitfall 1.2: PyO3 GIL Acquisition on Rayon Threads (Wrong Python Thread State)

**What goes wrong:** When Python is called from a Rayon worker, the call succeeds initially but subsequent Python operations fail with `RuntimeError: Python is not initialized on this thread` or cause segmentation faults. This is the GIL pool / thread state mismatch.

**Why it happens:** PyO3's GIL is tied to a specific Python thread state (`PyThreadState`). PyO3 API calls like `Python::attach` acquire the GIL by attaching to the current thread's Python thread state. When called from a Rayon worker thread:
- The thread has no Python thread state
- `Python::attach` creates a temporary one, but Python's interpreter-level state (imports, sys.modules, GIL holder) may be inconsistent
- If `spawn_blocking` was used correctly, GIL acquisition happened on a Tokio blocking thread with proper Python state

**Consequences:**
- `PyErr` exceptions silently dropped or produce wrong values
- Segmentation faults when Python objects cross thread boundaries
- `Python::with_gil` closures capturing `Py<T>` created on different threads

**Prevention:**
- **Always call Python from `spawn_blocking`, never directly from Rayon `spawn`.** `spawn_blocking` runs closures on Tokio's dedicated blocking thread pool, which has proper Python thread state initialized by PyO3.
- If you need Rayon to parallelize Python handler calls, use `spawn_blocking` inside the Rayon closure:
  ```rust
  // Rayon scope calls Python handlers in parallel via spawn_blocking
  rayon::spawn(|| {
      // Each spawn_blocking call acquires GIL on a Tokio blocking thread
      futures::future::join_all(handlers.iter().map(|h| {
          h.invoke_async()  // internally uses spawn_blocking
      })).await;
  });
  ```
- **Never** call `Python::attach` directly inside `rayon::spawn`.

**Detection:**
- `RuntimeError: Python is not initialized on this thread` in logs
- Segmentation faults during parallel Python handler execution
- GIL metrics show inconsistent acquisition patterns

**Phase:** Implementation + Testing

---

### Pitfall 1.3: Memory Pressure from Oversized Rayon Pool Under Kafka Load

**What goes wrong:** Rayon is sized based on CPU cores (the default). Under high Kafka message throughput with concurrent handler execution, the combined memory footprint of Tokio threads + PyO3 GIL pools + Rayon threads + in-flight messages exceeds available RAM. OOM kills or swap thrashing results.

**Why it happens:** KafPy already has bounded `mpsc::channel` queues per handler. Adding Rayon for parallel sync handler execution multiplies concurrent Python handler invocations. Each Python call holds the GIL and some memory (Python objects, numpy arrays, etc.). If Rayon spawns parallel work equal to the bounded channel capacity (e.g., 1000 messages), and each holds Python memory, the RAM pressure is 1000x handler memory.

**Consequences:**
- OOM kills in Kubernetes with default memory limits
- RSS grows linearly with message backlog
- Swap thrashing causes massive latency spikes

**Prevention:**
- **Set Rayon thread pool size explicitly, not to CPU count.** The correct size is: `min(num_cpu, max_concurrent_python_handlers)`. Since Python handlers are GIL-bound, more Rayon threads than Python handlers provides no benefit.
- **Use a semaphore on Python handler concurrency** (existing in v1.0 via `Arc<Semaphore>`) even when Rayon is processing in parallel. Rayon's parallelism should mirror the semaphore-gated concurrency, not exceed it.
- **Configure bounded channels smaller than available memory budget.** If `max.poll.records` = 500 and each message = 10KB Python dict, 500 messages = 5MB per poll cycle. With 3 Rayon workers, 15MB. With unbounded Rayon, multiply by queue depth.
- Monitor `consumerLag` and `queue_depth` metrics; alert on growth rate.

**Detection:**
- RSS memory grows linearly with consumer lag
- OOM kills correlate with high throughput bursts
- `mem_usage` metric on Kubernetes shows memory pressure

**Phase:** Architecture (pool sizing) + Configuration

---

### Pitfall 1.4: Tokio `enter` Guard Violation — Nested Runtime Initialization

**What goes wrong:** A `#[tokio::test]` or some debugging/initialization code creates a Tokio runtime inside a context where a `enter` guard is already active (from the application already running). This causes a panic: "cannot start a runtime from within a context where the runtime is already running."

**Why it happens:** When adding Rayon alongside Tokio, initialization code that creates a Tokio runtime for testing or setup may accidentally run on a Rayon thread or within an existing Tokio context. Tokio's `enter` guard is a thread-local mechanism that tracks whether a runtime is active.

**Consequences:**
- Panic on startup or during tests
- Confusing error message about "event loop already running"

**Prevention:**
- **Avoid creating Tokio runtimes inside Rayon closures.** Initialization and setup should happen before Tokio starts or on Tokio threads via `spawn_blocking`.
- Use `tokio::runtime::Handle::current()` to access the existing runtime from any thread rather than creating a new one.

**Detection:**
- Panic message: "cannot start a runtime"
- Stack trace shows `tokio::runtime::EnterGuard` failure

**Phase:** Implementation

---

## 2. Warning Signs (How to Detect Early)

| Symptom | Likely Cause | Investigation |
|---------|-------------|---------------|
| Application freezes under load; CPU at 100% but no throughput | Rayon deadlock on Tokio channel | `pstack` or `py-spy` to see where threads are blocked |
| `RuntimeError: Python is not initialized on this thread` | Python called from Rayon thread without proper GIL setup | Search logs for PyO3 thread state errors |
| Memory grows linearly with backlog; OOM kills | Rayon pool too large relative to memory budget | Monitor RSS vs `consumerLag` correlation |
| Duplicate messages or offset corruption under load | Offset commit racing with Rayon parallel handler completion | Check commit ordering in logs |
| `ILLEGAL_GENERATION` errors spike | Rebalance during Rayon-in-flight processing | Correlate rebalance events with throughput spikes |
| Latency spikes (P99) | GIL held too long in parallel Python calls | Profile GIL hold times via `py-spy --gil` |
| Tokio tasks timing out unexpectedly | Tokio worker threads blocked by PyO3 GIL or Rayon | Check `tokio::spawn` task wait times |

---

## 3. Prevention Strategies

### Strategy 1: Thread Pool Isolation Architecture

**Design a clear boundary between Tokio and Rayon:**

```
Tokio Runtime (async)
  ├── Kafka fetch loop (rdkafka, blocking on dedicated thread)
  ├── Async worker tasks (message dispatch, offset tracking)
  └── Tokio blocking pool (Python handler GIL calls via spawn_blocking)
       │
       └── One-shot channel ──► Rayon thread pool
                                   └── Parallel CPU preprocessing
                                       (no Python, no Tokio primitives)
```

**Rules:**
1. Rayon work never calls Tokio APIs, futures, or channels directly
2. Rayon communicates results back to Tokio via `mpsc::Sender` from a `spawn_blocking` task
3. Python calls always go through `spawn_blocking` (not Rayon threads directly)
4. Tokio blocking threads are sized to Python handler concurrency; Rayon threads are sized to CPU preprocessing parallelism

### Strategy 2: Explicit Pool Configuration

```rust
// Tokio blocking pool — sized to Python concurrency (GIL serializes Python calls)
let blocking_pool = TokioCpuPool::new(num_python_handlers);

// Rayon thread pool — sized to CPU cores, NOT Python concurrency
// Keep this separate from the Tokio blocking pool
rayon::ThreadPoolBuilder::new()
    .num_threads(num_cpus::get())
    .build_global();

// Communication: Rayon pushes results to a channel
// Tokio worker receives via spawn_blocking
```

### Strategy 3: GIL-Aware Python Invocation from Rayon

If Rayon needs to invoke Python handlers in parallel, the pattern is:

```rust
// WRONG — will cause GIL thread state errors
rayon::spawn(|| {
    python_handler.invoke(); // Direct Python call on Rayon thread
});

// CORRECT — spawn_blocking bridges Rayon to Python thread state
rayon::spawn(|| {
    let handle = tokio::runtime::Handle::current();
    handle.spawn_blocking(|| {
        Python::attach(|py| {
            python_handler.invoke_py(py)
        })
    });
});
```

But this effectively serializes Python calls back through Tokio, negating Rayon's parallelism. **Alternative:** Accept that Python handlers are inherently single-threaded (GIL) and use Tokio's blocking pool for them, not Rayon. Use Rayon only for CPU-intensive Rust preprocessing that does not call Python.

### Strategy 4: Bounded Memory Budget

Calculate the maximum concurrent memory:
```
max_concurrent_messages * avg_message_size * handler_memory_multiplier
```

Set `max.poll.records` and channel capacity so that worst-case memory usage < available RAM * 0.7.

---

## 4. Phase Mapping

| Pitfall | Phase | Tasks |
|---------|-------|-------|
| 1.1 Tokio/Rayon deadlock | Architecture | Design Tokio-Rayon boundary; no Tokio primitives in Rayon closures |
| 1.2 GIL on Rayon threads | Implementation | Enforce `spawn_blocking` for all Python calls; code review checklist |
| 1.3 Memory pressure | Configuration | Set pool sizes; configure `max.poll.records` and channel capacity to memory budget |
| 1.4 Nested runtime | Implementation | Never create Tokio runtime from Rayon; use `Handle::current()` |
| Warning signs detection | Testing | Add integration tests with `py-spy`, memory profiling, deadlock detection |
| Rayon pool sizing | Architecture | Benchmark-driven sizing for CPU preprocessing vs Python concurrency |

---

## 5. Interaction with Existing v1.0 Architecture

The v1.0 architecture already has:
- `spawn_blocking` for Python handlers (works correctly with PyO3)
- Bounded `mpsc::channel` per handler
- `Arc<Semaphore>` per handler for concurrency limiting

**What changes with Rayon:**
- CPU-intensive Rust preprocessing can happen in parallel before Python handlers run
- Multiple sync Python handlers can execute concurrently (limited by Semaphore, not by channel)
- New deadlock surfaces because Rayon threads are not Tokio threads

**What stays the same:**
- Python GIL acquisition must still go through `spawn_blocking`
- Semaphore-bounded concurrency remains the backpressure mechanism
- Offset tracking and commit ordering remain single-threaded (no change needed)

---

## 6. Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Tokio/Rayon deadlock risk | MEDIUM-HIGH | Documented in Tokio and Rayon communities; well-understood pattern |
| GIL on Rayon threads | MEDIUM-HIGH | PyO3 GIL behavior is documented; Rayon thread state issue is specific |
| Memory pressure analysis | MEDIUM | Depends on message size distribution; formula is correct but numbers are project-specific |
| Nested runtime pitfall | HIGH | Tokio runtime contract is well-documented |
| Pool sizing strategy | MEDIUM | Principle is sound; exact numbers need benchmarking |

---

## 7. Key References

- [Tokio spawn_blocking docs](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html) — blocking thread pool semantics
- [PyO3 GitHub issue #58](https://github.com/PyO3/pyo3-async-runtimes) — GIL + async runtime interaction
- [Rayon thread pool configuration](https://docs.rs/rayon/latest/rayon/struct.ThreadPoolBuilder.html) — global pool sizing
- [Tokio runtime enter guard](https://docs.rs/tokio/latest/tokio/runtime/struct.EnterGuard.html) — nested runtime prevention

---

## 8. Open Questions for Phase-1 Research

1. **Rayon pool granularity:** Global pool vs per-handler pool — is global pool safe when different handlers have different CPU/Rust ratios?
2. **Batch mode + Rayon interaction:** If batch handlers use Rayon to parallelize preprocessing, does the existing `invoke_batch` architecture still hold?
3. **Graceful shutdown with Rayon:** When Tokio shuts down, does Rayon drain gracefully? Or does it need explicit shutdown coordination?
