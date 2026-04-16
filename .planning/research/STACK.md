# Stack Research: Python Execution Lane

**Domain:** PyO3 Rust Kafka client - Python execution lane
**Researched:** 2026-04-16
**Confidence:** HIGH (based on PyO3 0.27.2 API analysis and existing codebase patterns)

## Executive Summary

The Python execution lane requires bridging Tokio async workers with Python callable invocation while minimizing GIL hold time. The existing stack (PyO3 0.27.2, pyo3-async-runtimes 0.27.0, Tokio 1.40, parking_lot 0.12) is sufficient. No new crate dependencies are needed. The key addition is architectural: a WorkerPool that pulls from mpsc channels and invokes Python via `spawn_blocking`.

## Recommended Stack

### Core Technologies (Already Present)

| Technology | Version | Status | Purpose |
|------------|---------|--------|---------|
| pyo3 | 0.27.2 | Existing | Python-Rust FFI, Py<PyAny> for GIL-independent storage |
| pyo3-async-runtimes | 0.27.0 | Existing | `future_into_py` for Python async boundary |
| tokio | 1.40 | Existing | Async runtime, `spawn_blocking` for GIL release |
| parking_lot | 0.12 | Existing | Fast mutex for handler map access |

### Architecture Components (No New Crates)

| Component | Implementation | Purpose |
|-----------|---------------|---------|
| Python storage | `Py<PyAny>` (existing) | GIL-independent callback storage |
| GIL release | `tokio::task::spawn_blocking` | Minimal GIL hold window |
| Worker pool | `tokio::task::JoinSet` | Manage concurrent workers |
| Work distribution | Tokio `mpsc::Receiver` (existing) | Pull from dispatcher queues |
| Executor trait | Custom trait in Rust | Retry/commit/async/batch policies |

## Py<PyAny> Storage Pattern

**Current state:** `pyconsumer.rs` uses `Arc<Mutex<HashMap<String, Py<PyAny>>>>` for callback storage.

**Correct pattern for GIL-independent storage:**

```rust
// Storage: Py<PyAny> is Send + Sync when cloned correctly
handlers: Arc<parking_lot::Mutex<HashMap<String, Py<PyAny>>>>

// Cloning for cross-thread use:
let handler = handler.clone();  // Py<PyAny> is Arc-based, cheap clone
let py_msg = py_msg.clone();

// Access pattern:
// 1. Lock the map (parking_lot::Mutex, not std::sync::Mutex)
// 2. Get &Py<PyAny>
// 3. Drop lock BEFORE calling Python
// 4. Use spawn_blocking for Python invocation
```

**Why parking_lot over std::sync::Mutex:**
- No poison semantics (easier error handling)
- Faster uncontended lock acquisition
- Already in Cargo.toml

## Python Invocation: spawn_blocking

**Why spawn_blocking:**

| Approach | GIL Hold | Throughput | Complexity |
|----------|----------|------------|------------|
| Direct call in async context | Full duration | Low | Low |
| `Python::spawn` (pyo3 0.20+) | Full duration | Medium | Medium |
| `spawn_blocking` | Only during actual Python call | High | Medium |

**Recommended pattern:**

```rust
// Minimal GIL hold window
let handler = handler.clone();
let py_msg = py_msg.clone();

tokio::task::spawn_blocking(move || {
    Python::with_gil(|py| {
        let py_msg = py_msg.as_ref(py);
        handler.call1(py, (py_msg,)).map_err(|e| e.to_string())
    })
})
.await
.map_err(|e| PyErr::from(e))?;
```

**Key principles:**
1. Clone `Py<PyAny>` before entering async block (cheap, Arc-based)
2. Call `Python::with_gil` inside `spawn_blocking`
3. Minimize work inside `with_gil` closure
4. Convert errors OUTSIDE the GIL if possible

**Python::with_gil vs Python::spawn:**

- `Python::with_gil(|py| { ... })` - acquire GIL, run closure, release GIL
- `Python::spawn(|py| { ... })` - spawn a thread with GIL (pyo3 0.20+)
- For `spawn_blocking`, use `with_gil` since Tokio manages the thread

## WorkerPool Architecture

**Design:** Rust owns orchestration; Python callbacks are invoked in worker tasks.

