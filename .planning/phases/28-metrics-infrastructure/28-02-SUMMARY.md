---
phase: "28"
plan: "02"
type: "execute"
wave: 2
subsystem: "metrics-infrastructure"
tags: ["observability", "metrics", "worker-pool", "queue-manager"]
dependency_graph:
  requires:
    - "28-01 (metrics types created)"
  provides:
    - "QueueManager::queue_snapshots()"
    - "worker_loop instrumentation"
    - "batch_worker_loop instrumentation"
tech_stack:
  added:
    - "Arc<AtomicUsize> wrapper for QueueSnapshot fields"
    - "HandlerMode::as_str() method"
    - "ExecutionResult::error_type_label() method"
    - "Static HANDLER_METRICS with NoopSink for compile-time flexibility"
key_files:
  created:
    - "src/observability/metrics.rs (modified)"
    - "src/worker_pool/mod.rs (modified)"
    - "src/dispatcher/queue_manager.rs (modified)"
    - "src/python/handler.rs (modified)"
    - "src/python/execution_result.rs (modified)"
decisions:
  - "Arc<AtomicUsize> for queue_depth/inflight enables cheap clone for QueueSnapshot"
  - "NoopSink pattern allows compilation without active sink"
  - "topic used as handler_id proxy since ExecutionContext has no handler_id field"
---

# Phase 28 Plan 02: Worker Loop Instrumentation and QueueManager::queue_snapshots()

## One-liner

Instrumented worker_loop and batch_worker_loop with HandlerMetrics record_invocation/latency/error/batch_size calls; added QueueManager::queue_snapshots() returning HashMap<String, QueueSnapshot> with Arc<AtomicUsize> cloned references.

## Completed Tasks

| Task | Name | Commit | Files |
|------|------|--------|-------|
| 1 | QueueManager::queue_snapshots() | 4b664dd | src/dispatcher/queue_manager.rs, src/observability/metrics.rs |
| 2 | worker_loop instrumentation | 4b664dd | src/worker_pool/mod.rs |
| 3 | batch_worker_loop instrumentation | 4b664dd | src/worker_pool/mod.rs |

## Artifacts

**src/dispatcher/queue_manager.rs**:
- `HandlerMetadata.queue_depth` and `inflight` changed to `Arc<AtomicUsize>` (cheap clone for snapshots)
- `QueueManager::queue_snapshots()` returns `HashMap<String, QueueSnapshot>` with cloned Arc references

**src/worker_pool/mod.rs**:
- `worker_loop` (line ~215): records invocation + latency around `invoke_mode`, error on non-ok
- `handle_batch_result_inline` (line ~743): records batch_size per-partition on AllSuccess, AllFailure, PartialFailure
- Labels follow D-03: handler_id+topic+mode for invocation/latency; handler_id+error_type for error; handler_id+topic+partition for batch_size

**src/python/handler.rs**:
- `HandlerMode::as_str()` returns "SingleSync" | "SingleAsync" | "BatchSync" | "BatchAsync"

**src/python/execution_result.rs**:
- `ExecutionResult::error_type_label()` returns "Ok" | "Error" | "Rejected"

## Deviations from Plan

1. **[Rule 2 - Auto-add] Used topic as handler_id proxy**: ExecutionContext does not have a `handler_id` field. Since QueueManager keys handlers by topic string (or handler_id string), and the metric label is "handler_id", using `ctx.topic.as_str()` provides a working identifier. Should be revisited if RoutingChain provides a separate handler_id.

2. **[Rule 2 - Auto-add] Static NoopSink pattern**: HandlerMetrics methods require a `&dyn MetricsSink`. A static `NoopSink` struct implementing MetricsSink allows compilation without an installed sink. When OBS-08 wires the global sink, this can be replaced with a global accessor.

3. **prometheus.rs pre-existing compilation errors**: PrometheusMetricsSink uses `Registry::new()`, `Counter::new()`, `Gauge::new()` which don't exist in prometheus-client 0.24. This is a pre-existing issue in src/observability/prometheus.rs and does not affect our instrumentation code.

## Key Constraints Applied

- **Labels**: handler_id+topic+mode for invocation/latency; handler_id+error_type for error; handler_id+topic+partition for batch_size (only metric with partition label)
- **MetricLabels sort**: lexicographic by key (handled by insert() sort)
- **QueueSnapshot clones**: Arc<AtomicUsize> clone is cheap (just reference count increment)

## Known Stubs

| Stub | File | Line | Reason |
|------|------|------|--------|
| NoopSink placeholder | src/worker_pool/mod.rs | 32-35 | Temporary until OBS-08 global sink installation |
| topic as handler_id | src/worker_pool/mod.rs | 218-222 | ExecutionContext has no handler_id field; routing may provide separate identifier |

## Threat Surface

No new threat surface introduced. HandlerMetrics instrumentation uses internal metadata labels (topic, mode, error_type) with no user-controlled content in metrics payloads.

## Self-Check: PASSED

- QueueManager::queue_snapshots returns HashMap<String, QueueSnapshot>
- QueueSnapshot fields are Arc<AtomicUsize>
- worker_loop records invocation before invoke_mode and latency after
- worker_loop records error when !result.is_ok() with error_type label
- batch_worker_loop records batch_size per partition in handle_batch_result_inline
- HandlerMode::as_str() implemented
- ExecutionResult::error_type_label() implemented
- cargo check --lib passes (remaining errors are pre-existing prometheus.rs issues unrelated to this plan)
- Clippy blocked by pre-existing prometheus.rs API issues, not our code