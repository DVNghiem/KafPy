---
gsd_state_version: 1.0
milestone: v2.0
milestone_name: Code Quality Refactor
status: planning
stopped_at: Milestone v2.0 roadmap created
last_updated: "2026-04-20T12:00:00.000Z"
last_activity: 2026-04-20 — Milestone v2.0 roadmap created
progress:
  total_phases: 6
  completed_phases: 0
  total_plans: 0
  completed_plans: 0
  percent: 0
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-04-20)

**Core value:** High-performance Rust Kafka client with idiomatic Python API
**Current focus:** v2.0 — Code Quality Refactor

## Current Position

Milestone: v2.0 (roadmap created)
Phase: Not started
Plan: —
Status: Ready for Phase 1 planning
Last activity: 2026-04-20 — Milestone v2.0 roadmap created

## Performance Metrics

**Velocity:**

- Total phases completed: 43
- Total milestones: 9 (v1.0 through v1.9 shipped)
- v2.0 phases: 6 (just planned)

**By Milestone:**

| Milestone | Phases | Status |
|-----------|--------|--------|
| v1.0 | 5 | Shipped 2026-04-15 |
| v1.1 | 3 | Shipped 2026-04-16 |
| v1.2 | 2 | Shipped 2026-04-16 |
| v1.3 | 6 | Shipped 2026-04-17 |
| v1.4 | 4 | Shipped 2026-04-17 |
| v1.5 | 3 | Shipped 2026-04-18 |
| v1.6 | 4 | Shipped 2026-04-18 |
| v1.7 | 5 | Shipped 2026-04-18 |
| v1.8 | 5 | Shipped 2026-04-20 |
| v1.9 | 6 | Shipped 2026-04-20 |
| v2.0 | 6 | In progress |

## Accumulated Context

### Key Architectural Decisions (preserved across refactor)

- Rust core / Python business logic — performance + idiomatic bindings
- rdkafka for Kafka protocol — battle-tested, async-capable
- Tokio for async runtime — native rdkafka compat, mpsc channels
- PyO3-free consumer core — clean separation, testable without Python
- Per-topic bounded queue dispatch — isolated backpressure per topic
- Highest contiguous offset commit — only commit when all prior offsets acked
- store_offset + commit coordination — at-least-once delivery guarantee

### Refactoring Principles

- Smaller focused modules over god objects
- Clear boundaries (no leakage between layers)
- Less duplication (DRY)
- Explicit state models over boolean flags
- Cleaner naming (descriptive, consistent)
- Lower coupling, high cohesion
- Stable public API (no behavior changes)

### Critical Pitfalls (from research)

1. **Send+Sync Guarantees** — Use `fn assert_send_sync<T: Send + Sync>()` compile-time checks
2. **Channel Semantic Changes** — Never change mpsc capacity without documenting backpressure impact
3. **PyO3 GIL Boundary** — All Python invocations must go through `spawn_blocking` or `PythonAsyncFuture`
4. **Shutdown Ordering** — Maintain `biased` directive on `select!` in `ConsumerRunner::run()`
5. **Offset Commit State Machine** — `has_terminal` gating must remain functional after split

### Open Questions

- HandlerId vs topic distinction: Are they always equal or conceptually distinct?
- NoopSink duplication: Is worker_pool NoopSink identical to observability NoopSink?
- PartitionAccumulator naming: Rename to PerPartitionBuffer?

## Deferred Items

| Category | Item | Status | Deferred At |
|----------|------|--------|-------------|
| Advanced rebalance | Rebalance interfaces | Deferred | v1.0 |
| Schema registry | Avro support | Deferred | v1.0 |
| Java/Node.js bindings | Python only | Deferred | v1.0 |
| Multi-consumer groups | Multiple consumer group support | Deferred | v2.0+ |
| High-throughput producer | Producer optimizations | Deferred | v2.0+ |
| Admin client | Kafka topic/partition administration | Deferred | v2.0+ |
| Stream processing | Kafka Streams-style operations | Deferred | v2.0+ |

## Session Continuity

Last session: 2026-04-20T12:00:00.000Z
Stopped at: Milestone v2.0 roadmap created
Resume file: .planning/milestones/v2.0-STATE.md
