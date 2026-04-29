---
phase: "04"
plan: "01"
type: execute
wave: 1
depends_on: []
files_modified:
  - src/observability/tracing.rs
  - src/observability/metrics.rs
  - src/observability/mod.rs
autonomous: true
requirements:
  - OBS-01
  - OBS-02
  - OBS-03
  - OBS-04
  - OBS-05
  - OBS-06
  - OBS-07
  - PY-05
user_setup: []
must_haves:
  truths:
    - "Each handler invocation creates a tracing span with topic, partition, offset, handler_name, mode, attempt attributes"
    - "W3C traceparent header is parsed and trace context is continued in the handler span"
    - "kafpy.message.throughput counter increments per message delivered"
    - "kafpy.handler.latency histogram records processing time per handler invocation"
    - "kafpy.consumer.lag gauge reflects highwater - committed offset per partition"
    - "kafpy.queue.depth gauge reflects current queued messages per handler"
    - "kafpy.dlq.messages counter increments when message is produced to DLQ"
    - "@handler(topic='...', batch=True) creates a batch handler with batch-level ack/nack"
  artifacts:
    - path: "src/observability/tracing.rs"
      provides: "Span creation with full attributes, trace context linking"
      min_lines: 50
    - path: "src/observability/metrics.rs"
      provides: "PrometheusSink, MetricsRegistry, all metric recorders (OBS-03 through OBS-07)"
      min_lines: 80
    - path: "kafpy/runtime.py"
      provides: "batch=True decorator support for @handler"
      min_lines: 20
  key_links:
    - from: "src/python/handler.rs"
      to: "src/observability/tracing.rs"
      via: "invoke() calls kafpy_handler_invoke() span and enters it before calling Python"
      pattern: "kafpy_handler_invoke.*enter"
    - from: "src/dispatcher/queue_manager.rs"
      to: "src/observability/metrics.rs"
      via: "queue_snapshots() feeds queue depth gauges via polling background task"
      pattern: "queue_snapshots.*MetricsSink"
    - from: "src/dlq/produce.rs"
      to: "src/observability/metrics.rs"
      via: "SharedDlqProducer produces to DLQ -> increments kafpy.dlq.messages counter"
      pattern: "record_counter.*dlq"
    - from: "kafpy/runtime.py"
      to: "src/python/handler.rs"
      via: "handler(batch=True) sets BatchSync/BatchAsync mode -> BatchPolicy configured"
      pattern: "batch_sync.*BatchSync"
---

<objective>
Add comprehensive observability to KafPy: tracing spans per handler execution (OBS-01), W3C trace context propagation from Kafka headers wired into spans (OBS-02), Prometheus metrics for message throughput (OBS-03), processing latency histograms (OBS-04), consumer lag gauges (OBS-05), queue depth gauges (OBS-06), and DLQ volume tracking (OBS-07). Also add batch=True handler support (PY-05).
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
</execution_context>

<context>
@.planning/PROJECT.md
@.planning/ROADMAP.md
@.planning/STATE.md
@.planning/phases/03-python-handler-api/03-SUMMARY.md
@.planning/phases/04-observability/04-CONTEXT.md

# Key existing interfaces (extracted for executor)

From src/observability/tracing.rs:
```rust
pub trait KafpySpanExt for Span {
    fn kafpy_handler_invoke(&self, handler_id: &str, topic: &str, partition: i32, offset: i64, mode: &str) -> Span;
    fn kafpy_dispatch_process(&self, topic: &str, partition: i32, offset: i64, routing_decision: &str) -> Span;
    fn kafpy_dlq_route(&self, handler_id: &str, reason: &str, partition: i32) -> Span;
}
pub fn inject_trace_context(headers: &HashMap<String, String>, out: &mut HashMap<String, String>);
```

From src/observability/metrics.rs:
```rust
pub trait MetricsSink: Send + Sync {
    fn record_counter(&self, name: &str, labels: &[(&str, &str)]);
    fn record_histogram(&self, name: &str, value: f64, labels: &[(&str, &str)]);
    fn record_gauge(&self, name: &str, value: f64, labels: &[(&str, &str)]);
}
pub struct MetricLabels(Vec<(&'static str, String)>);
impl MetricLabels {
    pub fn insert(mut self, key: &'static str, value: impl Into<String>) -> Self;
    pub fn as_slice(&self) -> Vec<(&str, &str)>;
}
pub struct HandlerMetrics;
impl HandlerMetrics {
    pub fn record_invocation(&self, sink: &dyn MetricsSink, labels: &MetricLabels);
    pub fn record_latency(&self, sink: &dyn MetricsSink, labels: &MetricLabels, elapsed: Duration);
    pub fn record_error(&self, sink: &dyn MetricsSink, labels: &MetricLabels);
    pub fn record_batch_size(&self, sink: &dyn MetricsSink, labels: &MetricLabels, size: usize);
}
pub struct QueueSnapshot { pub queue_depth: Arc<AtomicUsize>, pub inflight: Arc<AtomicUsize> }
pub mod noop_sink { pub struct NoopSink; }
```

