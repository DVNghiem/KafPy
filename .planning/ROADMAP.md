# KafPy Roadmap

**Project:** Rust-Core, Python-Logic Kafka Consumer Framework
**Created:** 2026-04-28
**Granularity:** Standard (5-8 phases)

---

## Overview

KafPy delivers a high-performance Kafka consumer framework where Rust owns the runtime core (concurrency, memory, Kafka protocol) and Python owns business logic (message handlers). The framework provides bounded queues with native backpressure, at-least-once delivery semantics, automatic DLQ routing, and graceful shutdown.

**Total v1 Requirements:** 45
**Phases:** 4

---

## Phase 1: Core Consumer Engine & Configuration

**Purpose:** Establish the foundational Rust-based Kafka consumer engine with Python configuration interface. This phase delivers a working single-handler consumer that can connect to Kafka, consume messages, and route them through bounded queues to Python handlers.

### Requirements Covered

| ID | Requirement | Category |
|----|-------------|----------|
| CORE-01 | Rust-based Kafka consumer engine managing full message-consumption lifecycle | Core Consumer |
| CORE-02 | Consumer groups with dynamic partition assignment via rdkafka | Core Consumer |
| CORE-03 | Topic subscription by name and regex pattern | Core Consumer |
| CORE-04 | Manual offset commit with highest-contiguous-offset semantics | Core Consumer |
| CORE-05 | Auto offset commit as fallback option | Core Consumer |
| CORE-07 | Graceful start/stop with deterministic resource cleanup | Core Consumer |
| MSG-01 | Message deserialization (JSON, msgpack) with custom decoder support | Message Handling |
| MSG-02 | Headers, timestamp, partition, and offset access in handlers | Message Handling |
| MSG-04 | Per-handler bounded queues with configurable capacity | Message Handling |
| MSG-05 | Backpressure propagation when handler queues are full | Message Handling |
| OFF-01 | Partition-aware offset tracking with BTreeSet buffering | Offset and Delivery |
| OFF-02 | Contiguous offset commit only when prior offsets are acked | Offset and Delivery |
| OFF-03 | Explicit store_offset + commit pattern (not auto-store) | Offset and Delivery |
| CONF-01 | Python-first configuration via ConsumerConfig dataclass | Configuration |
| CONF-02 | Kafka client config mapping to rdkafka settings | Configuration |
| CONF-04 | BackpressurePolicy trait with Drop/Wait/PausePartition actions | Configuration |

### Deliverables

1. **Rust Consumer Core**
   - rdkafka-based consumer with consumer group support
   - Topic subscription (by name and regex)
   - Manual and auto offset commit modes
   - Graceful start/stop with resource cleanup

2. **Message Pipeline**
   - JSON and msgpack deserialization with custom decoder interface
   - Message metadata access (headers, timestamp, partition, offset)
   - Per-handler bounded mpsc channels
   - Backpressure propagation (Drop/Wait/PausePartition)

3. **Offset Management**
   - BTreeSet-based partition-aware offset tracking
   - Highest-contiguous-offset commit semantics
   - Explicit store_offset + manual commit pattern

4. **Python Configuration Interface**
   - ConsumerConfig dataclass with typed fields
   - Direct rdkafka config mapping
   - BackpressurePolicy enum with action variants

### Success Criteria

1. **Consumer connects and subscribes**: A Python process can create a ConsumerConfig, build a consumer, subscribe to topics by name and regex, and receive messages without panics or resource leaks.
2. **Messages route through bounded queues**: When a handler's queue is full, the backpressure action (Drop/Wait/PausePartition) is applied and does not crash the consumer.
3. **Offsets commit correctly**: After processing a message, the offset is stored and committed; replay from the committed offset resumes at the correct position.
4. **Configuration is ergonomic**: A Python developer can configure the consumer entirely from Python using ConsumerConfig without touching Rust types directly.

---

## Phase 2: Rebalance Handling, Failure Handling & Lifecycle

