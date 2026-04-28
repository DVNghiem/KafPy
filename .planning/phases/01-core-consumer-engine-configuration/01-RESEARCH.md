# Phase 1 Research: Core Consumer Engine & Configuration

**Date:** 2026-04-28
**Phase:** 01-core-consumer-engine-configuration
**Requirements:** CORE-01, CORE-02, CORE-03, CORE-04, CORE-05, CORE-07, MSG-01, MSG-02, MSG-04, MSG-05, OFF-01, OFF-02, OFF-03, CONF-01, CONF-02, CONF-04

---

## 1. rdkafka Consumer Setup and Configuration Patterns

### Version and Build

- **rdkafka 0.39.0** with `cmake-build` and `ssl` features (STACK.md)
- librdkafka v1.9.2+ compatible
- MSRV: Rust 1.70+

### Consumer Creation Pattern

```rust
// src/consumer/runner.rs — ConsumerRunner::new()
let consumer: StreamConsumer = config
    .clone()
    .into_rdkafka_config()  // Converts ConsumerConfig → rdkafka ClientConfig
    .create()
    .map_err(ConsumerError::Kafka)?;

consumer.subscribe(&config.topics.iter().map(|s| s.as_str()).collect::<Vec<_>>())
    .map_err(|e| ConsumerError::Subscription(e.to_string()))?;
```

### Topic Subscription

- Subscription accepts `&[&str]` of topic names and regex patterns
- Regex patterns supported natively by rdkafka (CORE-03)
- The `into_rdkafka_config()` method sets all rdkafka properties (CONF-02)

### Key rdkafka Properties for Phase 1

```rust
// src/consumer/config.rs — ConsumerConfig::into_rdkafka_config()
cfg.set("bootstrap.servers", &self.brokers)
    .set("group.id", &self.group_id)
    .set("auto.offset.reset", self.auto_offset_reset.as_str())
    .set("enable.auto.commit", self.enable_auto_commit.to_string())
    .set("enable.auto.offset.store", self.enable_auto_offset_store.to_string())
    .set("session.timeout.ms", self.session_timeout_ms.to_string())
    .set("heartbeat.interval.ms", self.heartbeat_interval_ms.to_string())
    .set("max.poll.interval.ms", self.max_poll_interval_ms.to_string())
    // Memory-bound defaults (PITFALLS-2.1: prevent unbounded queue growth)
    .set("queued.max.messages.kbytes", "65536")  // 64MB per partition
    .set("queued.min.messages", "100000")
    .set("partition.assignment.strategy", self.partition_assignment_strategy.as_str())
```

### Auto Offset Commit (CORE-05)

- `enable_auto_commit = false` for manual commit mode (default in builder)
- `enable_auto_commit = true` for auto fallback
- Auto commit happens via `commit_consumer_state(Async)` in `ConsumerRunner::commit()`

### Manual Offset Commit Pattern (CORE-04, OFF-03)

```rust
// Two-phase manual commit (OFF-03):
// Phase 1: store_offset — fast, in-memory
pub async fn store_offset(&self, topic: &str, partition: i32, offset: i64) {
    tokio::task::spawn_blocking(move || {
        consumer.store_offset(&topic, partition, offset)
    }).await
}
// Phase 2: commit — network round-trip
pub fn commit(&self) -> Result<(), ConsumerError> {
    self.consumer.commit_consumer_state(rdkafka::consumer::CommitMode::Async)
}
```

### Graceful Start/Stop (CORE-07)

```rust
// ConsumerRunner uses broadcast channel for shutdown signaling
let (shutdown_tx, _) = broadcast::channel(1);
pub fn stop(&self) {
    let _ = self.shutdown_tx.send(());
}
```

Consumer loop uses `tokio::select!` with biased branch on shutdown:

```rust
loop {
    select! {
        biased;
        _ = shutdown_rx.recv() => { break; }
        message_result = consumer.recv() => { /* handle */ }
    }
}
```

---

## 2. Bounded mpsc Channel Architecture for Handler Queues

### QueueManager Design

`src/dispatcher/queue_manager.rs` owns all per-handler bounded channels:

```rust
pub struct QueueManager {
    pub(crate) handlers: Arc<parking_lot::Mutex<HashMap<String, HandlerEntry>>>,
}

pub struct HandlerEntry {
    pub sender: mpsc::Sender<OwnedMessage>,
    pub(crate) metadata: HandlerMetadata,
}
```

