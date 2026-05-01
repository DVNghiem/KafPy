# Phase 13: Fan-Out Python API - Context

**Gathered:** 2026-05-01
**Status:** Ready for planning

<domain>
## Phase Boundary

Python developers can register fan-out groups (multiple sink topics per handler) and observe fan-out-specific metrics and traces.

**Requirements:** FANOUT-06 (register_fanout API), OBSV-01 (fan-out metrics), OBSV-02 (trace context branching)

**Success Criteria:**
1. Python code can call `consumer.register_fanout(group_name, sink_topics, handler)` and receive a registration object
2. Fan-out metrics include `fan_out_id` and `branch_name` labels without double-counting throughput
3. Each fan-out branch spawns a child trace span with parent correlation

</domain>

<decisions>
## Implementation Decisions

### Fan-Out API Design
- **D-01:** `register_fanout(group_name, sink_topics, handler)` returns a `FanOutBuilder` that allows configuring `max_fan_out` before calling `.register()`. Sinks are attached to the handler at registration time — consistent with Phase 11's static per-handler model.
- **D-02:** The handler argument is a Python callable (same as `add_handler`). The same callable is invoked for both the primary topic and all sink topics.
- **D-03:** `fan_out_id` is generated as a UUID at `register_fanout()` call time and stored in `FanOutConfig`. This IDs all metrics and traces for this fan-out group.

### Fan-Out Metrics (OBSV-01)
- **D-04:** Primary message throughput is NOT double-counted — no fan-out-specific metric on the primary dispatch path.
- **D-05:** Fan-out branch metrics have labels: `fan_out_id` (group UUID), `branch_name` (topic name), `branch_outcome` (ok/error/timeout). Metric: `fan_out_branch_duration_seconds` (histogram).
- **D-06:** A counter `fan_out_branch_total{fan_out_id, branch_name, branch_outcome}` tracks branch completions. Throughput is branch-scoped, not message-scoped.

### Trace Context Branching (OBSV-02)
- **D-07:** Each fan-out branch gets a child span: `kafpy.fanout.branch` with fields `fan_out_id`, `branch_name`, `parent_trace_id`, `parent_span_id`.
- **D-08:** Trace context injection uses W3C traceparent format — `inject_trace_context()` (existing from Phase 10) is called for each branch with the parent's trace_id as `traceparent` header. The branch span derives its span_id independently.
- **D-09:** If no parent trace context exists, a new trace_id is generated for the fan-out dispatch and propagated to all branches.

### Claude's Discretion
- Exact metric histogram buckets — planner decides
- How to surface fan_out_id to Python handlers (via ExecutionContext field) — planner decides
- Registration object API surface details (method names, return type) — planner decides

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Requirements
- `.planning/REQUIREMENTS.md` §FANOUT — FANOUT-06
- `.planning/REQUIREMENTS.md` §OBSV — OBSV-01, OBSV-02

### Prior Phases
- `.planning/phases/11-fan-out-core/11-CONTEXT.md` — FanOutConfig, static per-handler sinks, callback-based completion
- `.planning/phases/12-fan-out-offset-commit/12-CONTEXT.md` — wait_all(), per-sink timeout, error classification
- `.planning/phases/10-streaming-handler/10-CONTEXT.md` — inject_trace_context, W3C traceparent parsing

### Implementation
- `src/worker_pool/fan_out.rs` — FanOutConfig, BranchResult, FanOutTracker
- `src/python/handler.rs` — invoke_mode_with_timeout, PythonHandler
- `src/observability/tracing.rs` — inject_trace_context, KafpySpanExt
- `src/observability/metrics.rs` — PrometheusSink, MetricLabels
- `src/python/context.rs` — ExecutionContext

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `inject_trace_context()` in tracing.rs: existing W3C traceparent parsing, can be called per branch with parent trace_id
- `MetricLabels` in metrics.rs: lexicographically sorted label map prevents cardinality explosion (OBSV-01)
- `FanOutTracker::wait_all()` in fan_out.rs: already tracks all branch completions, can emit metrics on completion
- `PythonHandler::set_fan_out()` in handler.rs: existing setter for FanOutConfig, already added in Phase 11

### Established Patterns
- Builder pattern for configs (ConsumerConfigBuilder, ProducerConfigBuilder)
- Handler registration API: `add_handler(topic, handler)` returns nothing (implicit)
- Trace context propagation via HashMap injected into PyDict

### Integration Points
- `register_fanout` is a method on the Python consumer exposed via PyO3
- FanOutConfig attaches to PythonHandler via existing set_fan_out method
- Branch spans created in worker_loop when dispatching fan-out branches (extend existing dispatch span)
- Metrics emitted in FanOutTracker::emit_completion or in the callback after wait_all()

</code_context>

<specifics>
## Specific Ideas

No external specs referenced. Implementation decisions fully captured above.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

---

*Phase: 13-fan-out-python-api*
*Context gathered: 2026-05-01*