**Purpose:** Add rebalance-safe partition handling, comprehensive failure classification with retry/DLQ, and lifecycle operations (graceful shutdown, drain, SIGTERM). This phase makes the consumer production-ready under real cluster operations.

### Requirements Covered

| ID | Requirement | Category |
|----|-------------|----------|
| CORE-06 | Rebalance listener with on_partitions_revoked/assigned callbacks | Core Consumer |
| CORE-08 | Pause/resume partitions for flow control | Core Consumer |
| MSG-03 | Key-based routing as fallback after topic routing | Message Handling |
| OFF-04 | Terminal failure blocking (poison messages block partition progress) | Offset and Delivery |
| FAIL-01 | Failure classification: Retryable, Terminal, NonRetryable | Failure Handling |
| FAIL-02 | Default failure classifier mapping Python exceptions | Failure Handling |
| FAIL-03 | Capped exponential backoff with jitter for retries | Failure Handling |
| FAIL-04 | Maximum retry attempts configuration | Failure Handling |
| FAIL-05 | DLQ handoff after retries exhausted or terminal failure | Failure Handling |
| FAIL-06 | DLQ metadata envelope (original topic/partition/offset, failure reason, attempt count) | Failure Handling |
| FAIL-07 | Default DLQ router: dlq_topic_prefix + original_topic | Failure Handling |
| LIFE-01 | Graceful shutdown with bounded drain timeout | Lifecycle and Operations |
| LIFE-02 | Queue draining before shutdown (in-flight messages complete or timeout) | Lifecycle and Operations |
| LIFE-03 | Failed messages flushed to DLQ on shutdown | Lifecycle and Operations |
| LIFE-04 | SIGTERM handling matching Kubernetes pod termination | Lifecycle and Operations |
| LIFE-05 | Rebalance-safe partition handling (revoke commits pending offsets) | Lifecycle and Operations |
| CONF-03 | RetryConfig: max_attempts, base_delay, max_delay, jitter_factor | Configuration |

### Deliverables

1. **Rebalance Handling**
   - on_partitions_revoked callback: commit pending offsets before revocation
   - on_partitions_assigned callback: resume from committed offsets
   - Pause/resume for flow control without losing partition ownership

2. **Failure Classification & Retry**
   - Failure trait: Retryable, Terminal, NonRetryable variants
   - Default classifier mapping Python exceptions (KeyError → NonRetryable, etc.)
   - Capped exponential backoff with jitter (RetryConfig)
   - Per-message retry accounting with attempt count

3. **DLQ Routing**
   - Automatic DLQ handoff after retries exhausted or on terminal failure
   - DLQ metadata envelope (original topic, partition, offset, failure reason, attempt count)
   - Default DLQ router: `{dlq_topic_prefix}{original_topic}`
   - Key-based routing fallback when topic routing does not match

4. **Lifecycle Operations**
   - Graceful shutdown with bounded drain timeout
   - Queue draining: in-flight messages complete or timeout
   - Failed messages flushed to DLQ on shutdown
   - SIGTERM handling aligned with Kubernetes pod terminationGracePeriod

5. **Terminal Failure Blocking**
   - Poison messages (retries exhausted) block partition progress
   - Terminal failure does not commit offset, allowing manual intervention

### Success Criteria

1. **Rebalance is handled safely**: When a partition is revoked and reassigned to another consumer, pending offsets are committed before revocation; after reassignment, the consumer resumes from the correct committed offset.
2. **Failures route to DLQ after retries**: A Python handler that raises a retryable exception is retried up to max_attempts with exponential backoff; after exhaustion, the message is sent to the DLQ topic with full metadata.
3. **Shutdown drains queues cleanly**: Sending SIGTERM to the process stops new message ingestion, waits for in-flight messages (up to drain_timeout), commits offsets, and flushes any failed messages to DLQ before exit.
4. **SIGTERM behavior matches Kubernetes**: The process installs a SIGTERM handler that triggers graceful shutdown; the process exits within the terminationGracePeriod.
5. **Poison messages block partition**: A message that exhausts retries is not committed; the partition does not advance offset until the poison message is manually handled or a configured redrive strategy is applied.

