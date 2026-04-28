# Phase 1 Plan: Core Consumer Engine & Configuration

**Date**: 2026-04-28
**Wave**: 1
**Phase**: 01-core-consumer-engine-configuration
**Depends on**: None

## Frontmatter

```yaml
wave: 1
depends_on: []
requirements:
  - CORE-01
  - CORE-02
  - CORE-03
  - CORE-04
  - CORE-05
  - CORE-07
  - MSG-01
  - MSG-02
  - MSG-04
  - MSG-05
  - OFF-01
  - OFF-02
  - OFF-03
  - CONF-01
  - CONF-02
  - CONF-04
autonomous: false
```

## Context

The codebase is **brownfield**: early implementation exists in both Rust (`src/`) and Python (`kafpy/`). Phase 1 verification requires confirming each module behaves according to spec, writing missing unit tests, and fixing any discrepancies. The core modules (ConsumerRunner, QueueManager, OffsetTracker, BackpressurePolicy) are structurally complete but need verification.

## Success Criteria

1. **Consumer connects and subscribes**: `ConsumerRunner::new()` subscribes to topics via rdkafka and produces `OwnedMessage` via a channel.
2. **Messages route through bounded queues**: `QueueManager` dispatches via bounded `mpsc::channel(capacity)` with `try_send`.
3. **Offsets commit with highest-contiguous-offset semantics**: `OffsetTracker::ack()` uses BTreeSet; `should_commit()` gates on contiguous cursor.
4. **Python ConsumerConfig is ergonomic**: `kafpy.ConsumerConfig` dataclass with `to_rust()` converts to `rdkafka` config.
5. **Backpressure policy works**: `BackpressurePolicy::on_queue_full()` returns `Drop`/`Wait`/`FuturePausePartition`; `DefaultBackpressurePolicy` drops.

---

## Wave 1 Tasks

### 1.1: Verify ConsumerRunner — Topic Subscription, Message Loop, Graceful Stop

**Requirement IDs**: CORE-01, CORE-02, CORE-03, CORE-07

**<read_first>**:
- `src/consumer/runner.rs`
- `src/consumer/config.rs`
- `src/consumer/error.rs`

**<acceptance_criteria>**:
- [ ] `grep -n "subscribe" src/consumer/runner.rs` shows `consumer.subscribe(...)` call in `ConsumerRunner::new()`
- [ ] `grep -n "StreamConsumer" src/consumer/runner.rs` confirms `StreamConsumer` type used (rdkafka consumer)
- [ ] `grep -n "broadcast::channel" src/consumer/runner.rs` shows shutdown channel creation
- [ ] `grep -n "tokio::select!" src/consumer/runner.rs` shows select loop with biased shutdown branch
- [ ] `grep -n "OwnedMessage::from_borrowed" src/consumer/runner.rs` confirms message conversion
- [ ] `grep -n "stop()" src/consumer/runner.rs` shows `stop()` method that sends shutdown signal
- [ ] `grep -n "auto.offset.reset\|enable.auto.commit\|group.id" src/consumer/config.rs` shows rdkafka property mapping in `into_rdkafka_config()`

**<action>**:
1. **Verify** `ConsumerRunner::new()` — it creates a `StreamConsumer` via `into_rdkafka_config().create()`, then calls `consumer.subscribe(&topics)`.
2. **Verify** `run()` — it spawns an async task with `tokio::select!` biased branch on shutdown receiver, calls `consumer.recv()` (rdkafka), converts each `BorrowedMessage` to `OwnedMessage` via `OwnedMessage::from_borrowed()`, sends into `mpsc::channel(1000)`.
3. **Verify** `stop()` — sends `()` via `shutdown_tx` broadcast, causing the select loop to break.
4. **Verify** `store_offset()` — calls `consumer.store_offset()` inside `spawn_blocking` (two-phase manual commit pattern).
5. **Verify** `into_rdkafka_config()` — sets `bootstrap.servers`, `group.id`, `auto.offset.reset`, `enable.auto.commit=false` (manual commit), `enable.auto.offset.store=false`, `session.timeout.ms`, `heartbeat.interval.ms`, `max.poll.interval.ms`, `partition.assignment.strategy`.
6. **Verify** `ConsumerConfigBuilder` — has `new()`, `brokers()`, `group_id()`, `topics()`, `auto_offset_reset()`, `enable_auto_commit()`, `build()` returning `Result<ConsumerConfig, BuildError>`.
7. **Fix any discrepancy**: if any of the above patterns are missing, implement them now.

