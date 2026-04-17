---
gsd_state_version: 1.0
milestone: v1.5
milestone_name: Extensible Routing
status: defining_requirements
stopped_at: Milestone v1.5 started
last_updated: "2026-04-17"
last_activity: 2026-04-17
progress:
  total_phases: 0
  completed_phases: 0
  total_plans: 0
  completed_plans: 0
  percent: 0
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-04-17)

**Core value:** High-performance Rust Kafka client with idiomatic Python API
**Current focus:** v1.5 — Extensible Routing

## Current Position

Phase: Not started (defining requirements)
Plan: —
Status: Defining requirements
Last activity: 2026-04-17 — Milestone v1.5 started

## Performance Metrics

**Velocity:**

- Total plans completed: 22
- Total milestones: 4

**By Milestone:**

| Milestone | Phases | Plans |
|-----------|--------|-------|
| v1.0 | 5 | — |
| v1.1 | 3 | — |
| v1.2 | 2 | — |
| v1.3 | 6 | — |
| v1.4 | 4 | 8 |

## Accumulated Context

### Decisions

- **v1.4**: RetryCoordinator 3-tuple (should_retry, should_dlq, delay)
- **v1.4**: has_terminal per-partition gating (once terminal, blocks commit for that partition)
- **v1.4**: fire-and-forget DLQ produce (bounded mpsc channel ~100)
- **v1.4**: configurable DLQ topic naming (dlq_topic_prefix, default "dlq.")
- **v1.5**: Routing precedence: pattern → header → key → python → default
- **v1.5**: Rust is fast-path owner; Python routing is optional fallback only
- **v1.5**: RoutingDecision: route, drop, reject, defer (no-route)
- **v1.5**: No payload copies in routing path

### Pending Todos

- v1.5 routing: Phase 21 (TBD)

### Blockers/Concerns

- PyO3 linking error in test binary (pre-existing)

## Deferred Items

| Category | Item | Status | Deferred At |
|----------|------|--------|-------------|
| RetryExecutor integration | Offset tracking on retry | Pending v1.5+ | v1.3 planning |
| DLQ routing | Rejected messages routing | Complete v1.4 | v1.3 planning |
| Advanced rebalance | Rebalance interfaces | Deferred | v1.0 |
| Schema registry | Avro support | Deferred | v1.0 |

## Session Continuity

Last session: 2026-04-17T22:45:00.000Z
Stopped at: Milestone v1.5 started
Resume file: None