From src/python/handler.rs:
```rust
pub enum HandlerMode { SingleSync, SingleAsync, BatchSync, BatchAsync }
impl HandlerMode {
    pub fn as_str(&self) -> &'static str;
    pub fn from_opt_str(s: Option<&str>) -> Self;
}
pub struct BatchPolicy { pub max_batch_size: usize, pub max_batch_wait_ms: u64 }
```

From src/dispatcher/queue_manager.rs:
```rust
impl QueueManager {
    pub fn queue_snapshots(&self) -> HashMap<String, QueueSnapshot>;
    pub fn get_inflight(&self, topic: &str) -> Option<usize>;
}
pub struct HandlerMetadata { pub queue_depth: Arc<AtomicUsize>, pub inflight: Arc<AtomicUsize> }
```

From src/dlq/router.rs:
```rust
pub trait DlqRouter: Send + Sync { fn route(&self, metadata: &DlqMetadata) -> TopicPartition; }
pub struct DefaultDlqRouter { dlq_topic_prefix: String }
```
</context>

<tasks>

<task type="auto">
  <name>Task 1: Add span attributes and wire trace context into handler spans (OBS-01, OBS-02)</name>
  <files>src/observability/tracing.rs</files>
  <action>
Extend `KafpySpanExt::kafpy_handler_invoke()` to include additional span attributes needed per OBS-01:
- `attempt: u32` — current attempt number (0 if first try)
- `handler_name: String` — Python handler function name extracted from the handler registry

Update `invoke()` in src/python/handler.rs to:
1. Extract W3C trace context from Kafka message headers using `inject_trace_context()` (already done in Phase 3)
2. Wrap the Python handler invocation in a tracing span using `tracing::Span::kafpy_handler_invoke()` with the extracted trace_id as a parent link
3. Record span attributes: topic, partition, offset, handler_name, mode, attempt

For batch modes (BatchSync/BatchAsync), create a span per batch with batch_size attribute.

The span should be entered before calling the Python handler and exited after completion (success or failure).

Use `tracing::Span::or_current()` to avoid creating redundant spans when no subscriber is configured.

Note: `inject_trace_context()` already exists in tracing.rs from Phase 3 — it parses W3C traceparent header format 00-{trace_id:32}-{span_id:16}-{flags:2}.
</action>
  <verify>cargo check --lib 2>&1 | tail -20</verify>
  <done>Span created per handler invocation with topic, partition, offset, handler_name, mode, attempt attributes; trace_id from W3C traceparent linked as parent context</done>
</task>

<task type="auto">
  <name>Task 2: Add metrics infrastructure — PrometheusSink and metric recorders (OBS-03, OBS-04, OBS-05, OBS-06, OBS-07)</name>
  <files>src/observability/metrics.rs</files>
  <action>
Add a `PrometheusSink` struct implementing `MetricsSink` that records to Prometheus-compatible metrics registries.

Add the following metric recorders to `HandlerMetrics`:

**OBS-03 — Message throughput:**
- `kafpy.message.throughput` — Counter with labels: topic, handler_id, mode
  - Increment on each message delivered to handler (single) or each message in batch

**OBS-04 — Processing latency:**
- `kafpy.handler.latency` — Histogram with labels: topic, handler_id, mode
  - Buckets: [0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0] seconds
  - Record elapsed time from dispatch to ack/nack

**OBS-05 — Consumer lag:**
- `kafpy.consumer.lag` — Gauge with labels: topic, partition
  - Updated by background task polling `consumer.position()` and `consumer.highwater()` every 10s
  - Lag = highwater - committed_offset per partition

**OBS-06 — Queue depth:**
- `kafpy.queue.depth` — Gauge with labels: handler_id
  - Updated by polling `queue_snapshots()` every 10s
  - Source: QueueManager.queue_snapshots() returns HashMap<String, QueueSnapshot>