---

### 1.2: Verify OwnedMessage — Deserialization, Headers, Timestamp, Metadata

**Requirement IDs**: MSG-01, MSG-02

**<read_first>**:
- `src/consumer/message.rs`
- `src/kafka_message.rs` (if exists)

**<acceptance_criteria>**:
- [ ] `grep -n "pub struct OwnedMessage" src/consumer/message.rs` shows fields: `topic: String`, `partition: i32`, `offset: i64`, `key: Option<Vec<u8>>`, `payload: Option<Vec<u8>>`, `timestamp: MessageTimestamp`, `headers: Vec<(String, Option<Vec<u8>>)>`
- [ ] `grep -n "pub enum MessageTimestamp" src/consumer/message.rs` shows `CreateTime(i64)`, `LogAppendTime(i64)`, `NotAvailable` variants
- [ ] `grep -n "from_borrowed" src/consumer/message.rs` — `OwnedMessage::from_borrowed(msg: &BorrowedMessage)` converts rdkafka message
- [ ] `grep -n "headers()" src/consumer/message.rs` — `from_borrowed` extracts headers via `msg.headers().map(|h| ...)` iterating `h.try_get(i)`
- [ ] `grep -n "timestamp()" src/consumer/message.rs` — `from_borrowed` calls `MessageTimestamp::from_kafka(msg.timestamp())`
- [ ] `grep -n "payload_str\|key_str" src/consumer/message.rs` — helper methods for UTF-8 access
- [ ] `grep -n "size_bytes" src/consumer/message.rs` — `size_bytes()` returns payload+key length for metrics
- [ ] `grep -n "fn from_borrowed" src/consumer/message.rs` — single conversion point; no lifetime aliasing

**<action>**:
1. **Verify** `OwnedMessage::from_borrowed()` — converts `BorrowedMessage` to fully owned `OwnedMessage`. Key, payload stored as `Option<Vec<u8>>` (owned bytes). Headers extracted as `Vec<(String, Option<Vec<u8>>)>`.
2. **Verify** `MessageTimestamp` — `from_kafka(Timestamp)` maps `CreateTime`, `LogAppendTime`, `NotAvailable`. `as_millis()` and `as_system_time()` helper methods present.
3. **Verify** `payload_str()` and `key_str()` — return `Option<&str>` via UTF-8 validation on owned bytes.
4. **Verify** `size_bytes()` — returns `payload.len() + key.len()` for metrics.
5. **Verify** `MessageRef` — non-owning reference for routing (used by `as_ref()` method).
6. **Fix any discrepancy**: implement missing fields or methods.

---

### 1.3: Verify QueueManager — Bounded Channels, Atomic Counters, Handler Registration

**Requirement IDs**: MSG-04

**<read_first>**:
- `src/dispatcher/queue_manager.rs`

**<acceptance_criteria>**:
- [ ] `grep -n "mpsc::channel(capacity)" src/dispatcher/queue_manager.rs` — bounded channel created per handler
- [ ] `grep -n "try_send" src/dispatcher/queue_manager.rs` — `send_to_handler_by_id()` uses `try_send` for non-blocking dispatch
- [ ] `grep -n "queue_depth: Arc<AtomicUsize>" src/dispatcher/queue_manager.rs` — atomic counter for buffered messages
- [ ] `grep -n "inflight: Arc<AtomicUsize>" src/dispatcher/queue_manager.rs` — atomic counter for dispatched-not-acked
- [ ] `grep -n "inc_queue_depth\|inc_inflight" src/dispatcher/queue_manager.rs` — increment on send success
- [ ] `grep -n "parking_lot::Mutex" src/dispatcher/queue_manager.rs` — uses parking_lot mutex (not tokio::sync::Mutex) for non-async registration
- [ ] `grep -n "register_handler_with_semaphore" src/dispatcher/queue_manager.rs` — creates bounded channel + HandlerMetadata
- [ ] `grep -n "HandlerMetadata::new" src/dispatcher/queue_manager.rs` — constructor with capacity, semaphore, limit

