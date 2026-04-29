# Phase 12: Fan-Out Offset Commit - Context

**Gathered:** 2026-04-29
**Status:** Ready for planning

<domain>
## Phase Boundary

Offset commit gated on all fan-out branch completions; per-sink errors classified and routed to DLQ with branch metadata.

**Requirements:** FANOUT-04 (per-sink timeout), FANOUT-05 (per-sink error classification + DLQ routing)

**Success Criteria:**
1. Offset is only committed after all fan-out branches complete (tracked by FanOutTracker)
2. Each fan-out branch has its own timeout scope propagated via invoke_mode_with_timeout
3. Per-sink errors carry branch_id and fan_out_id metadata for DLQ routing

</domain>

<decisions>
## Implementation Decisions

### Offset Commit Trigger
- **D-01:** Offset commits when all fan-out branches finish, regardless of individual outcomes. Failures (error or timeout) are logged and routed to DLQ — they do NOT block the offset commit. Aligns with Phase 11's non-blocking partial success philosophy.

### Per-Sink Timeout
- **D-02:** Per-sink timeout via `invoke_mode_with_timeout`. Each branch gets its own deadline scope. A timeout on one branch does not cancel other in-flight branches.

### Error Classification (DLQ Routing)
- **D-03:** Three-category classification: `Timeout` (sink exceeded its timeout window), `HandlerError` (sink handler raised an exception), `SystemError` (system-level failure — Kafka error, serialization error, etc.). Each category carries `branch_id` and `fan_out_id` metadata for DLQ routing.

### FanOutTracker Completion Signal
- **D-04:** `FanOutTracker::wait_all()` — all branches must reach a terminal state (Ok, Error, Timeout) before offset commit is triggered. Success/failure outcome is recorded per branch for DLQ routing, but does not gate the commit itself.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Requirements
- `.planning/REQUIREMENTS.md` §FANOUT — FANOUT-04, FANOUT-05

### Prior Phase
- `.planning/phases/11-fan-out-core/11-CONTEXT.md` — FanOutTracker, BranchResult, partial success philosophy
- `.planning/phases/10-streaming-handler/10-CONTEXT.md` — invoke_mode_with_timeout pattern
- `.planning/phases/08-async-timeout/08-CONTEXT.md` — JoinHandle abort pattern
- `.planning/STATE.md` — accumulated v2.0 decisions table

### Implementation
- `src/worker_pool/fan_out.rs` — FanOutTracker, BranchResult enum, CallbackRegistry
- `src/python/handler.rs` — invoke_mode_with_timeout, HandlerMode
- `src/dispatcher/mod.rs` — OwnedMessage, offset tracking

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `BranchResult` enum already has `Ok`, `Error { reason, exception }`, and `Timeout { timeout_ms }` — matches 3-category classification
- `FanOutTracker::wait_all()` needs to be added — tracks when all branches reach terminal state
- `CallbackRegistry::emit()` — invokes completion callbacks, already handles branch correlation

### Established Patterns
- Per-sink timeout via `invoke_mode_with_timeout` (existing in handler.rs)
- Non-blocking callback for completion (Phase 11)
- Backpressure via `PausePartition` on slot exhaustion

### Integration Points
- Offset commit gated by `FanOutTracker::wait_all()` after all branches complete
- Per-sink timeout wraps each branch's handler invocation
- Error metadata (branch_id, fan_out_id) added to ExecutionContext for DLQ routing

</code_context>

<specifics>
## Specific Ideas

No external specs referenced. Implementation decisions fully captured above.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

---

*Phase: 12-fan-out-offset-commit*
*Context gathered: 2026-04-29*