---

## Phase 3: Python Handler API

**Purpose:** Deliver the ergonomic Python developer API centered on the @handler decorator. This phase provides sync and async handler support, ExecutionContext for trace propagation, and per-handler concurrency configuration.

### Requirements Covered

| ID | Requirement | Category |
|----|-------------|----------|
| PY-01 | @handler decorator for registering topic handlers | Python Handler API |
| PY-02 | Sync Python handler support via spawn_blocking | Python Handler API |
| PY-03 | Async Python handler support via pyo3-async-runtimes | Python Handler API |
| PY-04 | ExecutionContext passed to handlers with trace context | Python Handler API |
| PY-06 | Handler concurrency configuration per handler | Python Handler API |
| CONF-05 | Context manager support (with statement for clean shutdown) | Configuration |

### Deliverables

1. **@handler Decorator API**
   - `@handler(topic="...", key=None)` decorator for registering handler functions
   - Handler registry maintained in Rust, accessible from Python
   - Batch handler variant: `@handler(topic="...", batch=True)`

2. **Sync Handler Execution**
   - Python sync functions executed via Tokio spawn_blocking
   - GIL acquired only during callback, released during I/O
   - No blocking of Tokio event loop

3. **Async Handler Execution**
   - Python async functions executed via pyo3-async-runtimes
   - Native async/await support without blocking the event loop
   - Both sync and async handlers coexist in the same consumer

4. **ExecutionContext**
   - Trace context (trace_id, span_id) propagated from W3C headers
   - Message metadata (topic, partition, offset, timestamp, headers)
   - Ack/nack methods for manual offset control

5. **Per-Handler Concurrency**
   - Concurrency limit per handler (e.g., 10 concurrent executions of the same handler)
   - Configurable via handler decorator or global config

6. **Context Manager Support**
   - `with Consumer(config) as consumer:` pattern for deterministic shutdown
   - Exits gracefully when exiting the `with` block

### Success Criteria

1. **@handler decorator works**: A Python developer can decorate a function with `@handler(topic="my-topic")` and have messages from "my-topic" routed to that function automatically.
2. **Sync and async handlers both work**: Both `def handle(msg)` and `async def handle(msg)` work correctly; sync handlers do not block the Tokio event loop.
3. **ExecutionContext carries trace context**: When a message with W3C trace headers arrives, the handler's ExecutionContext contains the trace_id and span_id for correlation.
4. **Per-handler concurrency is enforced**: When a handler has concurrency=5, no more than 5 instances of that handler run simultaneously.
5. **Context manager shuts down cleanly**: Using `with Consumer(config) as c:` ensures graceful shutdown when exiting the block, even if an exception is raised.

---

## Phase 4: Observability

**Purpose:** Add comprehensive observability with tracing instrumentation, message throughput/latency metrics, consumer lag, queue depth gauges, and DLQ volume as a first-class metric.

### Requirements Covered

| ID | Requirement | Category |
|----|-------------|----------|
| OBS-01 | tracing instrumentation with span per handler execution | Observability |
| OBS-02 | W3C trace context propagation from Kafka headers | Observability |
| OBS-03 | Message throughput metrics | Observability |
| OBS-04 | Processing latency histograms | Observability |
| OBS-05 | Consumer lag metrics | Observability |
| OBS-06 | Queue depth gauges per handler | Observability |
| OBS-07 | DLQ message volume as first-class metric | Observability |
| PY-05 | Batch handler support for high-throughput scenarios | Python Handler API |

### Deliverables

1. **Tracing Instrumentation**
   - tracing spans per handler execution
   - Span attributes: topic, partition, offset, handler_name, attempt
   - W3C trace context propagation from Kafka headers (traceparent)
   - Trace context injected into ExecutionContext for handlers

