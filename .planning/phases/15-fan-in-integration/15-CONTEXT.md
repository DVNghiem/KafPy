# Phase 15: Fan-In Integration - Context

**Gathered:** 2026-05-01
**Status:** Ready for planning
**Mode:** auto (no discuss-phase run — decisions derived from prior phases and codebase analysis)

<domain>
## Phase Boundary

Fan-in multiplexers are wired into QueueManager with per-source backpressure; Python API exposed via `register_fanin`.

**Requirements:** FANIN-04 (per-source backpressure), FANIN-05 (register_fanin API), OBSV-03 (per-source consumer lag metrics)

**Success Criteria:**
1. Slow source triggers `PausePartition` only for that topic+partition; fast sources are unaffected
2. Python code can call `consumer.register_fanin(handler_key, sources: List[str])`
3. Consumer lag metrics include topic label for per-source monitoring

</domain>

<decisions>
## Implementation Decisions

### Per-Source Backpressure (FANIN-04)
- **D-01:** Use existing `BackpressureAction::PausePartition { topic, partition }` variant — already supports targeted pause. Partition defaults to `-1` (all partitions) in `PauseOnFullPolicy::on_queue_full()`.
- **D-02:** Source topic is available from `ExecutionContext.source_topic` (populated in Phase 14, D-06). Route `on_queue_full` signal to the specific topic that is slow, not the fan-in handler key.
- **D-03:** Queue depth tracking per source requires mapping from `handler_key` (fan-in group) back to source topics. Since one fan-in handler merges N sources, the queue depth is shared across all sources. Backpressure is applied per-source by pausing the Kafka consumer subscription for that topic only.
- **D-04:** `consumer_dispatcher.rs` already has `check_resume(topic, queue_depth)` — threshold-based resume. Resume is checked per-topic since `queue_depth` is aggregated per handler but pause is per-topic.
- **D-05:** Per-source backpressure: when one source's queue is overwhelmed, only that source is paused via `pause_partition(topic)`. Other sources continue consuming normally. Resume when queue depth drops below `capacity * resume_threshold`.

### register_fanin API (FANIN-05)
- **D-06:** `FanInBuilderRust::register_into_consumer()` exists in `src/python/fan_in_bridge.rs` — stores handler with `fan_in_id` via `py_consumer.add_handler_with_fan_in()`. Needs to be exposed as a PyConsumer method callable from Python.
- **D-07:** API signature: `consumer.register_fanin(handler_key: str, sources: List[str])` returns `FanInRegistration` (handler_key, fan_in_id, sources) — consistent with Phase 13 `FanOutRegistration` pattern.
- **D-08:** `PyConsumer::register_fanin()` method accepts `handler_key` (str), `topics` (Vec<String>), and a Python callable, constructs `FanInBuilderRust`, and returns `FanInRegistration`.

### OBSV-03 Per-Source Consumer Lag Metrics
- **D-09:** Consumer lag metric: `consumer_lag` with `topic` label (not partition). Value = `Highwatermark - ConsumerPosition` per topic. Already partially instrumented in Phase 10.
- **D-10:** For fan-in, the topic label distinguishes which source is lagging. Without topic label, fan-in lag is opaque — can't tell which source is slow.

### Claude's Discretion
- Exact queue depth threshold values — planner decides based on existing defaults
- Whether to track per-partition lag in addition to per-topic lag — planner decides

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Requirements
- `.planning/REQUIREMENTS.md` §FANIN — FANIN-04, FANIN-05
- `.planning/REQUIREMENTS.md` §OBSV — OBSV-03

### Prior Phases
- `.planning/phases/13-fan-out-python-api/13-CONTEXT.md` — FanOutRegistration pattern, register_fanout API shape
- `.planning/phases/14-fan-in-multiplexer/14-CONTEXT.md` — source_topic, fan_in_id in ExecutionContext, round-robin merge

### Implementation
- `src/python/fan_in_bridge.rs` — FanInBuilderRust, FanInRegistration (already implemented)
- `src/python/handler.rs` — add_handler_with_fan_in method on PyConsumer
- `src/dispatcher/backpressure.rs` — BackpressureAction::PausePartition, BackpressurePolicy trait
- `src/dispatcher/consumer_dispatcher.rs` — pause_partition, resume_partition, check_resume, paused_topics
- `src/dispatcher/queue_manager.rs` — HandlerMetadata, queue_depth tracking
- `src/worker_pool/fan_in_loop.rs` — fan_in_worker_loop with source_topic context

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `BackpressureAction::PausePartition { topic, partition }` — already implemented, can route per-topic pause
- `FanInBuilderRust` in fan_in_bridge.rs — builder pattern already exists
- `FanInRegistration` pyclass — already defined with handler_key, fan_in_id, sources
- `ExecutionContext.source_topic` — populated in Phase 14 (D-06)

### Established Patterns
- `register_fanout` API from Phase 13 — same pattern applies to `register_fanin`
- Builder pattern: `FanInBuilderRust::new(...)` → `.register_into_consumer()`
- `PyConsumer::add_handler_with_fan_in()` — stores handler with fan_in_id

### Integration Points
- `register_fanin` PyMethod — adds to PyConsumer via `#[pymethods]`
- Fan-in handler registered in PyConsumer's handler map with fan_in_id set
- Backpressure signals routed through `BackpressurePolicy::on_queue_full` in consumer_dispatcher

</code_context>

<specifics>
## Specific Ideas

No external specs referenced. Implementation decisions fully captured above.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

---

*Phase: 15-fan-in-integration*
*Context gathered: 2026-05-01*