**<action>**:
1. **Verify** `register_handler_with_semaphore()` — creates `mpsc::channel(capacity)` (bounded), wraps in `HandlerEntry` with `HandlerMetadata`, inserts into `handlers: HashMap<String, HandlerEntry>` under parking_lot mutex.
2. **Verify** `send_to_handler_by_id()` — locks handlers, looks up `HandlerId`, calls `entry.sender.try_send(message)`. On `Ok`: increments `queue_depth` and `inflight`. On `Err(TrySendError::Full(_))`: returns `DispatchError::QueueFull`. On `Err(TrySendError::Closed(_))`: returns `DispatchError::QueueClosed`.
3. **Verify** `HandlerMetadata` — has `queue_depth` (Arc<AtomicUsize>), `inflight` (Arc<AtomicUsize>), `semaphore: Option<Arc<Semaphore>>`, `outstanding_permits: AtomicUsize`, `semaphore_limit: usize`.
4. **Verify** `try_acquire_semaphore()` — non-blocking check; returns true if under limit, increments outstanding_permits atomically.
5. **Verify** `ack()` — decrements both `queue_depth` and `inflight` with saturating `compare_exchange` loop; releases permits to semaphore.
6. **Verify** `queue_snapshots()` — returns `HashMap<String, QueueSnapshot>` with cloned Arc counters for polling-based metrics.
7. **Fix any discrepancy**: implement missing behavior.

---

### 1.4: Verify BackpressurePolicy — Drop/Wait/FuturePausePartition, Trait, Default Implementation

**Requirement IDs**: CONF-04, MSG-05

**<read_first>**:
- `src/dispatcher/backpressure.rs`

**<acceptance_criteria>**:
- [ ] `grep -n "pub enum BackpressureAction" src/dispatcher/backpressure.rs` shows `Drop`, `Wait`, `FuturePausePartition(String)` variants
- [ ] `grep -n "pub trait BackpressurePolicy" src/dispatcher/backpressure.rs` — trait with `fn on_queue_full(&self, topic: &str, handler: &HandlerMetadata) -> BackpressureAction`
- [ ] `grep -n "DefaultBackpressurePolicy" src/dispatcher/backpressure.rs` — struct implementing `BackpressurePolicy`, returns `BackpressureAction::Drop`
- [ ] `grep -n "PauseOnFullPolicy" src/dispatcher/backpressure.rs` — struct implementing `BackpressurePolicy`, returns `BackpressureAction::FuturePausePartition(topic.to_string())`
- [ ] `grep -n "FuturePausePartition" src/dispatcher/backpressure.rs` — carries topic name string

**<action>**:
1. **Verify** `BackpressureAction` enum has exactly three variants: `Drop`, `Wait`, `FuturePausePartition(String)`.
2. **Verify** `BackpressurePolicy: Send + Sync` trait — `on_queue_full(topic, handler)` returns `BackpressureAction`.
3. **Verify** `DefaultBackpressurePolicy` — on queue full, returns `Drop`.
4. **Verify** `PauseOnFullPolicy` — on queue full, returns `FuturePausePartition(topic)`.
5. **Verify** usage in `QueueManager::send_to_handler_by_id()` — when `try_send` returns `Full`, the caller consults `BackpressurePolicy` to decide action.
6. **Fix any discrepancy**: implement missing variants or trait methods.

---

### 1.5: Verify OffsetTracker — BTreeSet Algorithm, Contiguous Commit, Partition-Aware

**Requirement IDs**: OFF-01, OFF-02, OFF-03

