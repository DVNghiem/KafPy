# Phase 22: Python Integration - Context

**Gathered:** 2026-04-17
**Status:** Ready for planning

<domain>
## Phase Boundary

Python code can optionally participate in routing — `PythonRouter` is called via `spawn_blocking` only when all Rust routers (pattern → header → key) return `Defer`. Python routing is the last slot before the default handler.

</domain>

<decisions>
## Implementation Decisions

### Python callable argument — D-06
- **Dict argument** — Python callable receives a `dict` with keys: `topic` (str), `headers` (dict), `key` (bytes or None), `payload` (bytes or None)
- Follows the same `PyDict` pattern as `PythonHandler.invoke` — consistent API for Python users
- Headers as dict: `{"header-name": b"header-value"}` (bytes for values)

### Python callable return type — D-07
- **String enum** — Python callable returns a string:
  - `"route:{handler_id}"` → `RoutingDecision::Route(handler_id)`
  - `"drop"` → `RoutingDecision::Drop`
  - `"reject:{reason}"` → `RoutingDecision::Reject(Explicit(reason))`
  - `"defer"` → `RoutingDecision::Defer` (edge case — Python explicitly defers)
- Simple to implement, easy for Python users to understand

### Python routing errors — D-08
- **Reject(RoutingPythonError)** — if the Python callable throws an exception, `PythonRouter` returns `Reject(RoutingPythonError)`
- Explicit failure — routing error is distinguishable from a legitimate `Reject` from user code
- DLQ routing follows from `Reject` — same as Phase 23 DISPATCH-02 behavior
- Log the exception details at WARN level before returning `Reject`

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase Context
- `.planning/ROADMAP.md` — Phase 22 success criteria (4 items)
- `.planning/REQUIREMENTS.md` — PYROUTER-01, PYROUTER-02, PYROUTER-03
- `.planning/STATE.md` — Prior decisions: pattern→header→key→python→default precedence, Python routing is optional fallback
- `.planning/phases/21-routing-core/21-CONTEXT.md` — Phase 21 decisions (RoutingContext, RoutingDecision, Router trait, RoutingChain)

### Codebase References
- `src/routing/chain.rs` — `RoutingChain::with_python_router()` already exists; `PythonRouter` plugs into the `python_router` slot (fourth in chain)
- `src/routing/context.rs` — `RoutingContext<'a>` with zero-copy borrowed headers; `HandlerId = String`
- `src/routing/decision.rs` — `RoutingDecision` enum; `RejectReason` enum
- `src/routing/router.rs` — `Router` trait with `route(&self, ctx: &RoutingContext) -> RoutingDecision`
- `src/python/handler.rs` — `PythonHandler.invoke` pattern with `spawn_blocking` and `Py<PyAny>` callback; use same pattern for `PythonRouter`
- `src/kafka_message.rs` — `KafkaMessage` headers field `Vec<(String, Option<Vec<u8>>)>` as reference for dict conversion

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `PythonHandler.invoke` pattern: `spawn_blocking` + `Py<PyAny>` callback + `Python::with_gil` — replicate for `PythonRouter::route`
- `RoutingChain::with_python_router()` — already exists in chain.rs, needs `PythonRouter` implementation
- `RoutingContext<'a>::from_message(msg: &'a OwnedMessage)` — used to construct routing context from the incoming message

### Integration Points
- `PythonRouter` must implement `Router` trait: `fn route(&self, ctx: &RoutingContext) -> RoutingDecision`
- `PythonRouter` is stored as `Arc<dyn Router>` in `RoutingChain.python_router` slot
- `PythonRouter` is NOT stored in `ConsumerConfig` — it is set separately via `RoutingChain::with_python_router()`
- Phase 23 wires `RoutingChain` into `ConsumerDispatcher` — `PythonRouter` activation happens via the chain

### Pattern to Follow
- `spawn_blocking` for GIL release during Python execution (from `PythonHandler.invoke`)
- `Py<PyAny>` for GIL-independent callable storage
- Return string parsing in Rust: match on the returned string to produce `RoutingDecision`

</code_context>

<specifics>
## Specific Ideas

- Python callable: `def my_router(msg: dict) -> str: ...`
  - Returns `"route:handler-id"`, `"drop"`, `"reject:reason"`, or `"defer"`
- Headers dict: `{"X-Correlation-ID": b"abc123", "content-type": b"application/json"}`
- Error case: log.warn("Python router error: {exception}") + return `Reject(RoutingPythonError)`

</specifics>

<deferred>
## Deferred Ideas

- Schema-based routing (Avro/Protobuf) — deferred to post-v1.5
- Content-based routing (payload parsing) — Python fallback only; already handled via Python callable payload argument
- Multi-handler fan-out — single handler per message, deferred

</deferred>

---

*Phase: 22-python-integration*
*Context gathered: 2026-04-17*