### Registration

```rust
// src/dispatcher/queue_manager.rs — register_handler_with_semaphore()
pub(crate) fn register_handler_with_semaphore(
    &self,
    topic: impl Into<String>,
    capacity: usize,
    semaphore: Option<Arc<Semaphore>>,
) -> mpsc::Receiver<OwnedMessage> {
    let (tx, rx) = mpsc::channel(capacity);  // Bounded = backpressure
    // metadata tracks queue_depth, inflight, semaphore for concurrency
}
```

### Send with try_send (MSG-04)

```rust
// src/dispatcher/queue_manager.rs — send_to_handler_by_id()
match entry.sender.try_send(message) {
    Ok(()) => {
        entry.metadata.inc_queue_depth();   // Buffered
        entry.metadata.inc_inflight();      // Dispatched
        Ok(DispatchOutcome { topic, partition, offset, queue_depth })
    }
    Err(TrySendError::Full(_)) => Err(DispatchError::QueueFull(...))
    Err(TrySendError::Closed(_)) => Err(DispatchError::QueueClosed(...))
}
```

### Why parking_lot::Mutex

`parking_lot::Mutex` used instead of `tokio::sync::Mutex` because:
- Registration happens in non-async context
- Lock is held briefly, not across await points
- No async I/O inside critical section

### HandlerMetadata Atomic Counters

```rust
pub struct HandlerMetadata {
    pub capacity: usize,
    pub(crate) queue_depth: Arc<AtomicUsize>,   // Buffered messages
    pub(crate) inflight: Arc<AtomicUsize>,       // Dispatched, not acked
    pub(crate) semaphore: Option<Arc<Semaphore>>, // Concurrency limit
    outstanding_permits: AtomicUsize,
    semaphore_limit: usize,
}
```

`try_acquire_semaphore()` used at dispatch time (non-blocking):

```rust
pub fn try_acquire_semaphore(&self) -> bool {
    if self.semaphore.is_none() { return true; }
    let current = self.outstanding_permits.load(Ordering::Relaxed);
    if current >= self.semaphore_limit { return false; }
    self.outstanding_permits.fetch_add(1, Ordering::Relaxed);
    true
}
```

---

## 3. Offset Tracking with BTreeSet (Highest Contiguous Offset)

### Algorithm (OFF-01)

From `src/offset/offset_tracker.rs`:

```rust
// src/offset/offset_tracker.rs — PartitionState
pub struct PartitionState {
    pub committed_offset: i64,        // Starts at -1
    pub pending_offsets: BTreeSet<i64>,  // Buffered for out-of-order
    pub failed_offsets: BTreeSet<i64>,
    pub has_terminal: bool,          // Blocks commit
}

pub fn ack(&mut self, offset: i64) {
    self.pending_offsets.insert(offset);
    self.failed_offsets.remove(&offset);
    // Advance contiguous cursor
    loop {
        let next = self.committed_offset + 1;
        if self.pending_offsets.remove(&next) {
            self.committed_offset = next;
        } else {
            break;
        }
    }
}
```

### Highest-Contiguous-Offset Commit (OFF-02)

```rust
// src/offset/offset_tracker.rs — should_commit()
pub fn should_commit(&self, topic: &str, partition: i32) -> bool {
    guard.get(&key).is_some_and(|s| {
        if s.has_terminal { return false; }  // D-01: terminal blocks
        !s.pending_offsets.is_empty() || s.committed_offset >= 0
    })
}
```

### Terminal Failure Blocking (OFF-04, Phase 2)

`has_terminal` flag is set once and never cleared (idempotent):
```rust
if reason.category() == FailureCategory::Terminal {
    state.has_terminal = true;  // D-03: set once, never clear
}
```

### TopicPartitionKey Type Safety

```rust
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
struct TopicPartitionKey(String, i32);

impl TopicPartitionKey {
    fn new(topic: impl Into<String>, partition: i32) -> Self {
        Self(topic.into(), partition)
    }
}
```

### Partition-Aware (OFF-01)

Each partition has independent `PartitionState`:
```rust
partitions: Mutex<HashMap<TopicPartitionKey, PartitionState>>
```

If partition A has a terminal failure, partition B is unaffected.

---

## 4. BackpressurePolicy Trait Design (CONF-04)

From `src/dispatcher/backpressure.rs`:

