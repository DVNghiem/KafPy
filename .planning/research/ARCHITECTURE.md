# Architecture Research: Python Execution Lane

**Domain:** PyO3 Native Extension - Kafka Consumer Python Callback Invocation
**Researched:** 2026-04-16
**Confidence:** HIGH

## Executive Summary

The Python execution lane is a layer that sits between the dispatcher (which routes `OwnedMessage` to per-handler bounded channels) and the Python callbacks (which process messages). It receives messages from `mpsc::Receiver<OwnedMessage>` (returned by `ConsumerDispatcher::register_handler()`), polls them asynchronously, invokes Python callbacks via `spawn_blocking`, and returns normalized `ExecutionResult` values.

The architecture introduces four new components: `ExecutionResult` (outcome enum), `PythonHandler` (callback invoker), `WorkerPool` (concurrency manager), and `Executor` trait (policy interface). Data flows: `ConsumerRunner` → `ConsumerDispatcher.send()` → handler's `mpsc::Receiver` → `WorkerPool` → `PythonHandler` → Python callback → `ExecutionResult`.

The build order is: `Executor` trait and `ExecutionResult` (no deps) → `PythonHandler` (depends on trait) → `WorkerPool` (depends on handler) → PyO3 bridge (integrates with `ConsumerDispatcher`).

## Integration with Existing Architecture

### Current State (After v1.1)

```
ConsumerRunner (src/consumer/runner.rs)
    └─ stream() → ConsumerStream (impl Stream<Item=Result<OwnedMessage, ConsumerError>>)
            │
ConsumerDispatcher (src/dispatcher/mod.rs)
    ├─ owns: ConsumerRunner + Dispatcher
    ├─ run() → polls ConsumerStream, calls dispatcher.send()
    └─ register_handler(topic, capacity, max_concurrency)
            → returns mpsc::Receiver<OwnedMessage>

Dispatcher (src/dispatcher/mod.rs)
    └─ send(OwnedMessage) → Result<DispatchOutcome, DispatchError>
            └─ routes to per-topic bounded channel

PyConsumer (src/pyconsumer.rs)  [NOT using dispatcher yet]
    ├─ stores Py<PyAny> callbacks in HashMap
    ├─ directly streams from ConsumerRunner
    └─ calls Python via pyo3::Python::attach() in stream loop
```

### Target State (After v1.2)

```
ConsumerRunner
    └─ stream() → ConsumerStream
            │
ConsumerDispatcher
    ├─ run() polls stream, calls dispatcher.send()
    └─ register_handler(topic, capacity, max_concurrency)
            └─ returns mpsc::Receiver<OwnedMessage>
                    │
                    ▼
PythonHandler (new)
    ├─ poll mpsc::Receiver<OwnedMessage>
    ├─ convert to KafkaMessage
    ├─ call Python via spawn_blocking + GIL
    └─ return ExecutionResult

WorkerPool (new)
    └─ N workers polling same receiver (compete for messages)
            │
            ▼
Executor trait (new)  [pluggable policy]
    └─ handle(ExecutionResult) → handles retry/commit/ack
```

## New Components

### 1. ExecutionResult

**Purpose:** Normalized outcome from Python callback execution.

```rust
// New file: src/python/execution_result.rs
#[derive(Debug, Clone)]
pub enum ExecutionResult {
    /// Python callback returned successfully.
    Ok,
    /// Python callback raised an exception.
    Error { exception: String, traceback: String },
    /// Message was rejected by callback (e.g., schema mismatch, business rule).
    Rejected { reason: String },
}
```

**Why normalize:** `Py<PyAny>` call can return `PyResult<PyAny>`, which contains either success or a Python exception. We need to map this to a Rust enum that can be passed across thread boundaries and inspected by an `Executor` for retry/commit decisions. `ExecutionResult` is simple (no PyO3 types), Send+Sync, and serde-serializable for future metrics.

**Source of truth:** Python callback return value / exception caught via `PyErr::fetch()`.

### 2. PythonHandler

**Purpose:** Owns a `mpsc::Receiver<OwnedMessage>` and a `Py<PyAny>` callback. Polls the receiver and invokes the callback.

