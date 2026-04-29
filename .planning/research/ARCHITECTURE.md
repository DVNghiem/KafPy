# Architecture: Tokio + Rayon Thread Pool Integration

**Project:** KafPy v1.1 — Async & Concurrency Hardening
**Domain:** Work-stealing thread pool (Rayon) for blocking sync handlers
**Researched:** 2026-04-29

---

## System Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           Tokio Runtime                                  │
│                                                                          │
│  ┌──────────────┐     ┌─────────────────┐     ┌──────────────────────┐  │
│  │   Consumer   │────▶│    Dispatcher    │────▶│   Handler Queue      │  │
│  │   Runner     │     │   (topic-based)  │     │   (mpsc channel)     │  │
│  │  (rdkafka)   │     │                  │     │                      │  │
│  └──────────────┘     └─────────────────┘     └──────────┬───────────┘  │
│                                                             │              │
│                         ┌────────────────────────────────────┘              │
│                         ▼                                              │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │                    Worker Pool (N Tokio tasks)                  │    │
│  │                                                                  │    │
│  │   ┌──────────┐  ┌──────────┐       ┌──────────┐                 │    │
│  │   │Worker 0  │  │Worker 1  │  ...  │Worker N  │                 │    │
│  │   │async fn  │  │async fn  │       │async fn  │                 │    │
│  │   └─────┬────┘  └────┬────┘       └────┬────┘                 │    │
│  │         │             │                 │                       │    │
│  │         ▼             ▼                 ▼                       │    │
│  │   ┌─────────────────────────────────────────────────────────┐   │    │
│  │   │              Rayon Thread Pool (blocking work)        │   │    │
│  │   │                                                          │   │    │
│  │   │   ┌────────┐  ┌────────┐  ┌────────┐  ┌────────┐         │   │    │
│  │   │   │ Rayon  │  │ Rayon  │  │ Rayon  │  │ Rayon  │         │   │    │
│  │   │   │ Thread │  │ Thread │  │ Thread │  │ Thread │         │   │    │
│  │   │   │   0    │  │   1    │  │   2    │  │   3    │  ...    │   │    │
│  │   │   └────────┘  └────────┘  └────────┘  └────────┘         │   │    │
│  │   │                                                          │   │    │
│  │   │   work-stealing: idle threads steal tasks from busy ones │   │    │
│  │   └─────────────────────────────────────────────────────────┘   │    │
│  └─────────────────────────────────────────────────────────────────┘    │
│                         ▲                                              │
│                         │                                              │
│              ┌──────────┴───────────┐                                   │
│              │  Python Handler     │                                   │
│              │  (sync or async)     │                                   │
│              │  invoked via PyO3    │                                   │
│              └─────────────────────┘                                   │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Why Rayon for Blocking Handlers

**Problem:** Long-running sync Python handlers block the Tokio event loop, causing poll cycle delays that lead to heartbeat misses and rebalances.

**Solution:** Rayon provides a work-stealing thread pool specifically designed for CPU-bound and blocking work. By dispatching sync handlers to Rayon, Tokio remains free to poll Kafka.

| Characteristic | Tokio (async tasks) | Rayon (blocking work) |
|---------------|---------------------|----------------------|
| Model | Non-blocking async tasks | Work-stealing thread pool |
| Blocking behavior | Blocked tasks stall the runtime | Tasks yield to other Rayon work |
| GIL awareness | Python calls must be on a thread with GIL | Same — Python calls still need GIL |
| Use for | Kafka poll, channel ops, async I/O | Long-running sync Python handlers |
| Heartbeat impact | Blocking in async context stalls polls | Does not block Tokio event loop |

---

## Component Responsibilities

### New Components

| Component | File | Responsibility |
|-----------|------|----------------|
| `RayonPool` | `src/worker_pool/rayon_pool.rs` | Owns the Rayon thread pool, submits blocking work |
| `ThreadPoolBridge` | `src/worker_pool/thread_pool_bridge.rs` | Converts Tokio async context to Rayon blocking call |

### Modified Components