```rust
// src/dispatcher/backpressure.rs
pub enum BackpressureAction {
    Drop,                       // Discard message — fire-and-forget
    Wait,                       // Block briefly (currently maps to error)
    FuturePausePartition(String), // Signal pause for future implementation
}

pub trait BackpressurePolicy: Send + Sync {
    fn on_queue_full(&self, topic: &str, handler: &HandlerMetadata) -> BackpressureAction;
}

// Default: drop on full
pub struct DefaultBackpressurePolicy;
impl BackpressurePolicy for DefaultBackpressurePolicy {
    fn on_queue_full(&self, _topic: &str, _handler: &HandlerMetadata) -> BackpressureAction {
        BackpressureAction::Drop
    }
}

// Pause on full (for critical data)
pub struct PauseOnFullPolicy;
impl BackpressurePolicy for PauseOnFullPolicy {
    fn on_queue_full(&self, topic: &str, _handler: &HandlerMetadata) -> BackpressureAction {
        BackpressureAction::FuturePausePartition(topic.to_string())
    }
}
```

### Usage in Dispatcher

When `send_to_handler_by_id` returns `DispatchError::QueueFull`:
```rust
let action = backpressure_policy.on_queue_full(&topic, &handler_metadata);
match action {
    BackpressureAction::Drop => { /* discard, ack */ }
    BackpressureAction::Wait => { /* retry send after delay */ }
    BackpressureAction::FuturePausePartition(t) => { /* signal pause */ }
}
```

---

## 5. ConsumerConfig Python Dataclass Design (CONF-01)

From `kafpy/config.py`:

```python
@dataclass(frozen=True)
class ConsumerConfig:
    bootstrap_servers: str
    group_id: str
    topics: list[str]
    auto_offset_reset: str = "earliest"
    enable_auto_commit: bool = False
    session_timeout_ms: int = 30000
    heartbeat_interval_ms: int = 3000
    max_poll_interval_ms: int = 300000
    security_protocol: str | None = None
    sasl_mechanism: str | None = None
    sasl_username: str | None = None
    sasl_password: str | None = None
    fetch_min_bytes: int = 1
    max_partition_fetch_bytes: int = 1048576
    partition_assignment_strategy: str = "roundrobin"
    retry_backoff_ms: int = 100
    message_batch_size: int = 100
    retry_policy: RetryConfig | None = None
    dlq_topic_prefix: str | None = None
    drain_timeout_secs: int | None = None
    num_workers: int | None = None
    enable_auto_offset_store: bool | None = None
    observability_config: ObservabilityConfig | None = None
    handler_timeout_ms: int | None = None
```

### `to_rust()` Conversion

```python
def to_rust(self) -> object:
    import kafpy._kafpy as _kafpy
    return _kafpy.ConsumerConfig(
        brokers=self.bootstrap_servers,
        group_id=self.group_id,
        topics=self.topics,
        # ... all fields mapped
    )
```

### Validation in `__post_init__`

```python
def __post_init__(self) -> None:
    if self.auto_offset_reset not in AUTO_OFFSET_RESET_VALUES:
        raise ValueError(...)
    if self.num_workers is not None and self.num_workers <= 0:
        raise ValueError(...)
```

### Rust ConsumerConfig (CONF-02)

From `src/consumer/config.rs`:

```rust
pub struct ConsumerConfig {
    pub brokers: String,
    pub group_id: String,
    pub topics: Vec<String>,
    pub auto_offset_reset: AutoOffsetReset,
    pub enable_auto_commit: bool,
    pub enable_auto_offset_store: bool,
    pub session_timeout_ms: u32,
    pub heartbeat_interval_ms: u32,
    pub max_poll_interval_ms: u32,
    pub security_protocol: Option<String>,
    pub sasl_mechanism: Option<String>,
    pub sasl_username: Option<String>,
    pub sasl_password: Option<String>,
    pub fetch_min_bytes: i32,
    pub max_partition_fetch_bytes: i32,
    pub partition_assignment_strategy: PartitionAssignmentStrategy,
    pub retry_backoff_ms: u32,
    pub default_retry_policy: RetryPolicy,
    pub dlq_topic_prefix: String,
    pub drain_timeout_secs: u64,
    pub routing_rules: Vec<RoutingRule>,
}
```

`into_rdkafka_config()` maps all fields to rdkafka string properties.

---

## 6. Key Pitfalls to Avoid (from PITFALLS.md)

### GIL + Tokio Blocking (PITFALLS-1.1) — Critical

