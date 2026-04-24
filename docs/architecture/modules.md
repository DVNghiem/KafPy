# Module Reference

Comprehensive breakdown of all modules in `src/`.

## Public PyO3 API

### `src/lib.rs` — Module Root

**Responsibilities:**
- Initialize logging via `Logger::init()`
- Register all `#[pyclass]` types with Python module
- Export public API surface

**Public API:**
```rust
#[pymodule]
fn _kafpy(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<KafkaMessage>()?;
    m.add_class::<PyConsumer>()?;
    m.add_class::<PyProducer>()?;
    m.add_class::<config::ConsumerConfig>()?;
    m.add_class::<config::ProducerConfig>()?;
    Ok(())
}
```

**Key Files:**
- `src/config.rs` — `ConsumerConfig`, `ProducerConfig` (#[pymodule-exposed])

---

## Pure Rust Consumer Core

### `src/consumer/` — Kafka Consumer Implementation

**Responsibilities:**
- Kafka consumer lifecycle management
- Message streaming via rdkafka `StreamConsumer`
- Custom rebalance callback handling

**Files:**
| File | Purpose |
|------|---------|
| `mod.rs` | Module exports |
| `config.rs` | `ConsumerConfig` internal types |
| `error.rs` | Consumer-specific errors |
| `message.rs` | `OwnedMessage` type |
| `runner.rs` | `ConsumerRunner`, `ConsumerStream`, `ConsumerTask` |

**Key Types:**
```rust
// consumer/runner.rs
pub struct ConsumerRunner {
    consumer: Arc<StreamConsumer<CustomConsumerContext>>,
    dispatcher: ConsumerDispatcher,
    shutdown_token: CancellationToken,
}

pub struct ConsumerStream {
    stream: KafkaStream,
    runner: Arc<ConsumerRunner>,
}

pub struct ConsumerTask {
    runner: Arc<ConsumerRunner>,
}
```

---

### `src/dispatcher/` — Message Dispatcher

**Responsibilities:**
- Route `OwnedMessage` to per-handler bounded Tokio mpsc channels
- Track queue depth and inflight messages per handler
- Apply backpressure when queues are full

**Files:**
| File | Purpose |
|------|---------|
| `mod.rs` | Module exports, `Dispatcher` struct |
| `consumer_dispatcher.rs` | `ConsumerDispatcher` wiring consumer→dispatcher |
| `queue_manager.rs` | `QueueManager` tracking queue depth |
| `backpressure.rs` | `BackpressurePolicy` trait and implementations |
| `error.rs` | Dispatch-specific errors |

**Key Types:**
```rust
// dispatcher/mod.rs
pub struct Dispatcher {
    queues: Arc<HashMap<HandlerId, mpsc::Sender<OwnedMessage>>>,
    queue_manager: Arc<QueueManager>,
    backpressure: Arc<dyn BackpressurePolicy>,
}

// dispatcher/backpressure.rs
pub enum BackpressureAction {
    Drop,           // Reject immediately
    Wait,           // Block until capacity
    FuturePausePartition,  // Pause specific partition
}

pub trait BackpressurePolicy: Send + Sync {
    fn action(&self, handler: &HandlerId, depth: usize) -> BackpressureAction;
}
```

---

### `src/worker_pool/` — Worker Pool

**Responsibilities:**
- N Tokio tasks polling handler queues
- Invoke Python callbacks via `spawn_blocking`
- Batch accumulation for batch handler modes

**Files:**
| File | Purpose |
|------|---------|
| `mod.rs` | Module exports, `WorkerPool` struct |
| `worker.rs` | `worker_loop()` — single-sync handler execution |
| `batch_loop.rs` | `batch_worker_loop()` — batch handler execution |
| `pool.rs` | `WorkerPool` orchestration |
| `state.rs` | `WorkerState`, `BatchState` enums |
| `accumulator.rs` | `PartitionAccumulator` for batch buffering |

**Key Types:**
```rust
// worker_pool/state.rs
pub enum WorkerState {
    Idle,
    Processing { message: OwnedMessage },
    Retrying { message: OwnedMessage, attempt: u32 },
    WaitingForAck { offset: i64 },
}

pub enum BatchState {
    Idle,
    Accumulating { messages: Vec<OwnedMessage>, partition: i32 },
    Flushing { messages: Vec<OwnedMessage>, partition: i32 },
    WaitingForAck { offsets: Vec<i64> },
}
```

---

### `src/routing/` — Message Routing

**Responsibilities:**
- Route messages to handlers based on topic, headers, or keys
- Support pattern-based routing (regex on topic name)
- Python callback router for dynamic routing

**Files:**
| File | Purpose |
|------|---------|
| `mod.rs` | Module exports |
| `chain.rs` | `RoutingChain` — ordered router list |
| `context.rs` | `RoutingContext`, `HandlerId` newtype |
| `decision.rs` | `RoutingDecision` enum |
| `router.rs` | `Router` trait |
| `topic_pattern.rs` | `TopicPatternRouter` |
| `header.rs` | `HeaderRouter` |
| `key.rs` | `KeyRouter` |
| `python_router.rs` | `PythonRouter` for dynamic routing |
| `config.rs` | Routing configuration |

**Key Types:**
```rust
// routing/context.rs
pub struct HandlerId(String);

impl HandlerId {
    pub fn new(id: String) -> Self;
    pub fn as_str(&self) -> &str;
}

// routing/decision.rs
pub enum RoutingDecision {
    Route(HandlerId),      // Route to specific handler
    Drop,                  // Fast-path: drop + advance offset
    Reject,                // Fast-path: direct to DLQ
    Defer,                 // Routing inconclusive → default handler
}
```

---

## Python Integration Layer

### `src/python/` — Python Handler Execution

**Responsibilities:**
- Store Python callbacks (`Py<PyAny>`)
- Execute handlers via `spawn_blocking` (GIL management)
- Batch execution support

**Files:**
| File | Purpose |
|------|---------|
| `mod.rs` | Module exports |
| `handler.rs` | `PythonHandler`, `HandlerConfig` |
| `executor.rs` | `Executor` trait |
| `context.rs` | `ExecutionContext` for handlers |
| `execution_result.rs` | `ExecutionResult`, `ExecutionAction` |
| `batch.rs` | `BatchAccumulator`, `BatchExecutionResult` |
| `async_bridge.rs` | `PythonAsyncFuture` for async handlers |

---

### `src/runtime/` — Runtime Assembly

**Responsibilities:**
- `RuntimeBuilder` for composing consumer runtime
- Wire all components together (config → tracker → dispatcher → handler → worker → committer)

**Files:**
```rust
// runtime/builder.rs
pub struct RuntimeBuilder {
    config: ConsumerConfig,
    handlers: Vec<PythonHandler>,
    routing_chain: RoutingChain,
    offset_coordinator: Arc<dyn OffsetCoordinator>,
    // ...
}

impl RuntimeBuilder {
    pub fn build(self) -> Result<ConsumerRuntime, BuildError>;
}
```

---

## Internal Modules

### `src/offset/` — Offset Tracking

**Responsibilities:**
- Per-topic-partition ack tracking
- Highest contiguous offset calculation
- `store_offset()` + `commit()` coordination

**Files:** `offset_tracker.rs`, `offset_coordinator.rs`, `commit_task.rs`, `mod.rs`

---

### `src/shutdown/` — Shutdown Coordination

**Responsibilities:**
- 4-phase graceful shutdown lifecycle
- `ShutdownPhase` enum (replaces boolean flags)

**Files:** `shutdown.rs`, `mod.rs`

---

### `src/retry/` — Retry Scheduling

**Responsibilities:**
- `RetryPolicy` with exponential backoff + jitter
- `RetryCoordinator` tracking per-message retry state

**Files:** `policy.rs`, `retry_coordinator.rs`, `mod.rs`

---

### `src/dlq/` — Dead Letter Queue

**Responsibilities:**
- `DlqRouter` trait and `DefaultDlqRouter`
- `DlqMetadata` envelope (7 fields)
- Fire-and-forget produce to DLQ topic

**Files:** `router.rs`, `metadata.rs`, `produce.rs`, `mod.rs`

---

### `src/failure/` — Failure Classification

**Responsibilities:**
- `FailureReason` taxonomy
- `FailureCategory` classification
- `FailureClassifier` for retry/DLQ decisions

**Files:** `reason.rs`, `classifier.rs`, `logging.rs`, `mod.rs`

---

### `src/observability/` — Metrics & Tracing

**Responsibilities:**
- `MetricsSink` trait + `PrometheusMetricsSink` adapter
- OTLP tracing with W3C tracecontext propagation
- `RuntimeSnapshot` for zero-cost introspection

**Files:** `metrics.rs`, `tracing.rs`, `runtime_snapshot.rs`, `config.rs`, `mod.rs`

---

### `src/error.rs` — Unified Error Re-exports

**Purpose:** Single import point for all error types

```rust
pub use errors::{DispatchError, ConsumerError, CoordinatorError, PyError};
```