**<read_first>**:
- `src/offset/offset_tracker.rs`

**<acceptance_criteria>**:
- [ ] `grep -n "BTreeSet<i64>" src/offset/offset_tracker.rs` — `pending_offsets: BTreeSet<i64>` for out-of-order buffering
- [ ] `grep -n "committed_offset: i64" src/offset/offset_tracker.rs` — starts at -1, advances as contiguous offsets arrive
- [ ] `grep -n "fn ack" src/offset/offset_tracker.rs` — `ack(offset)` inserts into pending, then loop: while `pending_offsets.remove(committed_offset + 1)` advances `committed_offset`
- [ ] `grep -n "fn should_commit" src/offset/offset_tracker.rs` — `should_commit()` returns true when `pending_offsets.contains(committed_offset + 1)` and `!has_terminal`
- [ ] `grep -n "has_terminal: bool" src/offset/offset_tracker.rs` — terminal flag blocks commit for this partition
- [ ] `grep -n "fn mark_failed" src/offset/offset_tracker.rs` — `mark_failed(offset)` removes from pending_offsets, inserts into failed_offsets
- [ ] `grep -n "TopicPartitionKey" src/offset/offset_tracker.rs` — tuple struct for type-safe (String, i32) key
- [ ] `grep -n "partitions: Mutex<HashMap<TopicPartitionKey, PartitionState>>" src/offset/offset_tracker.rs` — per-partition independent state
- [ ] `grep -n "fn highest_contiguous" src/offset/offset_tracker.rs` — returns `committed_offset` for a topic-partition

**<action>**:
1. **Verify** `PartitionState` struct — `committed_offset: i64` (starts -1), `pending_offsets: BTreeSet<i64>`, `failed_offsets: BTreeSet<i64>`, `has_terminal: bool`, `last_failure_reason: Option<FailureReason>`.
2. **Verify** `ack()` algorithm — insert offset into pending, remove from failed (retry succeeded), then loop: advance `committed_offset` while next offset is in pending.
3. **Verify** `should_commit()` — returns true when next offset is pending and no terminal failure (per-partition blocking).
4. **Verify** `mark_failed()` — removes from pending, adds to failed. Does NOT revert committed_offset.
5. **Verify** `PartitionState::new()` — `committed_offset = -1`.
6. **Verify** `all_partitions()` — returns all registered (topic, partition) pairs for final commit on shutdown.
7. **Run unit tests**: `cargo test --lib offset::offset_tracker` — all tests in the `#[cfg(test)]` module at bottom of file must pass.
8. **Fix any discrepancy**: implement missing behavior.

---

### 1.6: Verify Python ConsumerConfig — Dataclass, Validation, to_rust() Conversion

**Requirement IDs**: CONF-01, CONF-02

**<read_first>**:
- `kafpy/config.py`

**<acceptance_criteria>**:
- [ ] `grep -n "@dataclass(frozen=True)" kafpy/config.py` — `ConsumerConfig` is frozen dataclass
- [ ] `grep -n "bootstrap_servers\|group_id\|topics" kafpy/config.py` — required fields present
- [ ] `grep -n "auto_offset_reset\|enable_auto_commit\|session_timeout_ms\|heartbeat_interval_ms\|max_poll_interval_ms" kafpy/config.py` — all consumer fields present
- [ ] `grep -n "__post_init__" kafpy/config.py` — validates `auto_offset_reset in {"earliest", "latest"}`, `num_workers > 0`, `drain_timeout_secs > 0`
- [ ] `grep -n "def to_rust" kafpy/config.py` — converts to `_kafpy.ConsumerConfig` via `kafpy._kafpy` module
- [ ] `grep -n "_kafpy.ConsumerConfig" kafpy/config.py` — maps all fields: `brokers`, `group_id`, `topics`, `auto_offset_reset`, `session_timeout_ms`, etc.
- [ ] `grep -n "RetryConfig\|ObservabilityConfig" kafpy/config.py` — nested config types present