**Problem:** `Python::attach` called inside `tokio::spawn` blocks Tokio worker threads.

**Fix in Phase 1:** All Python callbacks go through `spawn_blocking`:
```rust
tokio::task::spawn_blocking(move || {
    Python::attach(|py| { /* Python callback here */ })
})
```

### Unbounded Queue Growth (PITFALLS-2.1) — High

**Problem:** `queued.max.messages.kbytes` default is 1GB per partition → OOM on backlog.

**Fix in Phase 1:** Set `queued.max.messages.kbytes` to 65536 (64MB) in `into_rdkafka_config()`:
```rust
.set("queued.max.messages.kbytes", "65536")
```

### Offset Commit Reordering (PITFALLS-4.2) — High

**Problem:** Commit requests from multiple threads can reorder across partitions.

**Fix in Phase 1:** Commit only via `ConsumerRunner::commit()` which serializes through the runner's single threaded context.

### Off-by-One on Seek (PITFALLS-4.1) — High

**Problem:** After cache expiration, consumer seeks to last consumed offset instead of offset + 1.

**Fix in Phase 1:** Always seek to `lastConsumedOffset + 1` (documented for Phase 2 rebalance handler).

### Rebalance-in-progress Commit (PITFALLS-4.3)

**Problem:** Calling commit during rebalance callback causes `ILLEGAL_GENERATION`.

**Fix in Phase 2:** Never commit in rebalance callback for EAGER strategies. For Phase 1 (no rebalance handler yet), not applicable but design ensures manual commit only in consume loop.

### Unbounded Memory Without Backpressure (PITFALLS-2.1)

**Problem:** No backpressure when handler queues are full.

**Fix in Phase 1:** Bounded mpsc channels with `try_send` + `BackpressurePolicy` consulted on Full.

---

## 7. Concrete Module Structure and File Layout

### Phase 1 Modules (Pure Rust Core, PyO3-free where possible)