```rust
// New file: src/python/handler.rs
use pyo3::prelude::*;
use tokio::sync::mpsc;
use crate::consumer::OwnedMessage;
use crate::kafka_message::KafkaMessage;

pub struct PythonHandler {
    receiver: mpsc::Receiver<OwnedMessage>,
    callback: Py<PyAny>,
}

impl PythonHandler {
    /// Polls the receiver once, invokes Python, returns result.
    /// Returns None when receiver is closed.
    pub async fn poll_once(&mut self) -> Option<ExecutionResult> {
        let msg = self.receiver.recv().await?;
        let result = self.invoke(msg).await;
        Some(result)
    }

    /// Invokes Python callback via spawn_blocking.
    async fn invoke(&self, msg: OwnedMessage) -> ExecutionResult {
        let py_msg = KafkaMessage::from(msg);
        let callback = self.callback.clone();
        let result = tokio::task::spawn_blocking(move || {
            pyo3::Python::with(|py| {
                let cb = callback.as_ref(py);
                match cb.call1((py_msg,)) {
                    Ok(_) => ExecutionResult::Ok,
                    Err(e) => {
                        let exception = e.stringify(py).unwrap_or_else(|_| "unknown".to_string());
                        ExecutionResult::Error { exception, traceback: String::new() }
                    }
                }
            })
        }).await.unwrap_or_else(|e| ExecutionResult::Error {
            exception: format!("spawn_blocking failed: {e}"),
            traceback: String::new()
        });
        result
    }
}
```

**Key design decisions:**
- `Py<PyAny>` is Send+Sync — can be cloned and moved into `spawn_blocking` without GIL
- `spawn_blocking` holds the GIL only for the duration of `handler.call1()`, minimizing GIL contention
- `KafkaMessage::from(msg)` copies fields to owned Python objects at the boundary

### 3. Executor Trait

**Purpose:** Pluggable policy for what happens after Python returns `ExecutionResult`. Enables retry logic, offset commit decisions, DLQ routing, async processing, or batch accumulation — without coupling `PythonHandler` to specific policies.

```rust
// New file: src/python/executor.rs
use crate::consumer::OwnedMessage;

/// Outcome of an execute() call — signals to WorkerPool how to proceed.
#[derive(Debug)]
pub enum ExecutorOutcome {
    /// Message was processed successfully — advance offset.
    Ack,
    /// Message failed — apply retry or signal error.
    Retry { count: u32, delay_ms: u64 },
    /// Message was rejected by business logic — route to DLQ or log.
    Rejected { reason: String },
}

/// Policy interface for handling ExecutionResult from Python callbacks.
pub trait Executor: Send + Sync {
    /// Called after Python callback returns.
    /// Receives the original OwnedMessage and the ExecutionResult.
    /// Returns what action the WorkerPool should take.
    fn execute(&self, msg: OwnedMessage, result: ExecutionResult) -> ExecutorOutcome;
}

/// Default executor — ACK everything, no retry.
pub struct DefaultExecutor;

impl Executor for DefaultExecutor {
    fn execute(&self, _msg: OwnedMessage, result: ExecutionResult) -> ExecutorOutcome {
        match result {
            ExecutionResult::Ok => ExecutorOutcome::Ack,
            ExecutionResult::Error { .. } => ExecutorOutcome::Retry { count: 1, delay_ms: 100 },
            ExecutionResult::Rejected { reason } => ExecutorOutcome::Rejected { reason },
        }
    }
}
```

**Why a trait:** Different consumers have different needs. A batch consumer wants to accumulate messages before ACKing. A real-time consumer wants immediate ACK. A DLQ-capable consumer wants to route rejections to a dead-letter topic. The `Executor` trait decouples the execution loop from the policy.

**Lifetime considerations:** `Executor::execute()` takes `OwnedMessage` by value (owned) — this avoids tying the trait to the receiver's lifetime and keeps the PyO3 boundary clean (DISP-17: "Python integration boundary preserved — dispatcher API uses only owned types").

### 4. WorkerPool

**Purpose:** Runs N concurrent workers, each polling the same `mpsc::Receiver<OwnedMessage>`. Tokio's `select!` with `biased` on `recv()` provides fair distribution across workers. Workers invoke callbacks via `PythonHandler` and report results to the `Executor`.