**<action>**:
1. **Verify** `ConsumerConfig` dataclass has fields: `bootstrap_servers: str`, `group_id: str`, `topics: list[str]`, `auto_offset_reset: str = "earliest"`, `enable_auto_commit: bool = False`, `session_timeout_ms: int = 30000`, `heartbeat_interval_ms: int = 3000`, `max_poll_interval_ms: int = 300000`, `security_protocol`, `sasl_mechanism`, `sasl_username`, `sasl_password`, `fetch_min_bytes`, `max_partition_fetch_bytes`, `partition_assignment_strategy`, `retry_backoff_ms`, `message_batch_size`, `retry_policy`, `dlq_topic_prefix`, `drain_timeout_secs`, `num_workers`, `enable_auto_offset_store`, `observability_config`, `handler_timeout_ms`.
2. **Verify** `__post_init__` — validates `auto_offset_reset` against `AUTO_OFFSET_RESET_VALUES`, positive numeric checks.
3. **Verify** `to_rust()` — imports `kafpy._kafpy`, builds `_kafpy.ConsumerConfig(brokers=..., group_id=..., topics=..., ...)` passing all fields. Converts Python types to Rust equivalents (e.g., `retry_policy` → `PyRetryPolicy`, `observability_config` → `PyObservabilityConfig`).
4. **Verify** `RetryConfig` — `max_attempts: int = 3`, `base_delay: float = 0.1`, `max_delay: float | None = None`, `jitter_factor: float | None = None`, with `__post_init__` validation.
5. **Verify** `ObservabilityConfig` — `otlp_endpoint: str | None = None`, `service_name: str = "kafpy"`, `sampling_ratio: float = 1.0`, `log_format: str = "pretty"`.
6. **Fix any discrepancy**: implement missing fields or validation.

---

### 1.7: Write Unit Tests for OffsetTracker BTreeSet Algorithm

**Requirement IDs**: OFF-01, OFF-02

**<read_first>**:
- `src/offset/offset_tracker.rs` (bottom `#[cfg(test)]` module)

**<acceptance_criteria>**:
- [ ] `cargo test --lib offset::offset_tracker::tests -- --nocapture` — all 7 tests pass:
  - `sequential_acks_advance_contiguous`
  - `out_of_order_acks_buffer_until_gap_fills`
  - `should_commit_true_when_next_offset_pending`
  - `failed_offset_does_not_advance_cursor`
  - `new_partition_starts_at_minus_one`
  - `first_ack_advances_from_minus_one`
  - `multiple_partitions_independent`
- [ ] `grep -n "#\[test\]" src/offset/offset_tracker.rs` — at least 7 test functions present
- [ ] `grep -n "assert_eq!" src/offset/offset_tracker.rs` — assertions for committed offsets, should_commit state

**<action>**:
1. **Run** existing tests: `cargo test --lib offset::offset_tracker -- --nocapture`. All must pass.
2. **If tests missing**: add the test functions listed in acceptance criteria to the `#[cfg(test)]` module.
3. **Verify** `sequential_acks_advance_contiguous` — acks 10, 11, 12 → `highest_contiguous` returns 12.
4. **Verify** `out_of_order_acks_buffer_until_gap_fills` — acks 10, 12 → highest is 10 (gap at 11); acks 11 → highest becomes 12.
5. **Verify** `should_commit_true_when_next_offset_pending` — after ack(10), should_commit returns true (next=11 is pending).
6. **Verify** `failed_offset_does_not_advance_cursor` — ack 10, 11, 12; mark_failed(11) → committed stays at 10 until 11 retried/acked.
7. **Verify** `new_partition_starts_at_minus_one` — no partition registered → `committed_offset` returns -1.
8. **Verify** `first_ack_advances_from_minus_one` — ack(0) → highest becomes 0.
9. **Verify** `multiple_partitions_independent` — ack(5) on p0, ack(10) on p1 → both advance independently; gaps on p0 don't affect p1.

---

### 1.8: Write Unit Tests for BackpressurePolicy and QueueManager

**Requirement IDs**: MSG-04, MSG-05, CONF-04