```
┌─────────────────────────────────────────────────────────┐
│  ConsumerRunner (Tokio task)                           │
│  - Produces OwnedMessage via stream                    │
└─────────────────────┬───────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────────────┐
│  ConsumerDispatcher                                     │
│  - Routes to per-topic bounded mpsc channels           │
└─────────────────────┬───────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────────────┐
│  mpsc::Receiver<OwnedMessage> (per topic)               │
│  - Bounded queue with configurable capacity            │
└─────────────────────┬───────────────────────────────────┘
                      │
                      ▼
┌─────────────────────────────────────────────────────────┐
│  WorkerPool                                            │
│  - JoinSet manages N worker tasks                     │
│  - Each worker: recv() -> spawn_blocking -> invoke     │
│  - Executor trait for retry/commit/async/batch         │
└─────────────────────────────────────────────────────────┘
```

**WorkerPool implementation sketch:**

```rust
pub struct WorkerPool {
    workers: tokio::task::JoinSet<WorkerResult>,
    receiver: mpsc::Receiver<OwnedMessage>,
    executor: Arc<dyn Executor>,
}

impl WorkerPool {
    pub fn new(
        receiver: mpsc::Receiver<OwnedMessage>,
        executor: Arc<dyn Executor>,
        num_workers: usize,
    ) -> Self {
        let mut pool = Self {
            workers: JoinSet::new(),
            receiver,
            executor,
        };
        for _ in 0..num_workers {
            pool.spawn_worker();
        }
        pool
    }

    fn spawn_worker(&mut self) {
        let executor = Arc::clone(&self.executor);
        self.workers.spawn(async move {
            // Worker loop - recv from channel, invoke via spawn_blocking
            loop {
                // This recv() is cancellation-safe
                if let Some(msg) = receiver.recv().await {
                    let result = executor.execute(msg).await;
                    if result.is_err() {
                        // Log error, decide whether to continue
                    }
                } else {
                    break; // Sender dropped
                }
            }
        });
    }

    pub async fn join(self) {
        self.workers.join_all().await;
    }
}
```

## Executor Trait for Future Policies

**Design pattern:** Strategy/trait-based for retry, commit, async, batch policies.

```rust
/// Outcome of executing a message handler.
pub enum ExecutionResult {
    /// Handler succeeded.
    Success,
    /// Handler failed, optionally retryable.
    Failed { error: String, retryable: bool },
}

/// Executor trait - implemented for different policies.
pub trait Executor: Send + Sync {
    /// Execute handler for a single message.
    /// Called from worker task; MUST be async to support future policies.
    fn execute(&self, msg: OwnedMessage) -> impl Future<Output = ExecutionResult> + Send;
}

/// Retry executor - retries with exponential backoff.
pub struct RetryExecutor {
    max_retries: u32,
    base_delay: Duration,
    inner: Arc<dyn Executor>,
}

impl RetryExecutor {
    pub fn new(max_retries: u32, base_delay: Duration, inner: Arc<dyn Executor>) -> Self {
        Self { max_retries, base_delay, inner }
    }
}

impl Executor for RetryExecutor {
    async fn execute(&self, msg: OwnedMessage) -> ExecutionResult {
        let mut delay = self.base_delay;
        for attempt in 0..=self.max_retries {
            match self.inner.execute(msg.clone()).await {
                ExecutionResult::Success => return ExecutionResult::Success,
                ExecutionResult::Failed { error, retryable } if retryable && attempt < self.max_retries => {
                    tokio::time::sleep(delay).await;
                    delay *= 2; // Exponential backoff
                }
                ExecutionResult::Failed { error, .. } => {
                    return ExecutionResult::Failed { error, retryable: false };
                }
            }
        }
        ExecutionResult::Failed { error: "max retries exceeded".into(), retryable: false }
    }
}

/// Batch executor - accumulates messages, invokes in batch.
pub struct BatchExecutor {
    batch_size: usize,
    timeout: Duration,
    inner: Arc<dyn Executor>,
}
```

## spawn_blocking vs Thread-Per-Worker

**Decision criteria:**

| Factor | spawn_blocking | Dedicated Thread Pool |
|--------|----------------|----------------------|
| Callback duration | Short (<10ms) | Long-running |
| Throughput needs | Medium | High |
| CPU-bound Python | Poor | Good |
| IO-bound Python | Good | Good |
| Code complexity | Low | High |
| GIL contention | Low | Manageable |