**OBS-07 — DLQ volume:**
- `kafpy.dlq.messages` — Counter with labels: dlq_topic, original_topic
  - Increment when message is produced to DLQ topic
  - Wire into `SharedDlqProducer::produce()` in src/dlq/produce.rs

Create a `MetricsRegistry` struct holding all metric families (LazyCell + Registry pattern).
Create `PrometheusExporter` struct with a `metrics()` method returning a `Cow<'static, str>` of the Prometheus text format for exposing via `/metrics` endpoint.

Note: Consumer lag computation requires access to the rdkafka consumer position/highwater. The background task that polls for queue snapshots should also compute lag using `Consumer::position()` and `Consumer::highwater()`.
</action>
  <verify>cargo check --lib 2>&1 | tail -20</verify>
  <done>PrometheusSink records counters/histograms/gauges; MetricsRegistry holds all metric families; /metrics endpoint returns Prometheus text format</done>
</task>

<task type="auto">
  <name>Task 3: Add batch=True decorator support to Python runtime (PY-05)</name>
  <files>kafpy/runtime.py</files>
  <action>
Add `batch=True` parameter support to the existing `handler()` decorator in kafpy/runtime.py.

When `batch=True` is set:
- Set `handler_mode="batch_sync"` for sync handlers or `"batch_async"` for async handlers (detect via `inspect.iscoroutinefunction`)
- Use `batch_max_size` and `batch_max_wait_ms` parameters to configure the batch policy
- Pass to `register_handler()` which already supports batch_* parameters

Also add `batch` as a keyword-only argument alongside the existing parameters in `handler()`:

```python
def handler(
    self,
    topic: str,
    *,
    routing: object | None = None,
    timeout_ms: int | None = None,
    concurrency: int | None = None,
    batch: bool = False,
    batch_max_size: int = 100,
    batch_max_wait_ms: int = 1000,
) -> Callable[[Callable], Callable]:
```

When `batch=True`, detect sync vs async using `inspect.iscoroutinefunction(fn)` and set mode accordingly, then call `register_handler()` with appropriate `handler_mode` and batch parameters.

The BatchPolicy defaults (max_batch_size=100, max_batch_wait_ms=1000) should be passed to the Rust consumer via `add_handler()`.

Note: The Rust side already supports BatchSync/BatchAsync HandlerMode and BatchPolicy struct. This task adds the Python-side `batch=True` interface to the handler decorator.
</action>
  <verify>python -c "from kafpy.runtime import KafPy; print('import OK')"</verify>
  <done>handler(topic='...', batch=True) registers a batch handler; batch_max_size and batch_max_wait_ms passed to Rust</done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| Kafka message -> handler span | Untrusted W3C traceparent header crosses here; must handle malformed headers gracefully |

## STRIDE Threat Register

| Threat ID | Category | Component | Disposition | Mitigation Plan |
|-----------|----------|-----------|-------------|-----------------|
| T-04-01 | Tampering | W3C traceparent header | mitigate | `inject_trace_context()` only accepts valid 4-part dash-separated format; malformed headers are silently ignored (per W3C spec) |
| T-04-02 | Denial of Service | Queue depth gauge | accept | Bounded channels already limit queue depth; metric is informational only |
| T-04-03 | Information Disclosure | Prometheus /metrics endpoint | mitigate | Expose /metrics only on internal port or protect with auth; do not expose on public endpoint |
</threat_model>

<verification>
cargo check --lib passes; python -c "from kafpy import Consumer, KafPy" passes; grep -c "kafpy_handler_invoke" src/observability/tracing.rs returns > 0; grep -c "PrometheusSink" src/observability/metrics.rs returns > 0
</verification>

<success_criteria>
- OBS-01: Tracing span created per handler invoke with topic, partition, offset, handler_name, mode, attempt attributes
- OBS-02: W3C trace context from Kafka headers propagated and linked to handler span via parent context
- OBS-03: kafpy.message.throughput counter increments on each message delivery
- OBS-04: kafpy.handler.latency histogram records p50/p95/p99 latency per handler
- OBS-05: kafpy.consumer.lag gauge updated every 10s with highwater - committed offset
- OBS-06: kafpy.queue.depth gauge reflects current queue depth per handler
- OBS-07: kafpy.dlq.messages counter incremented on each DLQ produce
- PY-05: @handler(topic='...', batch=True) decorator works and passes batch policy to Rust
</success_criteria>

<output>
After completion, create `.planning/phases/04-observability/04-01-SUMMARY.md`
</output>
