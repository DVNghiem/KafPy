# Phase 11: Fan-Out Core - Context

**Gathered:** 2026-04-29
**Status:** Ready for planning

<domain>
## Phase Boundary

One message triggers multiple sinks in parallel via JoinSet, with bounded fan-out degree enforced at dispatch time.

**Requirements:** FANOUT-01 (JoinSet dispatch), FANOUT-02 (max_fan_out enforced), FANOUT-03 (partial success non-blocking)

</domain>

<decisions>
## Implementation Decisions

### Sink Registration Model
- **D-01:** Static per handler — each handler declares its sink topics upfront at registration time. Simple, predictable, compile-time safe. No dynamic per-message routing.

### Sink Failure Tracking
- **D-02:** Callback-based — user provides `on_sink_complete(branch_id, result)` callback. Fire-and-forget style per branch. Primary message ACKed immediately regardless of sink outcomes.

### Max Fan-Out Overflow Behavior
- **D-03:** Backpressure — Kafka consumer pauses (`PausePartition`) when fan-out slots are exhausted at max_fan_out degree. Message waits in queue until a slot frees. Self-healing under load. Aligns with existing pause/resume pattern from v1.1.

### Fan-Out Degree Scope
- **D-04:** max_fan_out is a per-handler config (not global consumer config). Each handler can declare its own fan-out ceiling up to global max of 64.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Requirements
- `.planning/REQUIREMENTS.md` §FANOUT — FANOUT-01, FANOUT-02, FANOUT-03

### Prior Phase
- `.planning/phases/10-streaming-handler/10-CONTEXT.md` — JoinSet and streaming loop patterns
- `.planning/phases/08-async-timeout/08-CONTEXT.md` — JoinHandle abort pattern
- `.planning/STATE.md` — accumulated context table (v2.0 decisions)

### Implementation
- `src/worker_pool/pool.rs` — existing JoinSet usage in WorkerPool
- `src/worker_pool/worker.rs` — worker_loop pattern for message dispatch
- `src/python/handler.rs` — existing handler invocation pattern

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `JoinSet` in `pool.rs` — existing Tokio JoinSet for concurrent task management
- `PausePartition/ResumePartition` in worker_pool — existing backpressure mechanism to extend for fan-out overflow
- `PythonHandler::invoke_mode_with_timeout` — existing invocation wrapping pattern

### Established Patterns
- Static handler registration at startup
- Bounded channels with backpressure
- Callback-based completion for async operations

### Integration Points
- Fan-out dispatch plugs into existing worker_loop message processing
- Overflow backpressure extends existing queue depth monitoring
- Callback registration via PythonHandler similar to middleware chain

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

*Phase: 11-fan-out-core*
*Context gathered: 2026-04-29*