```rust
// New file: src/python/worker_pool.rs
use tokio::select;
use tokio::time::{sleep, Duration};

pub struct WorkerPool {
    workers: usize,
    handler: Arc<PythonHandler>,
    executor: Arc<dyn Executor>,
}

impl WorkerPool {
    pub fn new(
        workers: usize,
        handler: Arc<PythonHandler>,
        executor: Arc<dyn Executor>,
    ) -> Self {
        Self { workers, handler, executor }
    }

    /// Starts N workers, each running the poll-invoke-report loop.
    pub async fn run(&self) {
        let handlers: Vec<_> = (0..self.workers)
            .map(|_| self.handler.clone())
            .collect();

        let futures: Vec<_> = handlers
            .into_iter()
            .map(|h| self.run_worker(h))
            .collect();

        futures::future::join_all(futures).await;
    }

    async fn run_worker(&self, handler: Arc<PythonHandler>) {
        let mut handler = handler.clone();
        loop {
            let result = handler.poll_once().await;
            match result {
                Some(exec_result) => {
                    let outcome = self.executor.execute(
                        handler.last_message().await.unwrap(), // TODO: fix this
                        exec_result
                    );
                    match outcome {
                        ExecutorOutcome::Ack => { /* ack tracking */ }
                        ExecutorOutcome::Retry { count, delay_ms } => {
                            sleep(Duration::from_millis(delay_ms)).await;
                        }
                        ExecutorOutcome::Rejected { .. } => { /* DLQ routing */ }
                    }
                }
                None => break, // receiver closed
            }
        }
    }
}
```

**Note:** `last_message()` on `PythonHandler` is a design issue — the handler consumes the message. Refactor to:

```rust
impl PythonHandler {
    /// Polls and processes a message, returning (OwnedMessage, ExecutionResult).
    /// This separates consumption from the callback result.
    pub async fn process_one(&mut self) -> Option<(OwnedMessage, ExecutionResult)> {
        let msg = self.receiver.recv().await?;
        let result = self.invoke(msg.clone()).await;
        Some((msg, result))
    }
}
```

## Data Flow

### Primary Flow: Message to ExecutionResult

```
1. ConsumerDispatcher::register_handler("events", 100, None)
       ↓
   returns mpsc::Receiver<OwnedMessage>

2. WorkerPool::run() spawns N workers

3. Each worker calls PythonHandler::process_one()
       ↓
   receiver.recv().await  ( Tokio mpsc, fair distribution via select! )

4. OwnedMessage → KafkaMessage (PyO3 boundary copy)
       ↓
5. tokio::task::spawn_blocking {
       pyo3::Python::with(|py| {
           callback.call1((py_msg,))
       })
   }
       ↓
6. ExecutionResult::Ok | Error | Rejected
       ↓
7. Executor::execute(msg, result) → ExecutorOutcome
       ↓
8. WorkerPool handles: Ack → track offset | Retry → re-enqueue | Rejected → DLQ
```

### Backpressure Flow

```
ConsumerDispatcher::send(msg) → try_send to bounded channel
       │
       ├── Ok → Message buffered in handler's queue
       │
       └── Err(TrySendError::Full) → DispatchError::Backpressure
                │
                └── ConsumerDispatcher sees Backpressure
                        ├── pause_partition() via rdkafka
                        └── Resume when queue_depth < threshold (0.5 * capacity)
```

This flow already exists in v1.1. The Python execution lane adds workers that poll from the handler's queue, draining it and preventing the queue from filling up (which would trigger backpressure).

### Channel Ownership

```
ConsumerDispatcher
    └── Dispatcher (owns mpsc::Sender half)
            └── QueueManager (stores sender + metadata per topic)
                    └── HandlerEntry { sender, metadata }

ConsumerDispatcher::register_handler() returns the Receiver half
    └── PythonHandler owns this Receiver
            └── WorkerPool workers poll from it
```

## Component Responsibilities

| Component | Responsibility | Location |
|-----------|----------------|----------|
| `ConsumerDispatcher` | Owns Runner+Dispatcher, dispatches messages, manages backpressure | `src/dispatcher/mod.rs` |
| `mpsc::Receiver<OwnedMessage>` | Per-handler message queue (bounded) | Returned by `register_handler()` |
| `PythonHandler` | Polls receiver, converts OwnedMessage→KafkaMessage, invokes Python callback | `src/python/handler.rs` |
| `WorkerPool` | Runs N workers, distributes work across receiver | `src/python/worker_pool.rs` |
| `Executor` | Policy: retry, commit, DLQ routing | `src/python/executor.rs` |
| `ExecutionResult` | Normalized outcome enum | `src/python/execution_result.rs` |

## Project Structure

