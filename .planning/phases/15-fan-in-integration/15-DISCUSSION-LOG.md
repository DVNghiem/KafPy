# Phase 15: Fan-In Integration - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-05-01
**Phase:** 15-fan-in-integration
**Areas discussed:** Per-source backpressure, register_fanin API, OBSV-03 metrics
**Mode:** auto (no interactive discuss-phase — decisions derived from prior phases and codebase analysis)

---

## Per-Source Backpressure (FANIN-04)

| Option | Description | Selected |
|--------|-------------|----------|
| BackpressureAction::PausePartition per-topic | Already implemented in backpressure.rs, PauseOnFullPolicy routes per topic | ✓ (D-01) |
| Global fan-in handler pause | Would pause all sources when one is slow | Not selected |

**User's choice:** Auto — decisions derived from existing code patterns (D-01 through D-05)
**Notes:** Source topic available from ExecutionContext.source_topic (Phase 14). Queue depth is shared across all sources via the fan-in handler key. Pause targets the specific Kafka topic, not the handler.

## register_fanin API (FANIN-05)

| Option | Description | Selected |
|--------|-------------|----------|
| Builder pattern consistent with register_fanout | FanInBuilderRust::new() → register_into_consumer() → FanInRegistration | ✓ (D-06 through D-08) |
| Direct handler registration | Simpler but less extensible | Not selected |

**User's choice:** Auto — builder pattern (D-06 through D-08)
**Notes:** Consistent with Phase 13 FanOutRegistration. FanInBuilderRust and FanInRegistration already exist in fan_in_bridge.rs.

## OBSV-03 Per-Source Consumer Lag Metrics

| Option | Description | Selected |
|--------|-------------|----------|
| Topic label on consumer_lag metric | Distinguishes per-source lag in fan-in scenarios | ✓ (D-09) |
| Partition label in addition to topic | More granular but higher cardinality | Not selected |

**User's choice:** Auto — topic label (D-09)
**Notes:** Fan-in merges multiple sources; topic label is essential for diagnosing which source is lagging.

## Claude's Discretion

- Exact queue depth threshold values — planner decides
- Per-partition vs per-topic lag granularity — planner decides

## Deferred Ideas

None — decisions stayed within phase scope.

---

*Phase: 15-fan-in-integration*
*Discussion logged: 2026-05-01*
