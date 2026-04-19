---
gsd_state_version: 1.0
milestone: v1.8
milestone_name: Graceful Shutdown & Rebalance Handling
status: Ready for Phase 34 (Rebalance Handling)
stopped_at: Phase 34 context gathered, ready for planning
last_updated: "2026-04-19T16:17:01.375Z"
last_activity: 2026-04-19 — Phase 33 complete
progress:
  total_phases: 1
  completed_phases: 1
  total_plans: 1
  completed_plans: 1
  percent: 100
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-04-19)

**Core value:** High-performance Rust Kafka client with idiomatic Python API
**Current focus:** Phase 33 (defining requirements)

## Current Position

Phase: 33 complete (ShutdownCoordinator shipped)
Plan: 33-01 complete
Status: Ready for Phase 34 (Rebalance Handling)
Last activity: 2026-04-19 — Phase 33 complete

## Performance Metrics

**Velocity:**

- Total plans completed: 23
- Total milestones: 8 (including v1.7)

**By Milestone:**

| Milestone | Phases | Plans |
|-----------|--------|-------|
| v1.0 | 5 | — |
| v1.1 | 3 | — |
| v1.2 | 2 | — |
| v1.3 | 6 | — |
| v1.4 | 4 | 8 |
| v1.5 | 3 | 5 |
| v1.6 | 4 | 7 |
| v1.7 | 5 | 7 |
| v1.8 | TBD | TBD |

## Accumulated Context

### Decisions

- **v1.4**: RetryCoordinator 3-tuple (should_retry, should_dlq, delay)
- **v1.4**: has_terminal per-partition gating (once terminal, blocks commit for that partition)
- **v1.4**: fire-and-forget DLQ produce (bounded mpsc channel ~100)
- **v1.4**: configurable DLQ topic naming (dlq_topic_prefix, default "dlq.")
- **v1.5**: Routing precedence: pattern → header → key → python → default
- **v1.5**: Rust is fast-path owner; Python routing is optional fallback only
- **v1.5**: RoutingDecision: Route(handler_id), Drop, Reject(reason), Defer
- **v1.5**: No payload copies in routing path
- **v1.5**: RoutingChain chains routers with precedence enforcement
- **v1.6**: HandlerMode enum (SingleSync, SingleAsync, BatchSync, BatchAsync) as gating abstraction
- **v1.6**: BatchAccumulator with fixed-window timeout (not sliding)
- **v1.6**: pyo3-async-runtimes into_future for async Python handlers
- **v1.6**: BatchExecutionResult::AllSuccess/AllFailure/PartialFailure
- **v1.6**: GIL never held across Rust-side orchestration
- **v1.7**: metrics crate facade (zero-cost when no recorder installed)
- **v1.7**: KafPy never calls set_global_default() or set_global_recorder()
- **v1.7**: MetricLabels enforces lexicographically sorted label ordering
- **v1.7**: Span context propagates via W3C tracecontext headers at spawn_blocking boundary
- **v1.7**: Kafka metrics polling-based (not per-message) to avoid hot-path overhead
- **v1.7**: RuntimeSnapshot zero-cost when not called (no atomic updates on hot path)
- **v1.8**: Explicit lifecycle states over boolean flags
- **v1.8**: Close-and-drain over abrupt drops
- **v1.8**: Partition ownership state: assigned / paused / draining / revoked
- **v1.8**: ShutdownCoordinator owns ShutdownPhase enum (Running -> Draining -> Finalizing -> Done)
- **v1.8**: Shutdown order: dispatcher stop -> worker drain (30s timeout) -> offset finalize -> consumer drop
- **v1.8**: rd_kafka_consumer_close() auto-called by BaseConsumer::Drop, never explicit
- **v1.8**: tokio::time::timeout(drain_timeout, join_set.shutdown()) with abort_all() on timeout

### Pending Todos

- Phase 28: MetricsSink trait, HandlerMetrics, Prometheus adapter, MetricLabels, zero-cost facade
- Phase 29: OTLP exporter, span wrapping, W3C context propagation, ObservabilityConfig
- Phase 30: Consumer lag, assignment size, committed offset gauges via rdkafka stats
- Phase 31: RuntimeSnapshot, get_runtime_snapshot() PyO3, Consumer.status() Python method
- Phase 32: Consistent field names, log format config, per-component log levels, LogTracer
- Phase 33: Graceful Shutdown Coordinator, lifecycle state machine, shutdown signal handling **[DONE]**
- Phase 34: Rebalance Handling, partition ownership state, Assign/Revoke/Error events
- Phase 35: Integration - offset manager, retry/DLQ, worker, batching, observability wired to lifecycle

### Blockers/Concerns

- PyO3 linking error in test binary (pre-existing)

## Deferred Items

| Category | Item | Status | Deferred At |
|----------|------|--------|-------------|
| Advanced rebalance | Rebalance interfaces | Deferred | v1.0 |
| Schema registry | Avro support | Deferred | v1.0 |
| Content-based routing | Payload parsing | Python fallback only | v1.5 |
| Multi-handler fan-out | Single handler per message | Deferred | v1.5 |
| PartialFailure tracking | Per-message outcome within batch | v1.7+ | v1.6 |
| Cross-partition batch | Batch aggregation across partitions | Deferred | v1.6 |
| Sliding window batch | Sliding window batch timeout | Deferred | v1.6 |
| Async Python event loop | Event loop lifecycle management | Deferred | v1.6 |
| Streaming batch | Batch handlers as generators | Deferred | v1.6 |
| OTLP exporter sink | Full OTLP protocol metrics export | v1.8+ | v1.7 |
| Alerting rules library | Pre-built Prometheus alerting rules | v1.8+ | v1.7 |
| Trace context into Kafka headers | Cross-service correlation | v1.8+ | v1.7 |
| Python-side tracing | Python tracing conventions differ | v1.8+ | v1.7 |
| Sliding window latency | p50, p95, p99 percentiles | v1.8+ | v1.7 |
| OTLP exporter sink | Full OTLP protocol metrics export | v1.9+ | v1.8 |
| Alerting rules library | Pre-built Prometheus alerting rules | v1.9+ | v1.8 |
| Trace context into Kafka headers | Cross-service correlation | v1.9+ | v1.8 |
| Sliding window latency | p50, p95, p99 percentiles | v1.9+ | v1.8 |

## Session Continuity

Last session: 2026-04-19T16:17:01.372Z
Stopped at: Phase 34 context gathered, ready for planning
Resume file: .planning/phases/34-rebalance-handling/34-CONTEXT.md