**<read_first>**:
- `src/dispatcher/backpressure.rs`
- `src/dispatcher/queue_manager.rs`

**<acceptance_criteria>**:
- [ ] `grep -n "#\[test\]" src/dispatcher/backpressure.rs` — at least one test for `DefaultBackpressurePolicy::on_queue_full`
- [ ] `grep -n "#\[test\]" src/dispatcher/queue_manager.rs` — tests for:
  - Handler registration creates bounded channel
  - `try_send` success increments counters
  - `try_send` Full returns `DispatchError::QueueFull`
  - `ack()` decrements counters
- [ ] `cargo test --lib dispatcher::queue_manager` — all tests pass
- [ ] `cargo test --lib dispatcher::backpressure` — all tests pass

**<action>**:
1. **Write** backpressure tests: verify `DefaultBackpressurePolicy::on_queue_full` returns `Drop`, `PauseOnFullPolicy` returns `FuturePausePartition(topic)`.
2. **Write** QueueManager tests:
   - `register_handler_with_semaphore(topic, capacity=10, None)` → returns `Receiver<OwnedMessage>`; calling `send_to_handler_by_id` with `try_send` succeeds when channel not full.
   - Fill channel to capacity, next `try_send` returns `DispatchError::QueueFull`.
   - After `ack(topic, 1)`, counters decrement correctly.
3. **Run** tests: `cargo test --lib dispatcher`.

---

## Wave 2 Tasks

### 2.1: Verify Consumer Config Builder — All Fields, Build Pattern

**Requirement IDs**: CONF-01, CONF-02

**<read_first>**:
- `src/consumer/config.rs`

**<acceptance_criteria>**:
- [ ] `grep -n "ConsumerConfigBuilder" src/consumer/config.rs` — builder struct with all optional fields
- [ ] `grep -n "fn brokers\|fn group_id\|fn topics\|fn auto_offset_reset\|fn enable_auto_commit" src/consumer/config.rs` — fluent builder methods
- [ ] `grep -n "fn build" src/consumer/config.rs` — `build()` returns `Result<ConsumerConfig, BuildError>`
- [ ] `grep -n "BuildError" src/consumer/config.rs` — error enum: `MissingField(&'static str)`, `NoTopics`
- [ ] `grep -n "into_rdkafka_config" src/consumer/config.rs` — full rdkafka property mapping

**<action>**:
1. **Verify** builder has methods: `new()`, `brokers()`, `group_id()`, `topics()`, `auto_offset_reset()`, `enable_auto_commit()`, `enable_auto_offset_store()`, `session_timeout()`, `heartbeat_interval()`, `max_poll_interval()`, `security_protocol()`, `sasl_mechanism()`, `fetch_min_bytes()`, `max_partition_fetch_bytes()`, `partition_assignment_strategy()`, `retry_backoff()`, `default_retry_policy()`, `dlq_topic_prefix()`, `drain_timeout()`, `routing_rule()`, `build()`.
2. **Verify** `build()` returns `Err(BuildError::MissingField("brokers"))` when brokers missing, `Err(BuildError::NoTopics)` when topics empty.
3. **Verify** `into_rdkafka_config()` sets: `queued.max.messages.kbytes = "65536"` (64MB per partition — memory bound per PITFALLS-2.1), `queued.min.messages = "100000"`.
4. **Run** build test: `cargo build --lib` in `src/` — must compile without errors.

---

### 2.2: Verify PyConsumer — add_handler, start, stop, status

**Requirement IDs**: CORE-01, CORE-07

**<read_first>**:
- `src/pyconsumer.rs`

