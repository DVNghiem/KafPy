# Feature Research

**Domain:** Observability Layer for Rust/PyO3 Kafka Consumer Framework
**Researched:** 2026-04-18
**Confidence:** MEDIUM-HIGH

## Feature Landscape

### Table Stakes (Users Expect These)

Features users assume exist in any production-grade Kafka framework. Missing these = product feels incomplete or unusable in production environments.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| **Structured logging** | Debugging production issues requires log fields, not format strings | LOW | KafPy already uses `tracing` facade. Need field-rich events at key integration points. |
| **Handler latency histogram** | Identify slow handlers, set SLAs, capacity planning | MEDIUM | Per-handler `Duration` from invoke start to result. Histogram with p50/p95/p99 percentiles. |
| **Error count per handler** | Alert on error rates, track reliability | LOW | Increment on `ExecutionResult::Error` / `ExecutionResult::Rejected`. Counter with labels. |
| **Invocation count per handler** | Throughput visibility, handler utilization | LOW | Increment on each handler call. Counter with topic/handler_id labels. |
| **Kafka consumer lag metric** | Core Kafka health indicator -- lag means backlog | MEDIUM | rdkafka provides `consumer_lag` per partition via `assignment()` + `position()`. Expose as gauge. |
| **Queue depth / inflight gauges** | Detect backpressure, identify queue buildup | LOW | `QueueManager` already tracks `queue_depth` and `inflight` via `AtomicUsize`. Expose as Prometheus gauges. |
| **Offset commit latency** | Know when commits are slow or failing | MEDIUM | Measure `store_offset` + `commit` duration. Histogram with topic/partition labels. |
| **Worker pool status introspection** | Runtime visibility: which workers are active/idle/busy | MEDIUM | `WorkerPool` already uses `JoinSet` + `CancellationToken`. Expose worker state via a status struct. |

### Differentiators (Competitive Advantage)

Features that set KafPy apart from basic Kafka clients. These align with the Core Value: high-performance Rust with idiomatic Python API.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **Pluggable metrics backend** | Users wire their own Prometheus, DataDog, OTLP. Not locked into one backend. | MEDIUM | Trait-based `MetricsSink` facade. Users provide `Arc<dyn MetricsSink>` at construction. |
| **OpenTelemetry trace hooks** | Distributed tracing across services using Kafka as transport | MEDIUM | Emit spans on handler invoke, commit, DLQ route. Zero-cost opt-in via `enable_otel()`. |
| **Per-partition batch size distribution** | Understand batching efficiency -- are batches forming as expected? | MEDIUM | Histogram of batch sizes per partition. Reveals if `max_batch_size` is too large for traffic pattern. |
| **Facade/abstraction layer** | Observability does not pull in Prometheus OTEL crates unless user opts in | LOW | Core emits to trait. Prometheus adapter is separate feature gate. |
| **Python-accessible introspection** | `consumer.status()` returns dict with worker states, queue depths, lag | LOW | Expose via PyO3 a status struct readable from Python for dashboarding. |

### Anti-Features (Commonly Requested, Often Problematic)

Features that seem good but create coupling or complexity problems.

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| **Auto-instrumentation with agent** | Java/Kafka-style JVM agent for zero-config tracing | Requires runtime bytecode injection, complex in Python native extensions | Explicit span hooks via OTel API -- opt-in, no agent |
| **Builtin Prometheus exporter endpoint** | `/metrics` endpoint for Prometheus scraping | HTTP server lifecycle complexity, opinionated about deployment | Expose metrics via `Arc<dyn MetricsSink>` that users wire to their own HTTP server |
| **Log aggregation in-process** | Centralized logging with buffering | Adds memory pressure, opinionated about log backend | Structured log facade outputs to `tracing`. Users configure their own subscriber (env-filter, Jaeger, etc.) |
| **Metrics on every single message** | Maximum granularity for debugging | High cardinality explosion. Histogram per message is expensive. | Aggregate in-process: per-batch or per-second rollup before emitting to sink |
| **Hardcoded DataDog/CloudWatch export** | "Just works" integration | Vendor lock-in, adds dependency weight | Adapter pattern: user provides `MetricsSink` implementation |

