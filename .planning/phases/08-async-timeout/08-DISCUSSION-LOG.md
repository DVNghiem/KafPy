# Phase 8: Async Timeout - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-04-29
**Phase:** 08-async-timeout
**Areas discussed:** Timeout Mechanism, DLQ Metadata, Prometheus Metric

---

## Timeout Mechanism

| Option | Description | Selected |
|--------|-------------|----------|
| AbortHandle | Wrap handler future in AbortHandle, call abort() when timeout fires | |
| Timeout race | Timeout future that races against the handler, returns Timeout on expiry | |
| CancellationToken | Token-based cancellation, handler checks periodically | |

**User's choice:** abort() on JoinHandle (Recommended)
**Notes:** Simple, native to Tokio, no additional dependencies

---

## DLQ Metadata

| Option | Description | Selected |
|--------|-------------|----------|
| timeout_duration + last_offset | Fields: timeout_duration (u64 seconds), last_processed_offset (Option<u64>) | ✓ |
| Full timing details | Fields: timeout_duration, last_offset, time_waited_ms, attempt | |
| Minimal (timeout only) | Just timeout_duration, no offset info | |

**User's choice:** timeout_duration + last_offset (Recommended)
**Notes:** Keep envelope lean

---

## Prometheus Metric

| Option | Description | Selected |
|--------|-------------|----------|
| Counter per handler | handler_timeout_total{topic, handler_name} counter | ✓ |
| Gauge for active timeouts | Expose current wait time in metric | |
| Histogram of durations | Timeout_duration histogram per handler | |

**User's choice:** Counter per handler (Recommended)
**Notes:** No gauge or histogram — just count

---

## Claude's Discretion

- How to track active timeouts (e.g., per-message timer management) — planner decides
- JoinHandle storage and cleanup strategy — planner decides

## Deferred Ideas

None — discussion stayed within phase scope.