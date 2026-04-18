# Roadmap: KafPy — v1.7 Observability Layer

## Milestones

- [x] **v1.0 Core Consumer Refactor** — Phases 1-5 (shipped 2026-04-15)
- [x] **v1.1 Dispatcher Layer** — Phases 6-8 (shipped 2026-04-16)
- [x] **v1.2 Python Execution Lane** — Phases 9-10 (shipped 2026-04-16)
- [x] **v1.3 Offset Commit Coordinator** — Phases 11-16 (shipped 2026-04-17)
- [x] **v1.4 Failure Handling & DLQ** — Phases 17-20 (shipped 2026-04-17)
- [x] **v1.5 Extensible Routing** — Phases 21-23 (shipped 2026-04-18)
- [x] **v1.6 Execution Modes** — Phases 24-27 (shipped 2026-04-18)
- [ ] **v1.7 Observability Layer** — Phases 28-32

## Phases

- [ ] **Phase 28: Metrics Infrastructure** — MetricsSink trait, HandlerMetrics, Prometheus adapter, zero-cost facade
- [ ] **Phase 29: Tracing Infrastructure** — OTLP exporter, span wrapping, W3C context propagation
- [ ] **Phase 30: Kafka-Level Metrics** — Consumer lag, assignment size, committed offset gauges
- [ ] **Phase 31: Runtime Introspection** — RuntimeSnapshot, Python status API, zero-cost when not called
- [ ] **Phase 32: Structured Logging** — Consistent field names, log format config, per-component log levels

---

## Phase Details

### Phase 28: Metrics Infrastructure
**Goal**: KafPy exposes metrics infrastructure via MetricsSink trait with zero-cost facade when no recorder installed
**Depends on**: Nothing (first phase of milestone)
**Requirements**: OBS-01, OBS-02, OBS-03, OBS-04, OBS-05, OBS-06, OBS-07, OBS-08, OBS-09, OBS-10
**Success Criteria** (what must be TRUE):
  1. User can implement MetricsSink trait and register a custom backend (Prometheus, DataDog, OTLP); default noop implementation provided
  2. Handler invocation counter is recorded on every invoke_mode call with handler_id, topic, mode labels
  3. Handler latency histogram records Duration from invoke start to ExecutionResult with configurable bucket boundaries
  4. Error counter records on ExecutionResult::Error and RoutingDecision::Reject with handler_id, error_type labels
  5. QueueManager.queue_snapshots() returns atomic queue_depth and inflight per handler for periodic gauge updates
  6. BatchAccumulator records batch size histogram per-partition flush size distribution
  7. PrometheusMetricsSink implements MetricsSink using prometheus-client crate, registered via builder pattern
  8. MetricLabels type enforces lexicographically sorted label ordering to prevent cardinality explosion
  9. Metrics are zero-cost when no recorder is installed — facade silently drops all recordings
**Plans**: TBD

### Phase 29: Tracing Infrastructure
**Goal**: OpenTelemetry tracing integration with proper span context propagation across PyO3 GIL boundary
**Depends on**: Phase 28
**Requirements**: OBS-11, OBS-12, OBS-13, OBS-14, OBS-15, OBS-16, OBS-17, OBS-18, OBS-19
**Success Criteria** (what must be TRUE):
  1. User can call enable_otel_tracing() to set up OTLP exporter and tracing-opentelemetry Layer, returning LayerHandle user owns
  2. KafPy never calls set_global_default() or set_global_recorder() — user owns all global state
  3. worker_loop wraps each invoke_mode call in span named "kafpy.handler.invoke" with handler_id, topic, partition, offset, mode fields
  4. ConsumerDispatcher::run wraps dispatch in span named "kafpy.dispatch.process" with topic, partition, offset, routing_decision fields
  5. DLQ routing emits span named "kafpy.dlq.route" with handler_id, reason, partition fields
  6. Span context propagates across PyO3 GIL boundary via explicit W3C tracecontext header injection/extraction at spawn_blocking call sites
  7. Async span patterns use #[instrument] or span.in_scope() — never Span::enter() across await points
  8. ObservabilityConfig struct allows user to configure OTLP endpoint, service name, and sampling rate
  9. Span naming follows kafpy.{component}.{operation} convention
**Plans**: TBD

