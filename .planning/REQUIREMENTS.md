# Milestone v1.7: Observability Layer — Requirements

**Gathered:** 2026-04-18
**Status:** Ready for roadmap

## Category: Metrics Infrastructure (OBS-01)

### Table Stakes

- [ ] **OBS-01**: KafPy exposes a `MetricsSink` trait that users implement to plug in their own metrics backend (Prometheus, DataDog, OTLP). Default noop implementation provided.
- [ ] **OBS-02**: `HandlerMetrics` struct records invocation count, latency histogram, error count, and batch size histogram per handler using the `metrics` crate facade.
- [ ] **OBS-03**: `worker_loop` and `batch_worker_loop` record handler invocation counter (labels: handler_id, topic, mode) on every `invoke_mode` call.
- [ ] **OBS-04**: `worker_loop` and `batch_worker_loop` record handler latency histogram (Duration from invoke start to ExecutionResult) with configurable bucket boundaries.
- [ ] **OBS-05**: `worker_loop` and `batch_worker_loop` record error counter (labels: handler_id, error_type) on ExecutionResult::Error and RoutingDecision::Reject.
- [ ] **OBS-06**: `QueueManager` exposes `queue_snapshots()` returning atomic queue_depth and inflight per handler for periodic gauge updates.
- [ ] **OBS-07**: `BatchAccumulator` records batch size histogram (per-partition flush size distribution).

### Differentiators

- [ ] **OBS-08**: Prometheus sink adapter (`PrometheusMetricsSink`) implements `MetricsSink` using `prometheus-client` crate, registered via builder pattern.
- [ ] **OBS-09**: MetricLabels type enforces lexicographically sorted label ordering to prevent cardinality explosion from non-deterministic label insertion order.
- [ ] **OBS-10**: Metrics are zero-cost when no recorder is installed — `metrics` facade silently drops all recordings when no recorder is set.

## Category: Tracing Infrastructure (OBS-02)

### Table Stakes

- [ ] **OBS-11**: `enable_otel_tracing()` function sets up OTLP exporter and `tracing-opentelemetry` layer as a `Layer`, returning a `LayerHandle` the user owns.
- [ ] **OBS-12**: KafPy never calls `set_global_default()` or `set_global_recorder()` — all global state is owned by the user.
- [ ] **OBS-13**: `worker_loop` wraps each `invoke_mode` call in a span named `handler.invoke` with fields: handler_id, topic, partition, offset, mode.
- [ ] **OBS-14**: `ConsumerDispatcher::run` wraps dispatch in a span named `dispatch.process` with fields: topic, partition, offset, routing_decision.
- [ ] **OBS-15**: DLQ routing emits a span named `dlq.route` with fields: handler_id, reason, partition.
- [ ] **OBS-16**: Span context propagates across the PyO3 GIL boundary via explicit W3C tracecontext header injection/extraction at `spawn_blocking` call sites.
- [ ] **OBS-17**: Async span patterns use `#[instrument]` or `span.in_scope()` — never `Span::enter()` across await points.

### Differentiators

- [ ] **OBS-18**: `ObservabilityConfig` struct allows users to configure OTLP endpoint, service name, and sampling rate at initialization.
- [ ] **OBS-19**: Span naming follows `kafpy.{component}.{operation}` convention (e.g., `kafpy.handler.invoke`, `kafpy.dlq.route`).

## Category: Kafka-Level Metrics (OBS-03)

### Table Stakes

- [ ] **OBS-20**: `KafkaMetrics` struct exposes consumer_lag gauge, assignment_size gauge, and committed_offset gauge per topic-partition.
- [ ] **OBS-21**: Consumer lag calculated as `highwater - position` per partition using rdkafka `TopicPartitionList` API (no extra Kafka API calls).
- [ ] **OBS-22**: Background polling task updates Kafka gauges every 10s (configurable) without adding per-message hot-path overhead.
- [ ] **OBS-23**: `OffsetTracker::offset_snapshots()` returns current committed offset per topic-partition for gauge reporting.
- [ ] **OBS-24**: rdkafka statistics callback integration is thread-safe — stats shared between rdkafka internal thread and Tokio runtime via `Arc<AtomicU64>` snapshots.

