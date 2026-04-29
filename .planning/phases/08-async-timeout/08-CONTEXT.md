# Phase 8: Async Timeout - Context

**Gathered:** 2026-04-29
**Status:** Ready for planning

<domain>
## Phase Boundary

Async handlers can be aborted after a configured timeout, with timeout metadata in DLQ and timeout metrics in Prometheus.

**Requirements:** TMOUT-01 (Python API), TMOUT-02 (DLQ metadata), TMOUT-03 (Prometheus metric)

</domain>

<decisions>
## Implementation Decisions

### Timeout Mechanism
- **D-01:** Use `abort() on JoinHandle` — Tokio's spawn returns JoinHandle, call abort() when timeout fires. Simple, native to Tokio, no additional dependencies.

### DLQ Metadata
- **D-02:** DLQ envelope includes `timeout_duration` (u64 seconds) and `last_processed_offset` (Option<u64>) when timeout triggers. No extra timing fields — keep envelope lean.

### Prometheus Metric
- **D-03:** Timeout metric is a counter per handler: `handler_timeout_total{topic, handler_name}`. No gauge or histogram — just count.

### Claude's Discretion
- How to track active timeouts (e.g., per-message timer management) — planner decides
- JoinHandle storage and cleanup strategy — planner decides

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Requirements
- `.planning/REQUIREMENTS.md` §TMOUT — TMOUT-01, TMOUT-02, TMOUT-03

### Prior Phase
- `.planning/REQUIREMENTS.md` §SYNC — Phase 7 thread pool, PythonHandler::invoke dispatch
- `.planning/phases/07-thread-pool/07-PLAN.md` — invoke() and handler timeout pattern

### Implementation
- `src/python/handler.rs` — existing `invoke_mode_with_timeout` pattern (lines ~287-315)
- `src/shutdown/shutdown.rs` — DLQ envelope construction
- `src/metrics/metrics.rs` — existing Prometheus metric registration pattern

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `invoke_mode_with_timeout` in handler.rs: existing timeout tracking pattern
- Prometheus metric registration in metrics.rs: existing counter/histogram pattern
- DLQ envelope construction in shutdown.rs: existing metadata field pattern

### Established Patterns
- PythonHandler stores Arc<RayonPool> for dispatch — similar pattern could store timeout handles
- ConsumerConfigBuilder::rayon_pool_size as builder method pattern for timeout config

### Integration Points
- Timeout config flows: ConsumerConfigBuilder → RuntimeConfig → PythonHandler::new()
- Timeout metrics: similar to existing handler_latency_seconds histogram
- DLQ metadata: extend existing FailureEnvelope with timeout fields

</code_context>

<specifics>
## Specific Ideas

No external specs referenced. Requirements fully captured above.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 08-async-timeout*
*Context gathered: 2026-04-29*