```
src/
├── lib.rs                          # Module declarations, PyO3 module entry
├── config.rs                       # Rust ConsumerConfig (mirrors Python dataclass)
├── consumer/
│   ├── mod.rs                       # Re-exports ConsumerRunner, OwnedMessage
│   ├── config.rs                    # ConsumerConfig struct + ConsumerConfigBuilder + rdkafka mapping (CONF-01, CONF-02)
│   ├── runner.rs                    # ConsumerRunner (CORE-01, CORE-07)
│   ├── message.rs                   # OwnedMessage, MessageTimestamp, MessageRef (MSG-01, MSG-02)
│   └── error.rs                     # ConsumerError
├── dispatcher/
│   ├── mod.rs                       # Re-exports Dispatcher, DispatchOutcome, DispatchError
│   ├── queue_manager.rs             # QueueManager, HandlerMetadata, HandlerEntry (MSG-04)
│   ├── backpressure.rs              # BackpressureAction, BackpressurePolicy trait (CONF-04)
│   ├── consumer_dispatcher.rs       # Dispatcher: routes OwnedMessage → handler queues
│   └── error.rs                     # DispatchError
├── worker_pool/
│   ├── mod.rs                       # Re-exports WorkerPool
│   ├── pool.rs                      # WorkerPool struct + run/shutdown (MSG-05)
│   ├── worker.rs                    # worker_loop (single message processing)
│   ├── batch_loop.rs                # batch_worker_loop (batch processing)
│   ├── state.rs                     # WorkerState, BatchState enums
│   └── accumulator.rs               # Batch accumulator per handler
├── offset/
│   ├── mod.rs                       # Re-exports OffsetTracker, OffsetCoordinator
│   ├── offset_tracker.rs            # PartitionState, OffsetTracker (OFF-01, OFF-02)
│   ├── offset_coordinator.rs        # OffsetCoordinator trait impl
│   └── commit_task.rs               # Dedicated commit task (OFF-03 pattern)
├── python/
│   ├── mod.rs                       # Re-exports PythonHandler, ExecutionContext
│   ├── handler.rs                   # PythonHandler with spawn_blocking (PY-02)
│   ├── context.rs                   # ExecutionContext (PY-04)
│   ├── executor.rs                  # Executor trait (PY-02, PY-03)
│   ├── async_bridge.rs              # PythonAsyncFuture for async handlers (PY-03)
│   ├── execution_result.rs          # ExecutionResult, HandlerResult
│   ├── batch.rs                     # Batch handler support
│   └── logger.rs                    # Python logging bridge
├── routing/
│   ├── mod.rs                       # Re-exports Router, RoutingChain
│   ├── router.rs                    # Router trait
│   ├── chain.rs                     # RoutingChain (topic → handler routing)
│   ├── topic_pattern.rs             # TopicPattern router
│   ├── key.rs                       # KeyRouter
│   ├── header.rs                    # HeaderRouter
│   ├── python_router.rs             # Python-driven routing (PY-03)
│   ├── context.rs                   # RoutingContext, HandlerId
│   ├── decision.rs                  # RoutingDecision
│   └── config.rs                    # RoutingRule, RoutingConfig
├── retry/
│   ├── mod.rs                       # Re-exports RetryPolicy, RetrySchedule
│   ├── policy.rs                    # RetryPolicy struct (FAIL-03, FAIL-04)
│   └── retry_coordinator.rs         # RetryCoordinator state machine
├── dlq/
│   ├── mod.rs                       # Re-exports DlqRouter, DlqMetadata
│   ├── router.rs                    # DlqRouter trait, DefaultDlqRouter (FAIL-07)
│   ├── metadata.rs                  # DlqMetadata envelope (FAIL-06)
│   └── produce.rs                   # DlqProducer (FAIL-05)
├── failure/
│   ├── mod.rs                       # Re-exports FailureCategory, FailureReason
│   ├── reason.rs                    # FailureReason enum (FAIL-01)
│   ├── classifier.rs                # FailureClassifier trait, DefaultFailureClassifier (FAIL-02)
│   └── logging.rs                   # Failure logging
├── coordinator/
│   ├── mod.rs                       # Re-exports RetryCoordinator, OffsetCoordinator, ShutdownCoordinator
│   ├── offset_coordinator.rs        # OffsetCoordinator impl (delegates to OffsetTracker)
│   ├── retry_coordinator.rs         # RetryCoordinator impl
│   ├── shutdown.rs                  # ShutdownCoordinator, 4-phase lifecycle
│   └── error.rs                     # CoordinatorError
├── observability/
│   ├── mod.rs                       # Re-exports MetricsSink, TracingSink
│   ├── metrics.rs                   # MetricsSink, QueueSnapshot, HandlerMetrics
│   ├── tracing.rs                   # TracingSink, W3C trace propagation (OBS-01, OBS-02)
│   ├── config.rs                    # ObservabilityConfig
│   └── runtime_snapshot.rs          # RuntimeSnapshot, WorkerPoolState (OBS-06)
├── kafka_message.rs                 # KafkaMessage PyO3 wrapper
├── pyconsumer.rs                   # PyConsumer (PyO3-exposed consumer)
├── pyconfig.rs                     # PyRetryPolicy, PyObservabilityConfig, PyFailureCategory, PyFailureReason
├── produce.rs                      # PyProducer
├── error.rs                        # Top-level error definitions
└── logging.rs                      # Logger initialization
```

### Python Package Layout

```
kafpy/
├── __init__.py          # Public API exports
├── config.py            # ConsumerConfig, RetryConfig, ObservabilityConfig, RoutingConfig (CONF-01)
├── consumer.py          # Consumer class, @handler decorator (PY-01, PY-02, PY-03)
├── handlers.py          # Handler registration utilities
├── runtime.py           # Runtime startup, RuntimeBuilder wiring
└── exceptions.py        # KafPyException, HandlerException, DLQException
```

---

## 8. Implementation Notes for Phase 1 Requirements

### CORE-01: Rust-based Kafka consumer engine

`ConsumerRunner` owns the rdkafka `StreamConsumer` and drives the message loop. Conversion to `OwnedMessage` at consumer boundary ensures no lifetime aliasing.

### CORE-02: Consumer groups

`group.id` set in `into_rdkafka_config()`. rdkafka handles partition assignment automatically.

### CORE-03: Topic subscription by name and regex

`consumer.subscribe(&topics)` accepts regex patterns directly (rdkafka built-in support).

### CORE-04: Manual offset commit

`enable_auto_commit = false` + explicit `store_offset` + `commit()` calls.

### CORE-05: Auto offset commit fallback

`enable_auto_commit = true` in config; rdkafka commits automatically.

### CORE-07: Graceful start/stop

`broadcast::channel(1)` + `tokio::select!` with biased shutdown branch. `stop()` signals broadcast, loop exits cleanly.

### MSG-01: Message deserialization