| Component | File | Change |
|-----------|------|--------|
| `worker_loop` | `src/worker_pool/worker.rs` | Route sync handlers to Rayon; async handlers stay on Tokio |
| `PythonHandler` | `src/python/handler.rs` | Add `is_blocking()` method to distinguish sync/async modes |
| `HandlerMode` | `src/python/handler.rs` | Add variant for thread-pool-backed sync handlers |
| `WorkerPool` | `src/worker_pool/pool.rs` | Initialize and hold `Arc<RayonPool>` |

### Unchanged Components

- `ConsumerRunner` — Tokio owns poll cycle, no changes needed
- `Dispatcher` — routing unchanged, only execution changes
- `OffsetCoordinator` — offset tracking independent of execution model
- `ShutdownCoordinator` — drain signaling unchanged

---

## Data Flow: Message Processing with Thread Pool

```
1. ConsumerRunner::run() polls rdkafka → produces OwnedMessage
                                    │
2. Dispatcher routes message to handler-specific queue
                                    │
3. WorkerLoop picks up message from mpsc::Receiver
   (tokio::select! on rx.recv())
                                    │
4. Acquire concurrency permit (Semaphore) ─────────────────┐
                                                            │
5. Determine handler mode:                                  │
   ┌─────────────────────────────────────────────────────┐   │
   │ if is_blocking_handler(handler.mode()) {             │   │
   │     // Use Rayon for sync handlers                   │   │
   │     rayon_pool.spawn_blocking(move || {             │   │
   │         handler.invoke_sync(&ctx, &msg)              │   │
   │     }).await                                         │   │
   │ } else {                                              │   │
   │     // Use async path for async handlers             │   │
   │     handler.invoke_async(&ctx, msg).await            │   │
   │ }                                                     │   │
   └─────────────────────────────────────────────────────┘   │
                                                            │
6. Release concurrency permit ◀────────────────────────────┘
                                    │
7. Executor decides outcome (Ack/Retry/Rejected)
                                    │
8. OffsetCoordinator records position
```

---

## Integration Patterns for Blocking Work

### Pattern 1: Dedicated Rayon Pool (Recommended)

```rust
// src/worker_pool/rayon_pool.rs

use rayon::ThreadPool;
use std::sync::Arc;

pub struct RayonPool {
    pool: ThreadPool,
}

impl RayonPool {
    pub fn new(num_threads: usize) -> Arc<Self> {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .thread_name(|i| format!("rayon-worker-{}", i))
            .build()
            .expect("rayon pool creation failed");

        Arc::new(Self { pool })
    }

    /// Submit a blocking sync handler to the Rayon thread pool.
    /// The async future resolves once the blocking work completes.
    pub async fn spawn_blocking<F, T>(&self, f: F) -> Result<T, SpawnError>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let (tx, rx) = std::sync::mpsc::channel();

        self.pool.spawn(move || {
            let result = f();
            let _ = tx.send(result);
        });

        // Block on a Tokio blocking task — not a regular async task.
        // This keeps Tokio's event loop free while waiting.
        tokio::task::spawn_blocking(move || rx.recv().unwrap())
            .await
            .map_err(|e| SpawnError::Cancelled(e.to_string()))?
            .map_err(|e| SpawnError::Panic(e.to_string()))
    }
}
```

### Pattern 2: Tokio-Rayon Bridge for GIL-Python Calls

```rust
/// Execute a Python handler on a Rayon thread with GIL.
/// This is the bridge between Tokio async context and Rayon blocking work.
pub async fn execute_python_on_rayon<F, T>(
    pool: &RayonPool,
    f: F,
) -> Result<T, ExecuteError>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    pool.spawn_blocking(f).await
}
```

### Pattern 3: Handler Mode Dispatch

```rust
// In worker_loop.rs — determines where to run handler

match handler.mode() {
    HandlerMode::SingleAsync => {
        // Async handlers stay on Tokio — no thread pool needed
        handler.invoke_async(&ctx, msg).await
    }
    HandlerMode::SingleSync => {
        // Sync handlers go to Rayon — non-blocking for Tokio poll cycle
        rayon_pool.spawn_blocking(move || {
            handler.invoke_sync(&ctx, &msg)
        }).await
    }
    HandlerMode::BatchSync => {
        // Batch sync — process entire batch on Rayon
        rayon_pool.spawn_blocking(move || {
            handler.invoke_batch_sync(&ctx, batch)
        }).await
    }
}
```

