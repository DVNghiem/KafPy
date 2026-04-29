---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: milestone
status: executing
last_updated: "2026-04-29T06:34:08.500Z"
last_activity: 2026-04-29 -- Phase 06 execution started
progress:
  total_phases: 6
  completed_phases: 4
  total_plans: 8
  completed_plans: 12
  percent: 100
---

# KafPy Project State

**Project:** Rust-Core, Python-Logic Kafka Consumer Framework
**Last Updated:** 2026-04-29

---

## Current Position

Phase: 06 (hardening) — EXECUTING
Plan: 1 of 2
Status: Executing Phase 06
Last activity: 2026-04-29 -- Phase 06 execution started

---

## Overall Status

| Dimension | Status | Notes |
|-----------|--------|-------|
| Requirements | 45 v1 defined | 100% mapped to phases |
| Phases | 4 defined | Standard granularity |
| Code | Brownfield | Existing early implementation needs verification |
| Tests | Minimal | No comprehensive test suite yet |
| Documentation | Partial | Architecture docs in .planning/research/ |

---

## Phase Status

### Phase 1: Core Consumer Engine & Configuration

| Item | Status |
|------|--------|
| Requirements | 16 (CORE-01, CORE-02, CORE-03, CORE-04, CORE-05, CORE-07, MSG-01, MSG-02, MSG-04, MSG-05, OFF-01, OFF-02, OFF-03, CONF-01, CONF-02, CONF-04) |
| Deliverables | Not started |
| Tests | Not started |
| Blocked By | None |

### Phase 2: Rebalance, Failure Handling & Lifecycle

| Item | Status |
|------|--------|
| Requirements | 18 (CORE-06, CORE-08, MSG-03, OFF-04, FAIL-01, FAIL-02, FAIL-03, FAIL-04, FAIL-05, FAIL-06, FAIL-07, LIFE-01, LIFE-02, LIFE-03, LIFE-04, LIFE-05, CONF-03) |
| Deliverables | Not started |
| Tests | Not started |
| Blocked By | Phase 1 |

### Phase 3: Python Handler API

| Item | Status |
|------|--------|
| Requirements | 6 (PY-01, PY-02, PY-03, PY-04, PY-06, CONF-05) |
| Deliverables | Not started |
| Tests | Not started |
| Blocked By | Phase 1 |

### Phase 4: Observability

| Item | Status |
|------|--------|
| Requirements | 8 (OBS-01, OBS-02, OBS-03, OBS-04, OBS-05, OBS-06, OBS-07, PY-05) |
| Deliverables | Not started |
| Tests | Not started |
| Blocked By | Phase 2, Phase 3 |

---

## Requirement Completion Matrix

### Phase 1 (16 requirements)

| ID | Requirement | Status | Notes |
|----|-------------|--------|-------|
| CORE-01 | Rust-based Kafka consumer engine | Not Started | |
| CORE-02 | Consumer groups via rdkafka | Not Started | |
| CORE-03 | Topic subscription by name and regex | Not Started | |
| CORE-04 | Manual offset commit | Not Started | |
| CORE-05 | Auto offset commit fallback | Not Started | |
| CORE-07 | Graceful start/stop | Not Started | |
| MSG-01 | Message deserialization | Not Started | |
| MSG-02 | Headers, timestamp access | Not Started | |
| MSG-04 | Per-handler bounded queues | Not Started | |
| MSG-05 | Backpressure propagation | Not Started | |
| OFF-01 | Partition-aware offset tracking | Not Started | |
| OFF-02 | Contiguous offset commit | Not Started | |
| OFF-03 | Explicit store_offset + commit | Not Started | |
| CONF-01 | ConsumerConfig dataclass | Not Started | |
| CONF-02 | Kafka client config mapping | Not Started | |
| CONF-04 | BackpressurePolicy trait | Not Started | |

### Phase 2 (18 requirements)

| ID | Requirement | Status | Notes |
|----|-------------|--------|-------|
| CORE-06 | Rebalance listener | Not Started | |
| CORE-08 | Pause/resume partitions | Not Started | |
| MSG-03 | Key-based routing | Not Started | |
| OFF-04 | Terminal failure blocking | Not Started | |
| FAIL-01 | Failure classification | Not Started | |
| FAIL-02 | Default failure classifier | Not Started | |
| FAIL-03 | Capped exponential backoff with jitter | Not Started | |
| FAIL-04 | Maximum retry attempts | Not Started | |
| FAIL-05 | DLQ handoff | Not Started | |
| FAIL-06 | DLQ metadata envelope | Not Started | |
| FAIL-07 | Default DLQ router | Not Started | |
| LIFE-01 | Graceful shutdown with drain timeout | Not Started | |
| LIFE-02 | Queue draining before shutdown | Not Started | |
| LIFE-03 | Failed messages flushed to DLQ | Not Started | |
| LIFE-04 | SIGTERM handling | Not Started | |
| LIFE-05 | Rebalance-safe partition handling | Not Started | |
| CONF-03 | RetryConfig | Not Started | |

### Phase 3 (6 requirements)

| ID | Requirement | Status | Notes |
|----|-------------|--------|-------|
| PY-01 | @handler decorator | Not Started | |
| PY-02 | Sync handler via spawn_blocking | Not Started | |
| PY-03 | Async handler via pyo3-async-runtimes | Not Started | |
| PY-04 | ExecutionContext with trace context | Not Started | |
| PY-06 | Per-handler concurrency config | Not Started | |
| CONF-05 | Context manager support | Not Started | |

### Phase 4 (8 requirements)

| ID | Requirement | Status | Notes |
|----|-------------|--------|-------|
| OBS-01 | tracing instrumentation | Not Started | |
| OBS-02 | W3C trace context propagation | Not Started | |
| OBS-03 | Message throughput metrics | Not Started | |
| OBS-04 | Processing latency histograms | Not Started | |
| OBS-05 | Consumer lag metrics | Not Started | |
| OBS-06 | Queue depth gauges | Not Started | |
| OBS-07 | DLQ message volume metric | Not Started | |
| PY-05 | Batch handler support | Not Started | |

---

## Open Issues

| Issue | Severity | Phase | Description |
|-------|----------|-------|-------------|
| GIL blocking Tokio event loop | Critical | 3 | Must use spawn_blocking for Python calls |
| Unbounded queue memory growth | High | 1 | Bounded channels required in Phase 1 |
| Commit reordering across partitions | High | 2 | Serialized via dedicated task |
| DLQ graveyard (no redrive) | Medium | 2 | Redrive mechanism needed |
| SIGTERM message loss | High | 2 | Graceful drain + commit before exit |

---

## v2 Requirements (Not Started)

These are deferred to a future release and do not block v1.

| ID | Requirement | Rationale |
|----|-------------|-----------|
| OBS-08 | OpenTelemetry OTLP exporter | Phase 4 exporter is sufficient for v1 |
| OBS-09 | Prometheus exporter endpoint | Covered by OBS-03 through OBS-07 |
| ADV-01 | State management with persistence | Future enhancement |
| ADV-02 | Windowed aggregations | Future enhancement |
| ADV-03 | Schema registry integration | Future enhancement |
| ADV-04 | Exactly-once semantics | At-least-once is v1 target |

---

*Last updated: 2026-04-28*

**Planned Phase:** 2 (Rebalance Handling, Failure Handling & Lifecycle) — 6 plans — 2026-04-28T18:23:34.865Z