### Differentiators

- [ ] **OBS-25**: `offset_commit_latency` histogram records time from offset advancement to commit acknowledgment.
- [ ] **OBS-26**: Assignment change counter emits event when topic-partition assignment changes (via rebalance callback).

## Category: Runtime Introspection (OBS-04)

### Table Stakes

- [ ] **OBS-27**: `RuntimeSnapshot` struct holds worker_pool status (idle/active/busy worker counts), queue depths per handler, accumulator states, and consumer lag summary.
- [ ] **OBS-28**: `get_runtime_snapshot()` PyO3 function returns `RuntimeSnapshot` as a Python dict for use in Python-side dashboards.
- [ ] **OBS-29**: `Consumer.status()` Python method returns structured dict with: worker_states, queue_depths, accumulator_info, consumer_lag_summary.
- [ ] **OBS-30**: Introspection API is zero-cost when not called — no atomic updates on hot path.

### Differentiators

- [ ] **OBS-31**: `worker_loop` status includes current handler_id being processed and per-partition accumulator depth.
- [ ] **OBS-32**: Python can register a `status_callback` invoked on every status snapshot (opt-in, not default).

## Category: Structured Logging (OBS-05)

### Table Stakes

- [ ] **OBS-33**: All structured log events use consistent field names across logging, metrics, and tracing signals.
- [ ] **OBS-34**: `ObservabilityConfig` accepts log format option (json / pretty / simple) used by `tracing-subscriber`.
- [ ] **OBS-35**: Log events include `handler_id`, `topic`, `partition`, `offset` as standard fields whenever applicable.
- [ ] **OBS-36**: `worker_loop` emits structured `log::info!` / `log::error!` events at handler invoke start, completion, and error with consistent field schema.

### Differentiators

- [ ] **OBS-37**: Log level can be configured per component (worker_loop, dispatcher, accumulator) independently via `ObservabilityConfig`.
- [ ] **OBS-38**: Python handler log messages (via Python's logging module) can be forwarded to Rust `tracing` via `tracing_log::LogTracer` integration.

## Out of Scope

| Item | Reason |
|------|--------|
| OTLP exporter sink | Full OTLP protocol export — depends on Tracing Infrastructure (OBS-11) stable first |
| Alerting rules library | Pre-built Prometheus alerting rules — depends on Prometheus sink (OBS-08) stable first |
| Trace context injection into Kafka headers | Cross-cutting concern — deferred to v1.8 after tracing is proven |
| Python-side tracing instrumentation | Python tracing conventions differ from Rust — deferred |

## Future Requirements (v1.8+)

- **OBS-F1**: Trace context injected into Kafka message headers for cross-service correlation
- **OBS-F2**: OTLP exporter sink implementing `MetricsSink` for unified OTLP metrics + traces
- **OBS-F3**: Pre-built Prometheus alerting rules for consumer lag, error rate, batch size thresholds
- **OBS-F4**: Sliding window latency percentiles (p50, p95, p99) alongside histograms

## Traceability

| Requirement | Phase | Notes |
|-------------|-------|-------|
| OBS-01 — OBS-10 | Phase 1 | Metrics Infrastructure |
| OBS-11 — OBS-19 | Phase 2 | Tracing Infrastructure |
| OBS-20 — OBS-26 | Phase 3 | Kafka-Level Metrics |
| OBS-27 — OBS-32 | Phase 4 | Runtime Introspection |
| OBS-33 — OBS-38 | Phase 5 | Structured Logging |
| OBS-F1 — OBS-F4 | v1.8+ | Future |

---
*Requirements gathered: 2026-04-18*
