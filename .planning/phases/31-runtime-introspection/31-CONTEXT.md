# Phase 31: Runtime Introspection - Context

**Gathered:** 2026-04-18
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 31 adds Python-accessible runtime introspection via RuntimeSnapshot without hot-path overhead. Provides get_runtime_snapshot() PyO3 function and Consumer.status() Python method returning structured dict with worker states, queue depths, accumulator info, and consumer lag summary. Zero-cost when not called — no atomic updates on hot path.

</domain>

<decisions>
## Implementation Decisions

### RuntimeSnapshot Struct — OBS-27
- **OBS-27:** `RuntimeSnapshot` holds worker_pool status (idle/active/busy worker counts), queue depths per handler, accumulator states, consumer_lag_summary

### PyO3 API — OBS-28
- **OBS-28:** `get_runtime_snapshot()` PyO3 function returns RuntimeSnapshot as Python dict

### Consumer.status() — OBS-29
- **OBS-29:** `Consumer.status()` Python method returns structured dict with: worker_states, queue_depths, accumulator_info, consumer_lag_summary

### Zero-Cost — OBS-30
- **OBS-30:** Introspection API zero-cost when not called — no atomic updates on hot path

### worker_loop Status — OBS-31
- **OBS-31:** worker_loop status includes current handler_id being processed and per-partition accumulator depth

### Status Callback — OBS-32
- **OBS-32:** Python can register `status_callback` invoked on every status snapshot (opt-in, not default)

</decisions>

<canonical_refs>
## Canonical References

### Codebase References
- `src/worker_pool/mod.rs` — worker_loop status
- `src/dispatcher/mod.rs` — QueueManager queue_snapshots()
- `src/coordinator/offset_tracker.rs` — offset_snapshots()
- `src/pyconsumer.rs` — PyO3 bindings

### Prior Phase Context
- `.planning/phases/28-metrics-infrastructure/28-CONTEXT.md` — zero-cost patterns
- `.planning/phases/30-kafka-level-metrics/30-CONTEXT.md` — kafka metrics structure

### Requirements
- `.planning/REQUIREMENTS.md` — OBS-27 through OBS-32

</canonical_refs>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---
*Phase: 31-runtime-introspection*
*Context gathered: 2026-04-18*
