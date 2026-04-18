# Phase 23: Dispatcher Integration - Context

**Gathered:** 2026-04-18
**Status:** Ready for planning

<domain>
## Phase Boundary

Wire Router into ConsumerDispatcher. RoutingDecision is evaluated before queue selection. Decision variants route to: explicit handler queue, default topic-based queue, DLQ, or offset-skip (Drop).

</domain>

<decisions>
## Implementation Decisions

### RoutingDecision::Route(handler_id) — D-01
- **Both topic AND handler ID apply** — handler_id annotates the message but topic-based routing still determines the queue
- Router selects HOW to route (handler_id for queue selection), dispatcher dispatches to topic queue
- The handler_id enables per-rule routing within topic queues

### RoutingDecision::Drop — D-02
- **Follow big tech** — drop message and advance offset as normal delivery (no handler, no DLQ, offset tracked for commit)
- Drop = normal completion path for filtering

### RoutingDecision::Reject — D-03
- **Follow big tech** — reject goes to DLQ immediately (no retry, fast failure path)
- Straight to DLQ without RetryCoordinator involvement

### RoutingDecision::Defer — D-04
- **Follow big tech** — Defer means routing inconclusive, proceed with default topic-based queue (normal dispatch)
- Default handler = topic queue via standard dispatcher.send

### Router call site — D-05
- **Follow big tech** — Router called in ConsumerDispatcher::run() before dispatcher.send
- RoutingContext built from OwnedMessage, Router::route() called before queue selection
- RoutingDecision then drives: send to topic queue (Defer), send to handler_id queue (Route), skip handler + advance offset (Drop), send to DLQ (Reject)

### No payload copies — D-06
- No heap allocations in routing path (inherited from Phase 21 zero-copy requirement)

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase Context
- `.planning/ROADMAP.md` — Phase 23 success criteria (7 items)
- `.planning/STATE.md` — Phase 22 complete, routing chain wired, next is Phase 23
- `.planning/PROJECT.md` — Key Decisions: pattern→header→key→python→default precedence, RoutingDecision 4 variants

### Codebase References
- `src/dispatcher/mod.rs` — ConsumerDispatcher::run() where router will be wired in
- `src/routing/chain.rs` — RoutingChain::route() precedence chain, python_slot already present
- `src/routing/context.rs` — RoutingContext::from_message(&OwnedMessage) for zero-copy construction
- `src/routing/decision.rs` — RoutingDecision enum (Route, Drop, Reject, Defer)
- `src/routing/router.rs` — Router trait
- `src/dlq/router.rs` — DlqRouter trait pattern for DLQ routing integration
- `src/consumer/runner.rs` — ConsumerRunner and stream integration

</canonical_refs>

<code_context>
## Existing Code Insights

### Integration Points
- `ConsumerDispatcher::new()` — where Router (RoutingChain Arc) gets injected
- `ConsumerDispatcher::run()` loop — where Router::route() gets called
- `OwnedMessage` → `RoutingContext::from_message()` — zero-copy path
- `send_with_policy_and_signal()` — backpressure + dispatch path
- WorkerPool/ack path for offset tracking

### Established Patterns
- RoutingChain::new().with_topic_router().with_header_router()... builder pattern
- Arc<dyn Router> for heterogeneous router storage
- D-04: explicit default handler required

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 23-dispatcher-integration*
*Context gathered: 2026-04-18*