**<acceptance_criteria>**:
- [ ] `grep -n "add_handler" src/pyconsumer.rs` — `#[pyo3(signature = (topic, callback, mode=None, ...))]` registers Python callback
- [ ] `grep -n "pub fn start" src/pyconsumer.rs` — `start(&self, py: Python<'_>) -> PyResult<Py<PyAny>>` calls `RuntimeBuilder::new(config, handlers, shutdown_token).build().await` then `runtime.run().await`
- [ ] `grep -n "pub fn stop" src/pyconsumer.rs` — `stop(&self)` calls `shutdown_token.cancel()`
- [ ] `grep -n "HandlerMetadata" src/pyconsumer.rs` — stores `callback: Arc<Py<PyAny>>`, `mode: HandlerMode`, `timeout_ms: Option<u64>`
- [ ] `grep -n "runtime.run()" src/pyconsumer.rs` — start runs the full runtime
- [ ] `grep -n "CancellationToken" src/pyconsumer.rs` — uses `tokio_util::sync::CancellationToken` for graceful stop

**<action>**:
1. **Verify** `PyConsumer::new(config: ConsumerConfig)` — stores config and creates `shutdown_token`.
2. **Verify** `add_handler(topic, callback, mode, batch_max_size, batch_max_wait_ms, timeout_ms)` — stores callback as `Arc<Py<PyAny>>` wrapped in `HandlerMetadata` in `handlers: Arc<Mutex<HashMap>>`.
3. **Verify** `start()` — calls `RuntimeBuilder::new(...).build().await` then `runtime.run().await`. Uses `pyo3_async_runtimes::tokio::future_into_py`.
4. **Verify** `stop()` — calls `shutdown_token.cancel()`.
5. **Verify** `status()` — calls `get_runtime_snapshot()` from `observability::runtime_snapshot`.
6. **Build check**: `cargo build --lib` in project root — must compile.

---

### 2.3: Verify WorkerPool — spawn_blocking for Python, Handler Invocation

**Requirement IDs**: MSG-04, MSG-05

**<read_first>**:
- `src/worker_pool/pool.rs`
- `src/worker_pool/worker.rs`

**<acceptance_criteria>**:
- [ ] `grep -n "spawn_blocking" src/worker_pool/pool.rs` — worker pool uses `spawn_blocking` for Python handler calls (not `spawn` which would block Tokio event loop)
- [ ] `grep -n "WorkerPool::new\|WorkerPool::run\|WorkerPool::shutdown" src/worker_pool/pool.rs` — struct with run/shutdown methods
- [ ] `grep -n "fn worker_loop" src/worker_pool/worker.rs` — single message processing loop
- [ ] `grep -n "Arc<Py" src/worker_pool/worker.rs` or `src/python/handler.rs` — Python callback stored as `Arc<Py<PyAny>>`

**<action>**:
1. **Verify** `WorkerPool::run()` — spawns N worker tasks that pull from `mpsc::Receiver<OwnedMessage>`.
2. **Verify** `worker_loop` — calls `receiver.recv().await`, routes to handler, calls `PythonHandler::invoke()` (which uses `spawn_blocking`).
3. **Verify** Python handler GIL handling — `spawn_blocking` ensures Python calls don't block Tokio async workers (critical per PITFALLS-1.1).
4. **Verify** backpressure propagation — when queue is full and `BackpressureAction::Drop` is returned, the message is discarded and `ack` is called to decrement counters.
5. **Build check**: `cargo build --lib`.

---

### 2.4: End-to-End Integration Test Verification

**Requirement IDs**: CORE-01, CORE-04, MSG-01, MSG-02, MSG-04, MSG-05, OFF-01, OFF-02, OFF-03

**<read_first>**:
- `tests/test_exceptions.py` (if exists)
- `tests/` directory

**<acceptance_criteria>**:
- [ ] `ls tests/` shows test files
- [ ] `grep -n "ConsumerConfig\|kafpy" tests/*.py` — tests import and use ConsumerConfig
- [ ] `grep -n "pytest\|unittest" tests/` — tests use pytest framework
- [ ] `python -m pytest tests/ --collect-only` — tests are discoverable

**<action>**:
1. **Verify** test files exist and import `kafpy.ConsumerConfig`.
2. **Verify** test infrastructure: `pytest` is used, `kafpy` module is importable.
3. **If no tests exist**: create `tests/test_consumer_config.py` with tests for:
   - `ConsumerConfig` dataclass creation and `to_rust()` conversion
   - Validation of `auto_offset_reset` invalid values
   - Field mapping from Python to Rust config