**Recommendation:** Start with `spawn_blocking` only.

**Rationale:**
1. Tokio maintains an internal thread pool for blocking tasks (size: `tokio::runtime::Builder::max_blocking_threads`)
2. For Kafka message processing, callbacks are typically IO-bound (database writes, HTTP calls)
3. Avoids complexity of managing a separate thread pool
4. Can upgrade to dedicated threads later if profiling shows blocking issues

**When to add dedicated threads:**
- Python callbacks are CPU-intensive (image processing, ML inference)
- Latency requirements are microsecond-level
- Profiling shows Tokio blocking pool saturation

**Implementation if needed:**

```rust
// Instead of spawn_blocking, use a dedicated thread pool
use std::thread;

pub struct DedicatedThreadPool {
    workers: Vec<thread::JoinHandle<()>>,
    sender: mpsc::Sender<OwnedMessage>,
}

impl DedicatedThreadPool {
    pub fn new(num_threads: usize) -> Self {
        let (tx, rx) = mpsc::channel(1000);
        let workers = (0..num_threads)
            .map(|_| {
                let rx = rx.clone();
                thread::spawn(move || {
                    Python::with_gil(|py| {
                        worker_loop(py, rx);
                    })
                })
            })
            .collect();
        Self { workers, sender: tx }
    }
}
```

## No New Crate Dependencies Required

**Cargo.toml already contains:**

| Dependency | Purpose | Used For |
|------------|---------|----------|
| pyo3 0.27.2 | Python bindings | Py<PyAny>, GIL management |
| pyo3-async-runtimes 0.27.0 | Async bridging | future_into_py |
| tokio 1.40 | Async runtime | spawn_blocking, JoinSet |
| parking_lot 0.12 | Mutex | Handler map storage |
| async-trait 0.1 | Async traits | Executor trait |
| thiserror 2.0 | Errors | ExecutionResult errors |

**No additions needed for v1.2 scope.**

## Alternatives Considered

| Approach | Why Not | When Better |
|----------|--------|-------------|
| `async-std` instead of Tokio | Existing stack uses Tokio | N/A (committed to Tokio) |
| `rayon` for parallelism | Rayon is for data parallelism, not Python callbacks | CPU-heavy batch processing |
| `flume` channel | Tokio mpsc is sufficient | Very high throughput (>1M msg/s) |
| `crossbeam-channel` | Tokio mpsc is sufficient | Multi-runtime communication |
| Dedicated thread pool | Adds complexity without benefit for IO-bound callbacks | CPU-intensive Python work |

## What NOT to Use

| Pattern | Avoid Because |
|---------|----------------|
| `Python::attach()` in async context | Blocks async executor, defeats purpose of async |
| `std::sync::Mutex` over `parking_lot::Mutex` | Poison semantics complicate error handling, slower |
| Unbounded channels in hot path | Memory unboundedness can OOM |
| Calling Python without `spawn_blocking` | Holds GIL for entire async task duration |

## Implementation Order

**Phase 1: WorkerPool skeleton**
- Create `WorkerPool` struct with `JoinSet`
- Basic message receive loop
- Single `spawn_blocking` invocation

**Phase 2: Executor trait**
- Define `Executor` trait
- Implement `DefaultExecutor` (direct invocation)
- Implement `RetryExecutor`

**Phase 3: Error handling + ack integration**
- Map Python errors to `ExecutionResult`
- Integrate with `QueueManager::ack()` for inflight tracking

**Phase 4: Concurrency tuning**
- Add `num_workers` configuration
- Benchmark with realistic workloads
- Add dedicated threads only if profiling demands

## Sources

- PyO3 0.27.2 source code (features: "extension-module", "generate-import-lib")
- pyo3-async-runtimes 0.27.0 documentation
- Tokio 1.40 `spawn_blocking` and `JoinSet` API
- Existing `pyconsumer.rs` patterns (line 24: `handlers: Arc<Mutex<HashMap<String, Py<PyAny>>>>`)
- Existing `queue_manager.rs` patterns (line 139: `parking_lot::Mutex`)

---

*Stack research for: Python Execution Lane v1.2*
*Researched: 2026-04-16*