---

## Anti-Patterns to Avoid

### Anti-Pattern 1: Blocking Tokio's Event Loop

```rust
// BAD — long Python sync call blocks the entire Tokio event loop
async fn worker_loop(...) {
    let result = handler.invoke_sync(&ctx, &msg).await; // blocks poll cycle!
}
```

```rust
// GOOD — dispatch to Rayon, Tokio remains free to poll Kafka
async fn worker_loop(...) {
    let result = rayon_pool.spawn_blocking(move || {
        handler.invoke_sync(&ctx, &msg)
    }).await;
}
```

### Anti-Pattern 2: Unbounded Thread Pool

```rust
// BAD — no limit on threads causes resource exhaustion under load
let pool = rayon::ThreadPoolBuilder::new()
    .num_threads(0) // unrestricted — dangerous
    .build();
```

```rust
// GOOD — explicit thread limit, tuned for workload
let pool = rayon::ThreadPoolBuilder::new()
    .num_threads(num_cpus::get() * 2) // or configurable
    .build();
```

### Anti-Pattern 3: Releasing GIL in Python C API Calls

```rust
// BAD — releasing GIL in blocking Python code causes crashes
rayon::spawn(|| {
    // Python C API calls MUST hold the GIL — this crashes
    Python::with_gil(|py| { /* GIL held here */ }); // OK
    drop_gil(); // RAYON THREAD HAS NO GIL — crash
});
```

```rust
// GOOD — PyO3 retains GIL automatically during Python::call()
// Rayon thread has GIL for the duration of the Python call
rayon::spawn(move || {
    Python::call(/* retains GIL automatically via PyO3 */);
});
```

### Anti-Pattern 4: Nested Tokio Inside Rayon

```rust
// BAD — blocking inside spawn_blocking that then calls back to Tokio
tokio::task::spawn_blocking(move || {
    let rt = tokio::runtime::Handle::current();
    rt.block_on(async { /* nested Tokio inside blocking — causes deadlock */ });
});
```

---

## Thread Pool Configuration

| Parameter | Recommended Value | Rationale |
|-----------|-------------------|-----------|
| `num_threads` | `num_cpus::get() * 2` or configurable | Balances parallelism with memory overhead |
| `thread_name` | `rayon-worker-{index}` | Easier debugging and profiling |
| `stack_size` | Default (sufficient for Python handlers) | Only increase if deep Python call stacks |
| `queue_depth` | Unlimited (work-stealing is inherent) | Rayon handles load internally |
| `shutdown_timeout` | 5 seconds | Allow graceful drain before hard abort |

---

## Build Order

```
1. src/worker_pool/rayon_pool.rs          (new — core Rayon pool)
2. src/worker_pool/thread_pool_bridge.rs  (new — Tokio/Rayon bridge)
3. src/python/handler.rs                 (modify — add is_blocking(), new mode variant)
4. src/worker_pool/worker.rs             (modify — dispatch to Rayon for sync handlers)
5. src/worker_pool/pool.rs              (modify — initialize RayonPool, pass to workers)
6. src/pyconsumer.rs                     (modify — wire Rayon pool to workers)
```

---

## Interaction with Existing Concurrency Controls

The Rayon pool **works alongside** existing controls, not replaces them:

| Control | Mechanism | Interaction |
|---------|-----------|-------------|
| `Arc<Semaphore>` per handler key | Limits concurrent handler executions | Still acquired BEFORE dispatching to Rayon |
| `WorkerPool` (JoinSet) | Tokio tasks for message polling | Worker task dispatches to Rayon for sync handlers |
| `CancellationToken` | Graceful shutdown signaling | Rayon threads receive cancel via their task handles |
| `ShutdownCoordinator` | Phased drain (Running -> Draining -> Finalizing) | Worker pool drain waits for Rayon tasks via `join_set.shutdown()` |

---

## Sources

- [Rayon documentation](https://docs.rs/rayon/latest/rayon/) — thread pool configuration
- [Tokio spawn_blocking](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html) — blocking task bridging
- KafPy existing worker pool architecture: `.claude/worktrees/agent-ad7fb0ce/src/worker_pool/pool.rs`, `worker.rs`