2. **Metrics**
   - Message throughput (messages/second per topic/handler)
   - Processing latency histograms (p50, p95, p99 per handler)
   - Consumer lag (messages behind high-watermark per partition)
   - Queue depth gauges (current depth per handler queue)
   - DLQ message volume (count and rate per DLQ topic)

3. **Batch Handler Support**
   - `@handler(topic="...", batch=True)` for high-throughput scenarios
   - Batches delivered to handler as list of (message, context) tuples
   - Batch-level ack/nack (commits offsets for entire batch)

### Success Criteria

1. **Traces are span-per-handler**: Each handler invocation creates a tracing span with topic, partition, offset, and handler name as attributes; spans are exported via the configured exporter.
2. **W3C trace context is propagated**: A traceparent header in the Kafka message is parsed and the trace context is continued in the handler span; trace_id is visible in handler logs.
3. **Metrics are queryable**: Prometheus metrics or an equivalent metrics endpoint exposes message throughput, latency histograms, consumer lag, queue depth, and DLQ volume; values update in real-time.
4. **Batch handlers process efficiently**: A batch handler receiving 100 messages processes them as a batch, with latency measured from first message pickup to batch completion; offsets are committed for the entire batch atomically.

---

## Phase Summary

| Phase | Focus | Requirements | Key Outcome |
|-------|-------|--------------|-------------|
| Phase 1 | Core Consumer Engine & Configuration | 16 | Working single-handler consumer with bounded queues and config |
| Phase 2 | Rebalance, Failure Handling & Lifecycle | 18 | Production-ready: rebalance-safe, retry/DLQ, graceful shutdown |
| Phase 3 | Python Handler API | 6 | Developer ergonomics: @handler, sync/async, ExecutionContext |
| Phase 4 | Observability | 8 | Full observability: tracing, metrics, batch handlers |

---

## Appendix: Requirement Traceability

| Requirement | Phase |
|-------------|-------|
| CORE-01 | Phase 1 |
| CORE-02 | Phase 1 |
| CORE-03 | Phase 1 |
| CORE-04 | Phase 1 |
| CORE-05 | Phase 1 |
| CORE-06 | Phase 2 |
| CORE-07 | Phase 1 |
| CORE-08 | Phase 2 |
| MSG-01 | Phase 1 |
| MSG-02 | Phase 1 |
| MSG-03 | Phase 2 |
| MSG-04 | Phase 1 |
| MSG-05 | Phase 1 |
| PY-01 | Phase 3 |
| PY-02 | Phase 3 |
| PY-03 | Phase 3 |
| PY-04 | Phase 3 |
| PY-05 | Phase 4 |
| PY-06 | Phase 3 |
| OFF-01 | Phase 1 |
| OFF-02 | Phase 1 |
| OFF-03 | Phase 1 |
| OFF-04 | Phase 2 |
| FAIL-01 | Phase 2 |
| FAIL-02 | Phase 2 |
| FAIL-03 | Phase 2 |
| FAIL-04 | Phase 2 |
| FAIL-05 | Phase 2 |
| FAIL-06 | Phase 2 |
| FAIL-07 | Phase 2 |
| LIFE-01 | Phase 2 |
| LIFE-02 | Phase 2 |
| LIFE-03 | Phase 2 |
| LIFE-04 | Phase 2 |
| LIFE-05 | Phase 2 |
| CONF-01 | Phase 1 |
| CONF-02 | Phase 1 |
| CONF-03 | Phase 2 |
| CONF-04 | Phase 1 |
| CONF-05 | Phase 3 |
| OBS-01 | Phase 4 |
| OBS-02 | Phase 4 |
| OBS-03 | Phase 4 |
| OBS-04 | Phase 4 |
| OBS-05 | Phase 4 |
| OBS-06 | Phase 4 |
| OBS-07 | Phase 4 |

**Coverage:** 45/45 requirements mapped to phases (100%)

---

*Last updated: 2026-04-28*