## Feature Dependencies

```
[Structured Logging Facade]
    └──requires──> [MetricsSink trait]
                       └──requires──> [Metrics struct definitions]

[Handler Latency Histogram]
    └──requires──> [MetricsSink trait]
    └──requires──> [Histogram bucket configuration]

[Kafka Consumer Lag]
    └──requires──> [ConsumerRunner::assignment() + position()]

[WorkerPool Status]
    └──requires──> [Worker state tracking in worker_loop]

[OpenTelemetry Hooks]
    └──enhances──> [Structured Logging Facade]
    └──conflicts──> [Metrics-only users who don't want OTel overhead]
```

### Dependency Notes

- **Structured Logging facade requires MetricsSink trait:** The `tracing` facade is already in use. Observability hooks emit to `Arc<dyn MetricsSink>` so both logging and metrics share the same pluggable backend.
- **Kafka Consumer Lag requires `ConsumerRunner::assignment()` + `position()`:** Already available on `ConsumerRunner`. rdkafka's `TopicPartitionList` provides offsets per partition.
- **OpenTelemetry hooks enhance logging facade:** OTel spans can be emitted alongside structured log events. They are orthogonal but share the hook integration points.
- **OTel conflicts with metrics-only users:** If user only wants Prometheus counters/histograms, they should not pay OTel crate cost. OTel is a feature-gated optional feature.

## MVP Definition

### Launch With (v1.7)

Minimum viable observability -- enough for users to run KafPy in production with visibility.

- [ ] **Structured logging via `tracing`** -- Enrich existing `tracing::info/warn/error` calls with fields (topic, partition, handler_id, worker_id, offset, queue_depth). Already uses `tracing-subscriber` with env-filter.
- [ ] **`MetricsSink` trait** -- Define `pub trait MetricsSink: Send + Sync`. Methods: `record_counter(name, labels)`, `record_histogram(name, value, labels)`, `record_gauge(name, value, labels)`. Minimal interface.
- [ ] **`noop` sink implementation** -- Default no-op sink that drops all metrics. Zero cost when observability disabled.
- [ ] **Handler invocation counter** -- Increment on each `PythonHandler::invoke_mode` call. Labels: `handler_id`, `topic`, `mode` (SingleSync/BatchSync/etc).
- [ ] **Handler latency histogram** -- Time from invoke start to `ExecutionResult` available. Buckets: [5ms, 10ms, 25ms, 50ms, 100ms, 250ms, 500ms, 1s, 2.5s, 5s]. Labels: `handler_id`, `topic`, `mode`.
- [ ] **Error counter per handler** -- Increment on `ExecutionResult::Error` and `ExecutionResult::Rejected`. Labels: `handler_id`, `topic`, `error_type` (Error/Rejected).
- [ ] **Batch size histogram** -- Record batch size when `BatchAccumulator` flushes. Buckets: [1, 2, 5, 10, 25, 50, 100]. Labels: `handler_id`, `topic`.
- [ ] **Queue depth gauges** -- Expose `queue_manager.get_queue_depth()` and `queue_manager.get_inflight()` per handler. Labels: `handler_id`, `topic`.
- [ ] **Python `status()` method** -- `Consumer.status()` returns dict: `{handlers: {handler_id: {queue_depth, inflight, capacity}}, workers: {worker_id: state}}`. Enables Python-side dashboards.

### Add After Validation (v1.8)

Features that add significant value once the observability foundation is stable.

- [ ] **Prometheus sink adapter** -- `kafpy_observability::PrometheusSink` implementing `MetricsSink`. Uses `prometheus-client` crate. Users wire it via config.
- [ ] **Kafka consumer lag gauge** -- Use rdkafka `consumer.position()` vs `consumer.highwater()` to compute lag. Expose as gauge per topic-partition.
- [ ] **Offset commit latency histogram** -- Time `store_offset` + `commit` cycle. Labels: `topic`, `partition`.
- [ ] **OpenTelemetry trace hooks** -- `enable_otel()` method on `ConsumerConfig`. Emits spans on: message received, handler invoked, execution result, offset committed, DLQ routed. Zero-cost when disabled.
- [ ] **Worker state introspection** -- `Idle/Active/Busy` per worker. `Active` = currently processing a message. `Busy` = backpressure blocked. Exposed via Python `status()`.

