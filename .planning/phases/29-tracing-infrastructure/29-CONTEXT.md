# Phase 29: Tracing Infrastructure - Context

**Gathered:** 2026-04-18
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 29 establishes OpenTelemetry tracing integration for KafPy. Creates `enable_otel_tracing()` setup function, wraps `worker_loop` and `batch_worker_loop` invoke calls in spans, adds dispatch and DLQ spans, propagates W3C tracecontext across the PyO3 GIL boundary via `spawn_blocking`, and provides `ObservabilityConfig` for endpoint/service name/sampling configuration. Zero-cost when tracing is not enabled — no global state set by KafPy.

</domain>

<decisions>
## Implementation Decisions

### Tracing Layer Initialization — OBS-11, OBS-12
- **OBS-11:** `enable_otel_tracing()` function sets up OTLP exporter and `tracing-opentelemetry` Layer, returns `LayerHandle` user owns
- **OBS-12:** KafPy never calls `set_global_default()` or `set_global_recorder()` — user owns all global state (consistent with Phase 28 metrics pattern)

### Span Wrapping — OBS-13, OBS-14, OBS-15
- **OBS-13:** `worker_loop` wraps each `invoke_mode` call in span named `kafpy.handler.invoke` with fields: handler_id, topic, partition, offset, mode
- **OBS-14:** `ConsumerDispatcher::run` wraps dispatch in span named `kafpy.dispatch.process` with fields: topic, partition, offset, routing_decision
- **OBS-15:** DLQ routing emits span named `kafpy.dlq.route` with fields: handler_id, reason, partition

### Span Context Propagation — OBS-16
- **OBS-16:** Span context propagates across PyO3 GIL boundary via explicit W3C tracecontext header injection at `spawn_blocking` call sites and extraction in Python callable
- Trace context injected as headers into the callable arguments; extracted and re-injected on return

### Async Span Patterns — OBS-17
- **OBS-17:** Use `span.in_scope()` for async code — never `Span::enter()` across await points
- `#[instrument]` not used — explicit `span.in_scope()` gives clearer control in worker_loop async context
- Rationale: worker_loop has complex async control flow; explicit scope is easier to verify than attribute macro expansion

### ObservabilityConfig — OBS-18
- **OBS-18:** `ObservabilityConfig` struct allows user to configure OTLP endpoint, service name, and sampling rate at initialization
- Same struct used across metrics (Phase 28) and tracing — unified observability configuration

### Span Naming Convention — OBS-19
- **OBS-19:** Span naming follows `kafpy.{component}.{operation}` convention
- Examples: `kafpy.handler.invoke`, `kafpy.dispatch.process`, `kafpy.dlq.route`
- Aligned with metrics naming (`kafpy.handler.*`) for correlation between signals

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Codebase References
- `src/worker_pool/mod.rs` — worker_loop, batch_worker_loop (primary span wrapping sites)
- `src/dispatcher/mod.rs` — ConsumerDispatcher, dispatch span
- `src/dlq/produce.rs` — DLQ routing span
- `src/python/handler.rs` — PythonHandler, spawn_blocking call sites (trace context boundary)
- `src/logging.rs` — existing Logger::init() pattern (Phase 32 refactor target)

### Prior Phase Context
- `.planning/phases/28-metrics-infrastructure/28-CONTEXT.md` — Phase 28 decisions on zero-cost facade, no global state, MetricLabels sorted ordering

### Requirements
- `.planning/REQUIREMENTS.md` — OBS-11 through OBS-19 (Phase 29 requirements)

</canonical_refs>

<codebase_context>
## Existing Code Insights

### Reusable Assets
- `tracing_subscriber::fmt()` already used in `src/logging.rs` — Layer setup pattern familiar
- `metrics` crate facade (Phase 28) already establishes zero-cost pattern — tracing should mirror this

### Established Patterns
- No global state set by KafPy (Phase 28 pattern) — tracing LayerHandle returned to user
- `spawn_blocking` GIL pattern — trace context injection happens at this boundary
- Per-component optional config (retry_policy, BatchPolicy) — ObservabilityConfig follows same pattern

### Integration Points
- `worker_loop` around `invoke_mode` call — span wraps the entire invoke cycle
- `spawn_blocking` in `src/python/handler.rs` — W3C header injection/extraction site
- `ConsumerDispatcher::run` in `src/dispatcher/mod.rs` — dispatch span
- `DlqRouter::route` in `src/dlq/` — DLQ span

</code_base_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---
*Phase: 29-tracing-infrastructure*
*Context gathered: 2026-04-18*
