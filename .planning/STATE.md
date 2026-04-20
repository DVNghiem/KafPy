---
gsd_state_version: 1.0
milestone: v1.8
milestone_name: milestone
status: in_progress
stopped_at: v1.7 shipped — awaiting next milestone
last_updated: "2026-04-20T02:05:00.000Z"
last_activity: 2026-04-20
progress:
  total_phases: 0
  completed_phases: 0
  total_plans: 0
  completed_plans: 0
  percent: 0
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-04-18)

**Core value:** High-performance Rust Kafka client with idiomatic Python API
**Current focus:** Planning v1.8 milestone

## Current Position

Milestone: v1.8 (not started)
Status: Defining requirements
Last activity: 2026-04-20 — Milestone v1.8 started

## Performance Metrics

**Velocity:**

- Total plans completed: 23
- Total milestones: 7 (including v1.7)

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

### Pending Todos

- v1.8: TBD — milestone planning not yet started

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

## Session Continuity

Last session: 2026-04-20T02:05:00.000Z
Stopped at: v1.7 shipped — awaiting next milestone
Resume file: None