```
src/
├── lib.rs                    # Module init, pyclass exports
├── consumer/                 # Pure Rust consumer core (PyO3-free)
│   ├── config.rs
│   ├── error.rs
│   ├── message.rs            # OwnedMessage
│   └── runner.rs             # ConsumerRunner, ConsumerStream
├── dispatcher/               # Pure Rust dispatcher (PyO3-free)
│   ├── mod.rs                # Dispatcher, ConsumerDispatcher
│   ├── queue_manager.rs      # QueueManager, HandlerMetadata
│   ├── backpressure.rs       # BackpressurePolicy, BackpressureAction
│   └── error.rs              # DispatchError
├── python/                   # Python execution lane (NEW)
│   ├── mod.rs
│   ├── execution_result.rs   # ExecutionResult enum
│   ├── executor.rs           # Executor trait + DefaultExecutor
│   ├── handler.rs            # PythonHandler
│   └── worker_pool.rs        # WorkerPool
├── kafka_message.rs          # PyO3 KafkaMessage (converts OwnedMessage)
├── pyconsumer.rs             # PyO3 Consumer pyclass
└── config.rs                 # PyO3 config structs
```

**Structure rationale:**
- `src/python/` is a new module for the execution lane — separates Python callback invocation from pure-Rust dispatcher logic
- `execution_result.rs` has no PyO3 dependencies — can be unit tested with plain Rust
- `executor.rs` is a pure Rust trait — no GIL needed, enables test doubles
- `handler.rs` bridges Rust async (`mpsc::Receiver`) with Python GIL (`spawn_blocking`)
- `worker_pool.rs` is async orchestration — depends on handler and executor

## Build Order

### Phase 1: Foundation (No dependencies)

**1a. `src/python/execution_result.rs`**
- Defines `ExecutionResult` enum
- No dependencies on any other new module
- Can be unit tested with plain Rust assertions

**1b. `src/python/executor.rs`**
- Defines `Executor` trait and `ExecutorOutcome`
- Depends only on `ExecutionResult` and `OwnedMessage` (already exists)
- Can be tested with a mock executor

### Phase 2: PythonHandler (Depends on Phase 1)

**`src/python/handler.rs`**
- Stores `mpsc::Receiver<OwnedMessage>` + `Py<PyAny>`
- `process_one()` → `(OwnedMessage, ExecutionResult)`
- Uses `spawn_blocking` for GIL-holding call
- Depends on `Executor` (trait, can use `DefaultExecutor` for now)

**Testing strategy:** Can be tested in isolation using a mock `Py<PyAny>` callback. No need for real Kafka or Python runtime — use `tox` environment or `mockall` for `Py<PyAny>`.

### Phase 3: WorkerPool (Depends on Phase 2)

**`src/python/worker_pool.rs`**
- Owns `Arc<PythonHandler>` + `Arc<dyn Executor>`
- `run()` spawns N Tokio tasks
- `select!` on receiver for graceful shutdown
- Implements backpressure-aware shutdown (stops accepting when signal received)

### Phase 4: Integration with ConsumerDispatcher

**`src/pyconsumer.rs` changes:**
- Replace direct `ConsumerRunner::stream()` polling with `ConsumerDispatcher`
- `add_handler(topic, callback)` → `ConsumerDispatcher::register_handler(topic, capacity, None)` + store `Py<PyAny>` in `PythonHandler`
- Wire `WorkerPool` to the returned receiver
- `start()` becomes: build config → create `ConsumerRunner` → wrap in `ConsumerDispatcher` → register handlers → run `WorkerPool`

### Phase 5: PyO3 Bridge (Depends on Phase 4)

**`src/python/mod.rs` exports + `src/lib.rs` updates:**
- Export `ExecutionResult`, `Executor`, `DefaultExecutor` to Python (if needed)
- Add `#[pyclass]` variants if Python needs to inspect execution state
- Update module initialization

## Anti-Patterns to Avoid

### 1. Calling Python on the Tokio Async Thread

**What people do:** Use `pyo3::Python::with()` directly inside an async task without `spawn_blocking`.

**Why it's wrong:** Python's GIL is not thread-safe with async task scheduling. Holding the GIL across an `.await` point blocks the entire Tokio runtime from making progress on other tasks.

**Do this instead:** Always use `spawn_blocking` to cross from async Tokio context to Python GIL context.

### 2. Storing `Py<PyAny>` in a Mutex in the Async Context

**What people do:** Store callbacks in `Mutex<HashMap<String, Py<PyAny>>>` and lock on every message.

**Why it's wrong:** `Mutex` introduces serial contention across workers and creates a bottleneck at the hottest path.

**Do this instead:** Use `Arc<PythonHandler>` per handler — each handler owns its receiver and callback. Workers clone the `Arc` (cheap) and process independently.

