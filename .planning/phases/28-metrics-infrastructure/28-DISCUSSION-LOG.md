# Phase 28: Metrics Infrastructure - Discussion Log

> **Audit trail only.** Do not use as input to planning or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-04-18
**Phase:** 28-metrics-infrastructure
**Areas discussed:** MetricsSink trait design, HandlerMetrics recording, Label naming convention

---

## Area 1: MetricsSink Trait Design

| Option | Description | Selected |
|--------|-------------|----------|
| Sync only | Simpler, faster for hot-path recording in worker_loop. Metrics facade handles internal async behind the scenes. | ✓ |
| Async fn | More complex but could batch to external systems directly | |
| Trait has both | record_sync() + record_async() — users pick which to implement | |

**User's choice:** Sync only
**Notes:** Simpler and faster for hot-path. No need for async overhead in the recording path.

---

### Noop Default

| Option | Description | Selected |
|--------|-------------|----------|
| Facade handles noop | metrics crate global recorder drops all when no recorder set. No need for explicit noop MetricsSink. | ✓ |
| Explicit noop impl required | Provide NoopMetricsSink as explicit default so users can reference it | |

**User's choice:** Facade handles noop (Recommended)
**Notes:** Zero-cost when disabled — no explicit noop needed.

---

### Describe Method

| Option | Description | Selected |
|--------|-------------|----------|
| Minimal | Trait just has record_{counter,gauge,histogram} methods. metrics facade handles describe() via macros. | ✓ |
| With describe() | trait describe() method for explicit metric registration per backend | |

**User's choice:** Minimal (Recommended)
**Notes:** Keep trait small. metrics facade macros handle describe().

---

## Area 2: HandlerMetrics Recording

### Recording Site

| Option | Description | Selected |
|--------|-------------|----------|
| Around invoke_mode call | Wrap the entire invoke_mode call for accurate latency + count. Counter before, latency after. | ✓ |
| Per-branch in invoke_mode | Record inside each HandlerMode branch (SingleSync, SingleAsync, etc.) | |

**User's choice:** Around invoke_mode call (Recommended)
**Notes:** Single site covers all 4 HandlerMode variants. Clean wrap.

---

### Labels

| Option | Description | Selected |
|--------|-------------|----------|
| handler_id + topic + mode | Sufficient cardinality control. Partition too high-cardinality for high-volume topics. | ✓ |
| handler_id + topic + partition + mode | Full granularity, higher cardinality — partition counts can explode on large topics | |
| handler_id + mode only | Minimal cardinality, may not be enough for debugging | |

**User's choice:** handler_id + topic + mode (Recommended)
**Notes:** Partition excluded to control cardinality.

---

### Latency Histogram Buckets

| Option | Description | Selected |
|--------|-------------|----------|
| Fixed defaults | Standard buckets: [1ms, 5ms, 10ms, 50ms, 100ms, 500ms, 1s, 5s]. Configurable in Phase 32 if needed. | ✓ |
| Per-handler configurable | BatchPolicy-like struct with bucket boundaries per handler. Added API surface for Phase 28. | |

**User's choice:** Fixed defaults (Recommended)
**Notes:** Keep Phase 28 simple. Can add configurability later.

---

## Area 3: Label Naming Convention

### Prefix

| Option | Description | Selected |
|--------|-------------|----------|
| kafpy.handler.* | kafpy.handler.invocation, kafpy.handler.latency, kafpy.handler.error. Aligned with span naming. | ✓ |
| kafpy.* | kafpy.invocation, kafpy.latency, kafpy.error. Shorter but less organized. | |
| kafpy_python.* | kafpy_python.handler.invocation. Emphasizes Python handler focus. | |

**User's choice:** kafpy.handler.* (Recommended)
**Notes:** Aligned with tracing span naming convention from research.

---

### Error Labels

| Option | Description | Selected |
|--------|-------------|----------|
| FailureReason as string | error_type = FailureReason derived Display string. Aligns with existing FailureReason taxonomy. | ✓ |
| HandlerMode as string | error_type = mode (SingleSync, BatchAsync, etc.). Simpler but less specific. | |
| ExecutionResult variant | error_type = Error/Rejected based on result enum. Coarse-grained. | |

**User's choice:** FailureReason as string (Recommended)
**Notes:** Aligns with existing FailureReason taxonomy.

---

*Phase: 28-metrics-infrastructure*
*Discussion complete: 2026-04-18*