### Phase 30: Kafka-Level Metrics
**Goal**: Consumer lag, assignment size, committed offset metrics via rdkafka statistics callback
**Depends on**: Phase 29
**Requirements**: OBS-20, OBS-21, OBS-22, OBS-23, OBS-24, OBS-25, OBS-26
**Success Criteria** (what must be TRUE):
  1. KafkaMetrics struct exposes consumer_lag gauge, assignment_size gauge, committed_offset gauge per topic-partition
  2. Consumer lag calculated as highwater - position per partition using rdkafka TopicPartitionList API (no extra Kafka API calls)
  3. Background polling task updates Kafka gauges every 10s (configurable) without adding per-message hot-path overhead
  4. OffsetTracker::offset_snapshots() returns current committed offset per topic-partition for gauge reporting
  5. rdkafka statistics callback thread-safe — stats shared between rdkafka internal thread and Tokio runtime via Arc<AtomicU64> snapshots
  6. offset_commit_latency histogram records time from offset advancement to commit acknowledgment
  7. Assignment change counter emits event when topic-partition assignment changes via rebalance callback
**Plans**: TBD

### Phase 31: Runtime Introspection
**Goal**: Python-accessible runtime introspection via RuntimeSnapshot without hot-path overhead
**Depends on**: Phase 30
**Requirements**: OBS-27, OBS-28, OBS-29, OBS-30, OBS-31, OBS-32
**Success Criteria** (what must be TRUE):
  1. RuntimeSnapshot struct holds worker_pool status (idle/active/busy worker counts), queue depths per handler, accumulator states, consumer_lag_summary
  2. get_runtime_snapshot() PyO3 function returns RuntimeSnapshot as Python dict for use in Python-side dashboards
  3. Consumer.status() Python method returns structured dict with worker_states, queue_depths, accumulator_info, consumer_lag_summary
  4. Introspection API is zero-cost when not called — no atomic updates on hot path
  5. worker_loop status includes current handler_id being processed and per-partition accumulator depth
  6. Python can register a status_callback invoked on every status snapshot (opt-in, not default)
**Plans**: TBD

### Phase 32: Structured Logging
**Goal**: Consistent structured log field names across logging, metrics, and tracing signals
**Depends on**: Phase 31
**Requirements**: OBS-33, OBS-34, OBS-35, OBS-36, OBS-37, OBS-38
**Success Criteria** (what must be TRUE):
  1. All structured log events use consistent field names across logging, metrics, and tracing signals
  2. ObservabilityConfig accepts log format option (json / pretty / simple) used by tracing-subscriber
  3. Log events include handler_id, topic, partition, offset as standard fields whenever applicable
  4. worker_loop emits structured log::info! / log::error! events at handler invoke start, completion, and error with consistent field schema
  5. Log level configurable per component (worker_loop, dispatcher, accumulator) independently via ObservabilityConfig
  6. Python handler log messages (via Python logging module) forwarded to Rust tracing via tracing_log::LogTracer integration
**Plans**: TBD

---

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. Foundation | v1.0 | - | Complete | 2026-04-15 |
| 6. Dispatcher Layer | v1.1 | - | Complete | 2026-04-16 |
| 9. Python Execution Lane | v1.2 | - | Complete | 2026-04-16 |
| 11. Offset Commit Coordinator | v1.3 | - | Complete | 2026-04-17 |
| 17. Failure Handling & DLQ | v1.4 | - | Complete | 2026-04-17 |
| 21. Routing Core | v1.5 | 3/3 | Complete | 2026-04-17 |
| 22. Python Integration | v1.5 | 1/1 | Complete | 2026-04-18 |
| 23. Dispatcher Integration | v1.5 | 1/1 | Complete | 2026-04-18 |
| 24. HandlerMode & Execution Foundation | v1.6 | 1/1 | Complete | 2026-04-18 |
| 25. Batch Accumulation & Flush | v1.6 | 3/3 | Complete | 2026-04-18 |
| 26. Async Python Handlers | v1.6 | 2/2 | Complete | 2026-04-18 |
| 27. Shutdown Drain & Polish | v1.6 | 1/1 | Complete | 2026-04-18 |
| 28. Metrics Infrastructure | v1.7 | 0/TBD | Not started | - |
| 29. Tracing Infrastructure | v1.7 | 0/TBD | Not started | - |
| 30. Kafka-Level Metrics | v1.7 | 0/TBD | Not started | - |
| 31. Runtime Introspection | v1.7 | 0/TBD | Not started | - |
| 32. Structured Logging | v1.7 | 0/TBD | Not started | - |

---

*Last updated: 2026-04-18*