### 3. Unbounded Receiver Loop Without Backpressure Awareness

**What people do:** `while let Some(msg) = receiver.recv().await { ... }` without checking queue depth.

**Why it's wrong:** If Python callbacks are slow (I/O bound, network), the queue fills up, causing `Dispatcher::send()` to return `Backpressure`, triggering `pause_partition()` on rdkafka. This creates a deadlock: consumer is paused, workers are still processing old messages slowly.

**Do this instead:** WorkerPool should track in-flight count and pause polling when inflight exceeds a threshold (e.g., `capacity * 0.8`). Use `HandlerMetadata::get_inflight()` to inspect.

### 4. Mapping PyErr to String Without Preserving Type Information

**What people do:** `e.stringify(py).unwrap_or("unknown".to_string())` loses the Python exception class name.

**Why it's wrong:** `"TypeError"` and `"ValueError"` are both just "exception occurred" — harder to debug in production.

**Do this instead:** Extract exception type and message separately:

```rust
Err(e) => {
    let exc_type = e.get_type().name().to_string();
    let exc_msg = e.stringify(py).unwrap_or_else(|_| "unknown".to_string());
    ExecutionResult::Error {
        exception: format!("{}: {}", exc_type, exc_msg),
        traceback: String::new(),
    }
}
```

## Integration Points

### With ConsumerDispatcher

**Registration:**
```rust
// In PyConsumer::add_handler()
let receiver = consumer_dispatcher.register_handler(topic.clone(), capacity, max_concurrency);
let handler = Arc::new(PythonHandler::new(receiver, callback));
let worker_pool = WorkerPool::new(num_workers, handler, Arc::new(DefaultExecutor));
```

**Shutdown:** When `stop()` is called on `PyConsumer`, signal `WorkerPool` to stop polling gracefully. The `mpsc::Receiver` being dropped causes workers to exit their loops cleanly.

### With KafkaMessage (PyO3 Boundary)

`OwnedMessage` → `KafkaMessage` conversion happens inside `PythonHandler::invoke()` via `KafkaMessage::from(msg)`. This copy is necessary because:
1. `KafkaMessage` is a `#[pyclass]` — Python owns the reference
2. `OwnedMessage` is Rust-owned and must not contain Python references (Send+Sync requirement)
3. The conversion is a single `From` implementation already in `kafka_message.rs`

### With Existing PyConsumer

The current `pyconsumer.rs` directly uses `ConsumerRunner::stream()`. After v1.2, it should use `ConsumerDispatcher` which internally uses the same stream but adds dispatcher routing. The change is additive — the dispatcher wraps the runner:

```rust
// Before
let mut stream = runner.stream();
while let Some(result) = stream.next().await { /* direct dispatch */ }

// After
let dispatcher = ConsumerDispatcher::new(runner);
let receiver = dispatcher.register_handler(topic, capacity, concurrency);
let pool = WorkerPool::new(workers, handler, executor);
pool.run().await;
```

## Scaling Considerations

| Scale | Concern | Approach |
|-------|---------|----------|
| 1-10 workers | GIL contention | Keep workers ≤ 4; Python GIL serializes callbacks |
| 10-100 workers | Handler queue depth | Increase capacity; monitor queue_depth via `HandlerMetadata` |
| 100+ workers | Partition distribution | Each topic-partition has own queue — no cross-partition contention |

**GIL is the hard limit.** Multiple workers only help when Python callbacks release the GIL (e.g., I/O operations via `aiofiles`, external API calls). For CPU-bound Python callbacks, a single worker is optimal.

**Concurrency configuration:**
```rust
// Per-handler semaphore limits concurrent Python executions
let semaphore = Arc::new(tokio::sync::Semaphore::new(max_concurrent));
let receiver = dispatcher.register_handler_with_semaphore(topic, capacity, Some(semaphore));
```

This is already wired through `ConsumerDispatcher::register_handler()` — the Python layer just needs to propagate the `max_concurrent` parameter.

## Sources

- PyO3 GIL management: `pyo3-async-runtimes` + `spawn_blocking` (observed in `src/pyconsumer.rs`)
- Tokio mpsc channel: `tokio::sync::mpsc` (used in `src/dispatcher/mod.rs`)
- Executor pattern: service layer idiom from `common/patterns.md` (Rust adaptation)
- OwnedMessage: `src/consumer/message.rs` (already Send+Sync)

---
*Architecture research for: Python execution lane*
*Researched: 2026-04-16*