`OwnedMessage::from_borrowed()` converts rdkafka `BorrowedMessage` to owned format. Payload/key stored as `Option<Vec<u8>>`. Custom decoder support via trait (deferred to routing module).

### MSG-02: Headers, timestamp access

`OwnedMessage` exposes all metadata fields directly. `MessageTimestamp` enum with `CreateTime`, `LogAppendTime`, `NotAvailable` variants. `as_millis()` and `as_system_time()` helpers.

### MSG-04: Per-handler bounded queues

`QueueManager::register_handler_with_semaphore()` creates `mpsc::channel(capacity)`. Each handler has independent queue.

### MSG-05: Backpressure propagation

`BackpressurePolicy::on_queue_full()` called when `try_send` fails. `BackpressureAction` variants: `Drop`, `Wait`, `FuturePausePartition`.

### OFF-01: Partition-aware offset tracking

`TopicPartitionKey(String, i32)` for type-safe partition identification. `BTreeSet<i64>` for out-of-order buffering.

### OFF-02: Contiguous offset commit

`ack()` advances `committed_offset` cursor only when next offset is present. `should_commit()` returns true when contiguous cursor can advance.

### OFF-03: Explicit store_offset + commit

Two-phase pattern: `store_offset()` (in-memory) then `commit()` (network). Required for at-least-once semantics.

### CONF-01: ConsumerConfig dataclass

Python `dataclass(frozen=True)` with validation. `to_rust()` converts to Rust `ConsumerConfig`.

### CONF-02: Kafka client config mapping

`ConsumerConfig::into_rdkafka_config()` converts all fields to rdkafka string properties.

### CONF-04: BackpressurePolicy trait

`trait BackpressurePolicy: Send + Sync` with `on_queue_full()` method. `DefaultBackpressurePolicy` (Drop) and `PauseOnFullPolicy` built in.

---

## 9. What's Already Implemented vs. What Needs Work

Based on codebase survey:

### Fully Implemented (ready for Phase 1 verification)
- `ConsumerRunner` with stream-based message loop
- `ConsumerConfig` + `ConsumerConfigBuilder` with rdkafka mapping
- `OwnedMessage` conversion from `BorrowedMessage`
- `QueueManager` with bounded channels and atomic counters
- `BackpressurePolicy` trait with `Drop`/`Wait`/`FuturePausePartition`
- `OffsetTracker` with BTreeSet algorithm and highest-contiguous-offset
- `WorkerPool` with `spawn_blocking` for Python handlers
- Python `ConsumerConfig` dataclass with `to_rust()`
- `PyConsumer` with `add_handler` and `start`/`stop`

### Needs Verification / Polish
- Message deserialization (JSON, msgpack) — `OwnedMessage` stores raw bytes; custom decoder interface exists but needs verification
- Handler timeout enforcement — `handler_timeout_ms` field exists in config but worker loop timeout behavior not verified
- Batch handler support — `batch_loop.rs` and `batch.rs` exist but Phase 1 is single-handler (batch deferred to Phase 3/4)
- Topic regex subscription — subscribe accepts topic list but regex handling needs verification
- Commit task serialization — dedicated commit task pattern noted but `store_offset` currently spawns blocking task per call

### Deferred to Phase 2
- Rebalance listener (`on_partitions_revoked`/`on_partitions_assigned`)
- `PauseOnFullPolicy` actual partition pause/resume
- Terminal failure blocking (`has_terminal` flag already set, commit gating already in `should_commit`)
- Retry/DLQ infrastructure

---

## 10. Key Sources

- `src/consumer/runner.rs` — ConsumerRunner implementation
- `src/consumer/config.rs` — ConsumerConfig + rdkafka mapping
- `src/consumer/message.rs` — OwnedMessage, MessageTimestamp
- `src/dispatcher/queue_manager.rs` — QueueManager, HandlerMetadata
- `src/dispatcher/backpressure.rs` — BackpressurePolicy trait
- `src/offset/offset_tracker.rs` — BTreeSet offset tracking algorithm
- `src/worker_pool/pool.rs` — WorkerPool
- `kafpy/config.py` — Python ConsumerConfig dataclass
- `src/pyconsumer.rs` — PyConsumer PyO3 binding
- `.planning/research/STACK.md` — Crate versions, patterns
- `.planning/research/PITFALLS.md` — Domain pitfalls
- `.planning/research/ARCHITECTURE.md` — Architecture overview