### Future Consideration (v2+)

Features to defer until product-market fit is established.

- [ ] **OTLP exporter sink** -- Export metrics/traces via OpenTelemetry Protocol to Jaeger/Prometheus OTLP gateway.
- [ ] **Alerting rules library** -- Pre-built Prometheus alerting rules for: high error rate, consumer lag exceeds threshold, queue depth approaching capacity.
- [ ] **Trace context propagation** -- Inject trace context into Kafka message headers for cross-service tracing.
- [ ] **Adaptive batch sizing hints** -- Based on observed batch sizes, suggest `max_batch_size` tuning.

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| Structured logging (enrich existing) | HIGH | LOW | P1 |
| `MetricsSink` trait + noop | HIGH | LOW | P1 |
| Handler invocation counter | HIGH | LOW | P1 |
| Handler latency histogram | HIGH | MEDIUM | P1 |
| Error counter per handler | HIGH | LOW | P1 |
| Python `status()` method | HIGH | LOW | P1 |
| Queue depth gauges | MEDIUM | LOW | P1 |
| Batch size histogram | MEDIUM | LOW | P1 |
| Prometheus sink adapter | HIGH | MEDIUM | P2 |
| Kafka consumer lag gauge | HIGH | MEDIUM | P2 |
| Offset commit latency histogram | MEDIUM | MEDIUM | P2 |
| OpenTelemetry trace hooks | MEDIUM | HIGH | P2 |
| Worker state introspection | MEDIUM | MEDIUM | P2 |
| OTLP exporter sink | LOW | HIGH | P3 |
| Alerting rules library | LOW | MEDIUM | P3 |
| Trace context propagation | LOW | HIGH | P3 |

**Priority key:**
- P1: Must have for launch -- observability foundation
- P2: Should have, add when possible -- production hardening
- P3: Nice to have, future consideration -- deferred

## Competitor Feature Analysis

| Feature | kafka-python | confluent-kafka-python | faust (Bytewax) | Our Approach |
|---------|--------------|------------------------|-----------------|--------------|
| Structured logging | `logging` module (stdlib) | `logging` module | `structlog` + `logging` | `tracing` facade with field-rich events |
| Metrics | No builtin | Optional `confluent_kafka_monitoring` | Prometheus client | `MetricsSink` trait -- pluggable, zero-cost noop default |
| Consumer lag | Manual calculation | Manual calculation | Via `consumer_group` metric | Built-in gauge via rdkafka `position()` |
| Per-handler latency | No | No | Via Faust instrumentation | First-class histogram per registered handler |
| OpenTelemetry | Third-party | Third-party | Built-in OTEL | Opt-in OTel hooks via `enable_otel()` |
| Runtime introspection | No | No | `faust --inspect` | Python `status()` dict with worker + queue state |
| Batch metrics | No | No | No | Batch size histogram on accumulator flush |

**Key differentiation:** KafPy's observability layer is facade-based (not bound to a specific backend), first-class (not an afterthought), and Python-accessible (not only Rust-visible).

## Sources

- [rdkafka STATISTICS documentation](https://github.com/confluentinc/librdkafka/blob/master/STATISTICS.md) -- rdkafka exposes consumer lag and throughput metrics via JSON statistics callback
- [tracing crate documentation](https://docs.rs/tracing/) -- structured logging facade, already in use in KafPy
- [prometheus-client Rust crate](https://docs.rs/prometheus-client/) -- idiomatic Prometheus metrics in Rust
- [OpenTelemetry Rust SDK](https://docs.rs/opentelemetry/) -- OTEL trace/span API, used for trace hook integration
- KafPy existing implementation -- `src/worker_pool/mod.rs`, `src/dispatcher/queue_manager.rs`, `src/coordinator/offset_tracker.rs`

---

*Feature research for: observability layer (structured logging, metrics, tracing, introspection)*
*Researched: 2026-04-18*
