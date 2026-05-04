# KafPy Architecture

## System Overview

KafPy is a high-performance Kafka consumer framework where Rust handles all I/O-intensive work (consuming, dispatching, offset management) and Python provides the ergonomic handler API surface.

```
┌──────────────────────────────────────────────────────┐
│                    Python Layer                       │
│  kafpy/                                              │
│  ├── KafPy (runtime.py) — handler decorator, run()   │
│  ├── Consumer (consumer.py) — Python wrapper         │
│  ├── ConsumerConfig (config.py) — config + to_rust()│
│  ├── KafkaMessage (handlers.py) — typed message      │
│  ├── FanOutBuilder (fanout.py) — fan-out builder     │
│  └── BenchmarkResult (benchmark.py) — benchmarks      │
├──────────── PyO3 Bridge ──────────────────────────────┤
│  src/lib.rs — #[pymodule] _kafpy                       │
│  src/pyconsumer.rs — PyConsumer (Python-facing)       │
│  src/config.rs — ConsumerConfig, ProducerConfig       │
│  src/kafka_message.rs — KafkaMessage (PyO3)           │
│  src/produce.rs — PyProducer (PyO3)                   │
│  src/pyconfig.rs — PyRetryPolicy, PyObservabilityConfig│
├──────────────────────────────────────────────────────┤
│                Pure Rust Core (no PyO3)               │
│  consumer/ — ConsumerRunner, ConsumerStream            │
│  dispatcher/ — Queue-based message routing             │
│  worker_pool/ — Worker threads, batch/stream loops     │
│  coordinator/ — Offset commit coordination              │
│  offset/ — Highest-contiguous-offset algorithm         │
│  shutdown/ — 4-phase shutdown lifecycle                 │
│  retry/ — Exponential backoff with jitter              │
│  failure/ — Failure classification taxonomy            │
│  dlq/ — DLQ routing and metadata                       │
│  routing/ — Topic/header/key pattern routing           │
│  observability/ — Metrics, tracing, runtime snapshots   │
│  middleware/ — Before/after/error middleware chain      │
│  benchmark/ — Latency/throughput measurement           │
│  runtime/ — RuntimeBuilder (wires config to internals)│
└──────────────────────────────────────────────────────┘
```

## Key Data Flows

### 1. Message Consumption Flow
```
Kafka Broker → rdkafka → ConsumerRunner → ConsumerDispatcher
  → per-topic mpsc channel → Worker Pool → Python Handler callback
```

### 2. Offset Commit Flow
```
Python Handler returns → Worker Pool → OffsetCoordinator
  → highest-contiguous-offset algorithm → rdkafka offset commit
```

### 3. Error/Retry/DLQ Flow
```
Handler exception → FailureClassifier → FailureCategory (Retryable/Terminal/NonRetryable)
  → Retryable: RetryPolicy exponential backoff → re-queue
  → Terminal: DlqRouter → produce to DLQ topic
  → NonRetryable: skip (ack offset, no DLQ)
```

### 4. Shutdown Flow
```
Signal → ShutdownCoordinator
  → Running → Draining (wait for in-flight, flush DLQ)
  → Finalizing (commit offsets) → Done
```

## Key Design Decisions

1. **PyO3 boundary is narrow**: Only `PyConsumer`, `KafkaMessage`, `ProducerConfig`/`ConsumerConfig`, `PyRetryPolicy`, `PyObservabilityConfig`, `PyFailureCategory`/`PyFailureReason` cross the bridge. All internals are pure Rust.

2. **Dispatcher pattern**: Messages route from `ConsumerRunner` through `ConsumerDispatcher` to per-topic bounded `mpsc` channels, providing backpressure via `try_send` (DISP-08).

3. **Highest-contiguous-offset**: Offset commits use the highest-contiguous-offset algorithm to avoid skipping offsets on partial failures.

4. **4-phase shutdown**: Running → Draining → Finalizing → Done ensures graceful in-flight message completion before offset commits.

5. **Config pipeline**: Python `ConsumerConfig` → `to_rust()` → Rust `ConsumerConfig` → `RuntimeBuilder` assembles all components.

6. **Handler modes**: Sync, async, batch_sync, batch_async, streaming_async detected via `inspect.iscoroutinefunction` / `inspect.isasyncgenfunction` (D-02).

7. **Middleware chain**: `before()` → handler → `after()` / `on_error()` wraps each handler invocation with extensible middleware (Logging, Metrics built-in).

8. **Fan-out**: Messages dispatch to multiple sink topics concurrently with configurable `max_fan_out` degree (capped at 64).

## Module Boundary Map

| Rust Module | PyO3 Exposed | Purpose |
|-------------|-------------|---------|
| `config` | Yes | ConsumerConfig, ProducerConfig |
| `pyconfig` | Yes | PyRetryPolicy, PyObservabilityConfig, PyFailureCategory, PyFailureReason |
| `kafka_message` | Yes | KafkaMessage (with headers, timestamp) |
| `produce` | Yes | PyProducer |
| `pyconsumer` | Yes | PyConsumer, get_runtime_snapshot, register_status_callback |
| `consumer` | No | Core consumer loop, message types |
| `dispatcher` | No | Queue-based routing, backpressure |
| `python` | No | Python callback execution |
| `worker_pool` | No | Worker management, batch/stream loops |
| `coordinator` | No | Offset commit coordination |
| `failure` | No (via PyFailureCategory/PyFailureReason) | Failure classification |
| `retry` | No (via PyRetryPolicy) | Exponential backoff |
| `shutdown` | No | 4-phase shutdown lifecycle |
| `offset` | No | Highest-contiguous algorithm |
| `dlq` | No | DLQ routing, metadata |
| `observability` | Partially | Metrics, tracing, runtime snapshots |
| `routing` | No | Topic/header/key pattern routing |
| `runtime` | No | RuntimeBuilder (assembles components) |
| `middleware` | No | Handler middleware chain |
| `benchmark` | No | Internal measurement infrastructure |