4. **Run** Python tests: `python -m pytest tests/ -v`.

---

## Wave 3 Tasks

### 3.1: Rust Compilation and Clippy Check

**Requirement IDs**: all Phase 1

**<read_first>**:
- `Cargo.toml` (root)
- `src/lib.rs`

**<acceptance_criteria>**:
- [ ] `cargo check --lib 2>&1 | tail -20` — no compilation errors
- [ ] `cargo clippy --lib -- -D warnings 2>&1 | tail -20` — no clippy warnings (treated as errors)
- [ ] `cargo fmt -- --check 2>&1` — no formatting issues

**<action>**:
1. Run `cargo check --lib` — fix any compilation errors.
2. Run `cargo clippy --lib -- -D warnings` — fix all warnings.
3. Run `cargo fmt` if formatting issues found.
4. Commit format fix if needed: `cargo fmt && git diff src/ | head -50`.

---

### 3.2: Final Verification — All Must-Haves

**Requirement IDs**: all Phase 1

**<read_first>**:
- `src/consumer/runner.rs`
- `src/dispatcher/queue_manager.rs`
- `src/offset/offset_tracker.rs`
- `kafpy/config.py`

**<acceptance_criteria>**:
- [ ] **Consumer connects and subscribes**: `ConsumerRunner::new()` calls `subscribe(&config.topics)` on `StreamConsumer` created from `into_rdkafka_config()`
- [ ] **Messages route through bounded queues**: `QueueManager::register_handler_with_semaphore(capacity)` creates bounded `mpsc::channel(capacity)`; `send_to_handler_by_id()` uses `try_send`
- [ ] **Offsets commit with highest-contiguous-offset semantics**: `OffsetTracker::ack()` uses BTreeSet algorithm; `should_commit()` returns true only when next offset is pending; `commit()` called after store_offset
- [ ] **Python ConsumerConfig is ergonomic**: `ConsumerConfig(brokers=..., group_id=..., topics=[...])` with `to_rust()` mapping to `_kafpy.ConsumerConfig`
- [ ] **Backpressure policy works**: `BackpressurePolicy` trait with `Drop`/`Wait`/`FuturePausePartition`; `DefaultBackpressurePolicy` returns `Drop`; `PauseOnFullPolicy` returns `FuturePausePartition`

**<action>**:
1. Read each file listed in read_first.
2. Trace the full message path: `ConsumerRunner::run()` → `Dispatcher` → `QueueManager::send_to_handler_by_id()` → WorkerPool → `PythonHandler::invoke()` → `spawn_blocking` → Python callback.
3. Trace the offset path: `PythonHandler::invoke()` returns `ExecutionResult` → `OffsetTracker::ack(topic, partition, offset)` → `should_commit()` → `store_offset` + `commit`.
4. Confirm all 16 requirements are satisfied: run through each requirement ID and mark complete.

---

## Files Modified (Summary)

| File | Changes |
|------|---------|
| `src/offset/offset_tracker.rs` | Add missing unit tests if not present |
| `src/dispatcher/queue_manager.rs` | Add unit tests if not present |
| `src/dispatcher/backpressure.rs` | Add unit tests if not present |
| `tests/test_consumer_config.py` | Create if no Python tests exist |
| `src/consumer/config.rs` | Verify all builder methods; fix if missing |
| `src/consumer/runner.rs` | Verify all CORE-07 graceful stop; fix if missing |
| `kafpy/config.py` | Verify all CONF-01 fields; fix if missing |

## Dependencies

No Phase 1 task is blocked by another Phase 1 task. All tasks in Wave 1 can run in parallel. Wave 2 and Wave 3 follow Wave 1.

## Verification Commands

```bash
# Rust compilation
cargo check --lib

# Clippy
cargo clippy --lib -- -D warnings

# Format
cargo fmt -- --check

# Unit tests
cargo test --lib

# Python tests (if pytest installed)
python -m pytest tests/ -v
```