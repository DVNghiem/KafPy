# Phase 28: Metrics Infrastructure - Context

**Gathered:** 2026-04-18
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 28 establishes the metrics infrastructure foundation for the v1.7 observability layer. Creates `MetricsSink` trait, `HandlerMetrics` struct, instruments `worker_loop` and `batch_worker_loop` with counter/histogram recording, adds `QueueManager::queue_snapshots()` for polling-based gauge updates, and implements `PrometheusMetricsSink`. The `metrics` crate facade provides zero-cost noop when no recorder is installed.

</domain>

<decisions>
## Implementation Decisions

### MetricsSink Trait — D-01
- **Sync API only** — `record_counter()`, `record_histogram()`, `record_gauge()` methods
- No async fn — hot-path recording is sync; metrics facade handles internal batching behind the scenes
- **Minimal trait** — only record methods, no `describe()` (metrics facade macros handle describe)
- **No explicit noop** — `metrics` crate global recorder drops all when no recorder set (zero-cost)
- Users install recorder via `metrics::set_recorder()` before creating consumers

### HandlerMetrics Recording Site — D-02
- **Around `invoke_mode` call** — counter increments before, latency recorded after
- Wraps the entire invoke cycle: start_time → invoke_mode → ExecutionResult → record latency
- Single site covers all 4 HandlerMode variants (SingleSync, SingleAsync, BatchSync, BatchAsync)
- Error counter records on ExecutionResult::Error and RoutingDecision::Reject

### HandlerMetrics Labels — D-03
- **Invocation counter + latency histogram**: labels = `handler_id`, `topic`, `mode`
- **Error counter**: labels = `handler_id`, `error_type` where `error_type` = FailureReason::to_string()
- Partition NOT included as label — too high-cardinality for large topics with many partitions
- `MetricLabels` type enforces lexicographically sorted insertion (prevents cardinality explosion)

### Metric Naming Convention — D-04
- **Prefix: `kafpy.handler.*`** — e.g., `kafpy.handler.invocation`, `kafpy.handler.latency`, `kafpy.handler.error`, `kafpy.handler.batch_size`
- Aligned with tracing span naming (`kafpy.handler.invoke`) for correlation
- Gauge names: `kafpy.queue.depth`, `kafpy.queue.inflight`

### Latency Histogram Buckets — D-05
- **Fixed default buckets**: `[1ms, 5ms, 10ms, 50ms, 100ms, 500ms, 1s, 5s]`
- Not configurable in Phase 28 — can be added in Phase 32 if needed
- Duration measured from invoke start to ExecutionResult return

### QueueManager Gauge Polling — D-06
- `QueueManager::queue_snapshots()` returns `HashMap<HandlerId, QueueSnapshot>` with `queue_depth: AtomicUsize` and `inflight: AtomicUsize`
- Polling-based: gauges updated by background task every 10s, not per-message (avoids hot-path overhead)
- `WorkerPoolMetrics` struct holds the snapshot state

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Codebase References
- `src/worker_pool/mod.rs` — worker_loop, batch_worker_loop (primary instrumentation sites)
- `src/dispatcher/mod.rs` — QueueManager with existing queue_depth/inflight atomics
- `src/python/handler.rs` — PythonHandler, HandlerMode enum
- `src/python/execution_result.rs` — ExecutionResult, FailureReason
- `src/failure/reason.rs` — FailureReason taxonomy
- `src/logging.rs` — existing Logger::init() (will be refactored in Phase 32)

### Research References
- `.planning/research/SUMMARY.md` — metrics crate facade, MetricLabels sorted ordering, zero-cost noop
- `.planning/research/PITFALLS.md` — label cardinality explosion prevention

</canonical_refs>

<codebase_context>
## Existing Code Insights

### Reusable Assets
- `QueueManager` already tracks `queue_depth` and `inflight` atomically — `queue_snapshots()` is a thin wrapper
- `FailureReason` already has Display impl — used as `error_type` label string directly
- `metrics` crate already available via rdkafka-lite dependency chain

### Established Patterns
- Facade trait with noop default (similar to Executor trait in v1.2)
- `spawn_blocking` GIL pattern — metrics recording happens outside the blocking scope (before or after)
- Per-handler optional config (retry_policy, BatchPolicy) — MetricsPolicy could follow same pattern

### Integration Points
- `worker_loop` around `invoke_mode` call
- `batch_worker_loop` around `invoke_batch` / `invoke_batch_async` call
- `QueueManager` for gauge snapshots
- `WorkerPool` for housing the metrics polling task

</codebase_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---
*Phase: 28-metrics-infrastructure*
*Context gathered: 2026-04-18*
