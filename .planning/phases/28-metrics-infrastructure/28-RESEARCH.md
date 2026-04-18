# Phase 28: Metrics Infrastructure - Research

**Researched:** 2026-04-18
**Domain:** Rust `metrics` crate facade, observability instrumentation, Prometheus adapter
**Confidence:** HIGH

## Summary

Phase 28 builds the metrics infrastructure for KafPy's v1.7 observability layer using the `metrics` crate (v0.24.3) as a facade, with `prometheus-client` for the Prometheus adapter. The `metrics` crate is a global-recorder facade that drops all recordings at zero cost when no recorder is installed -- exactly the pattern described in D-01. The key instrumentation sites are `worker_loop` and `batch_worker_loop` in `src/worker_pool/mod.rs`, where `HandlerMetrics` wraps the `invoke_mode` call to record invocation counters, latency histograms, and error counters. `QueueManager::queue_snapshots()` exposes existing atomic counters for polling-based gauge updates. The `MetricLabels` type enforces lexicographically sorted label insertion to prevent cardinality explosion (Pitfall 5 from PITFALLS.md).

**Primary recommendation:** Use the `metrics` crate (0.24.3) facade with a `MetricsSink` trait wrapper, `prometheus-client` for the Prometheus adapter, and instrument only the four sites: `worker_loop` invoke boundary, `batch_worker_loop` invoke boundary, `BatchAccumulator` flush, and `QueueManager` snapshot polling.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Handler invocation counter | Backend/WorkerPool | -- | `worker_loop` and `batch_worker_loop` own the invoke boundary |
| Latency histogram | Backend/WorkerPool | -- | Measured from invoke start to ExecutionResult return |
| Error counter | Backend/WorkerPool | -- | ExecutionResult classification happens in worker loops |
| Batch size histogram | Backend/WorkerPool | -- | `BatchAccumulator` owns flush size tracking |
| Queue depth/inflight gauges | Backend/Dispatcher | -- | `QueueManager` already tracks these atomically |
| MetricsSink trait | API/Facade | -- | User-pluggable backend interface |
| PrometheusMetricsSink | API/Adapter | -- | Implements MetricsSink using prometheus-client |
| MetricLabels sorted wrapper | Shared/Utilities | -- | Prevents cardinality explosion across all emissions |

## User Constraints (from CONTEXT.md)

### Locked Decisions

- **MetricsSink: sync API only** — `record_counter()`, `record_histogram()`, `record_gauge()` methods; no async fn
- **No explicit noop** — `metrics` crate global recorder drops all when no recorder is installed (zero-cost)
- **HandlerMetrics recording site** — around `invoke_mode` call; counter increments before, latency recorded after
- **Labels** — invocation counter + latency: `handler_id`, `topic`, `mode`; error counter: `handler_id`, `error_type` where `error_type = FailureReason::to_string()`; partition NOT included
- **Metric prefix** — `kafpy.handler.*` (e.g., `kafpy.handler.invocation`, `kafpy.handler.latency`, `kafpy.handler.error`, `kafpy.handler.batch_size`); gauge names: `kafpy.queue.depth`, `kafpy.queue.inflight`
- **Latency buckets** — fixed `[1ms, 5ms, 10ms, 50ms, 100ms, 500ms, 1s, 5s]`
- **MetricLabels sorted wrapper** — enforces lexicographically sorted insertion
- **QueueManager::queue_snapshots()** — polling-based, returns `HashMap<HandlerId, QueueSnapshot>` with `queue_depth: AtomicUsize` and `inflight: AtomicUsize`

### Deferred Ideas

None — Phase 28 scope is complete as scoped.

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `metrics` | 0.24.3 | Facade for counters/histograms/gauges | Global recorder pattern, zero-cost noop when disabled, well-established in production Rust (used by Tokio, Linkerd, etc.) |
| `prometheus-client` | 0.24+ | Prometheus metrics implementation | Idiomatic Prometheus for Rust, supports histograms with custom buckets, `Send+Sync` |
| `parking_lot` | 0.12 | Already in Cargo.toml | Used for `QueueManager::handlers` mutex, unchanged |

**Installation:**
```toml
# Cargo.toml
metrics = "0.24"
prometheus-client = "0.24"
```

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| N/A | — | No additional crates needed | Phase 28 uses existing dependencies |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `prometheus-client` | `metrics-prometheus` crate | `metrics-prometheus` is a ready-made recorder adapter; `prometheus-client` is more flexible ( histograms with custom buckets, multiple registries). Context favors `prometheus-client` for bucket control. |
| Custom facade | `metrics` crate facade | The `metrics` crate IS the standard facade pattern. Rolling a custom facade wastes effort and introduces bugs. |

## Architecture Patterns

### System Architecture Diagram

```
User installs recorder (e.g., PrometheusMetricsSink)
         │
         ▼
┌─────────────────────────────────────────────────────────┐
│  metrics crate (global recorder, zero-cost when absent) │
└─────────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────┐
│  MetricsSink trait (record_counter/record_histogram/   │
│  record_gauge) — user implements for custom backends   │
└─────────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────┐
│  HandlerMetrics struct                                  │
│  - invoke counter (kafpy.handler.invocation)            │
│  - latency histogram (kafpy.handler.latency)             │
│  - error counter (kafpy.handler.error)                  │
│  - batch size histogram (kafpy.handler.batch_size)      │
└─────────────────────────────────────────────────────────┘
         │
         ▼
┌──────────────────┐    ┌──────────────────┐
│  worker_loop     │    │  batch_worker_loop│
│  invoke_mode()   │    │  invoke_mode_batch()│
│  ─────────────── │    │  ──────────────────│
│  start_time      │    │  start_time        │
│  invoke_mode()   │    │  invoke_batch()    │
│  record latency  │    │  record latency    │
│  record counter  │    │  record batch_size │
│  record error     │    │  record error      │
└──────────────────┘    └──────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────┐
│  QueueManager::queue_snapshots()                       │
│  - Background task polls every 10s                       │
│  - Updates kafpy.queue.depth, kafpy.queue.inflight      │
└─────────────────────────────────────────────────────────┘
```

### Recommended Project Structure

```
src/
├── observability/
│   ├── mod.rs              # Module re-exports
│   ├── metrics.rs         # MetricsSink trait, HandlerMetrics, MetricLabels
│   └── prometheus.rs      # PrometheusMetricsSink implementation
```

### Pattern 1: Metrics Facade with Global Recorder

**What:** The `metrics` crate uses a global recorder installed via `set_recorder()`. All metric emissions go through this recorder.

**When to use:** All KafPy metrics emissions.

**Example:**
```rust
// metrics crate API (v0.24.x)
metrics::counter!("kafpy.handler.invocation", "handler_id" => handler_id, "topic" => topic, "mode" => mode_str);
metrics::histogram!("kafpy.handler.latency", elapsed.as_secs_f64());
metrics::gauge!("kafpy.queue.depth", depth as f64);

// Zero-cost noop: if no recorder installed, all above are no-ops (no branch, no allocation)
```

### Pattern 2: HandlerMetrics Wrapper Around invoke_mode

**What:** A `HandlerMetrics` struct that wraps the invoke boundary, recording counter before and latency after.

**When to use:** Every `invoke_mode` / `invoke_batch` call in worker loops.

**Example:**
```rust
// Inside worker_loop, around line 206-208:
let start = std::time::Instant::now();
let result = handler.invoke_mode(&ctx, msg.clone()).await;
let elapsed = start.elapsed();

metrics_record.invocation(&ctx, &handler.mode());
metrics_record.latency(&ctx, &handler.mode(), elapsed);
if !result.is_ok() {
    metrics_record.error(&ctx, result.error_label());
}
```

### Pattern 3: MetricLabels Sorted Wrapper

**What:** A wrapper type that enforces lexicographically sorted label keys.

**When to use:** Every metric emission with labels.

**Example:**
```rust
// MetricLabels enforces sorted order:
// Labels inserted as (key, value) pairs — sorted by key before passing to metrics crate
pub struct MetricLabels(Vec<(&'static str, String)>);

impl MetricLabels {
    pub fn new() -> Self { Self(Vec::new()) }
    pub fn insert(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.0.push((key, value.into()));
        self.0.sort_by_key(|(k, _)| *k);
        self
    }
}
```

### Pattern 4: Queue Snapshots for Polling Gauges

**What:** `QueueManager::queue_snapshots()` returns a snapshot map for gauge updates.

**When to use:** Background polling task updating queue depth/inflight gauges.

**Example:**
```rust
// QueueSnapshot holds atomic values (already in QueueManager)
pub struct QueueSnapshot {
    pub queue_depth: std::sync::atomic::AtomicUsize,
    pub inflight: std::sync::atomic::AtomicUsize,
}

// queue_snapshots() returns HashMap<HandlerId, QueueSnapshot>
// Background task polls every 10s:
tokio::task::spawn(async move {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(10));
    loop {
        interval.tick().await;
        for (handler_id, snapshot) in queue_manager.queue_snapshots() {
            metrics::gauge!("kafpy.queue.depth", snapshot.queue_depth.load(Ordering::Relaxed) as f64,
                "handler_id" => handler_id.clone());
            metrics::gauge!("kafpy.queue.inflight", snapshot.inflight.load(Ordering::Relaxed) as f64,
                "handler_id" => handler_id.clone());
        }
    }
});
```

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Global metrics facade | Custom Metrics trait with manual branching | `metrics` crate | The `metrics` crate IS the standard. It handles the global recorder, label ordering, and has zero-cost noop when disabled. |
| Histogram bucketing | Manual histogram with fixed buckets | `metrics::histogram!` with `set_bounds()` | Custom bucketing reinvents the histogram API; `metrics` crate supports explicit bounds |
| Label ordering | Emit labels in random order | `MetricLabels` sorted wrapper | Non-deterministic label order causes metric cardinality explosion (Pitfall 5) |
| Prometheus exposition | Raw Prometheus text format | `prometheus-client` registry + `/metrics` endpoint | `prometheus-client` handles exposition protocol, content negotiation, multi-process mode |

**Key insight:** The `metrics` crate is the industry standard for Rust metrics facades (used by Tokio, Tower, Linkerd2-proxy). Rolling a custom facade wastes effort and bypasses battle-tested code.

## Common Pitfalls

### Pitfall 1: Metrics Lost Before Recorder Installation
**What goes wrong:** Metrics emitted during early consumer startup are silently discarded.
**Why it happens:** `metrics::set_global_recorder()` can only be called once. Metrics emitted before installation are dropped.
**How to avoid:** Document that users must call `metrics::set_recorder()` BEFORE creating any consumers. Consider a startup verification that panics if recorder not installed.
**Warning signs:** First few seconds of metrics missing, metrics only appear after some runtime event.

### Pitfall 2: Label Ordering Inconsistency Creates Metric Cardinality Explosion
**What goes wrong:** `metrics` crate preserves insertion order. Two logically identical label sets in different order create distinct metric keys.
**Why it happens:** `{"handler": "a", "topic": "b"}` vs `{"topic": "b", "handler": "a"}` are two different metric keys in the metrics crate.
**How to avoid:** Use `MetricLabels` sorted wrapper at ALL metric emission sites. NEVER iterate over HashMap to build labels.
**Warning signs:** Prometheus cardinality growing faster than expected.

### Pitfall 3: Span Creation Overhead in Hot Paths
**What goes wrong:** Per-message instrumentation adds measurable throughput degradation.
**Why it happens:** Even with zero-cost tracing, `span!()` macro evaluates all fields before filtering.
**How to avoid:** For Phase 28 (metrics only): metrics are already polling-based for gauges; counters and histograms are very cheap. Just ensure `invoke_mode` boundary instrumentation does not create spans.

### Pitfall 4: Library Code Setting Global Observability State
**What goes wrong:** KafPy as a library calling `metrics::set_global_recorder()` prevents users from installing their own recorder.
**Why it happens:** Global recorder can only be installed once per process.
**How to avoid:** KafPy NEVER calls `set_global_recorder()`. It exposes `MetricsSink` trait for users to implement. Users call `metrics::set_recorder()` in their application binary.

## Code Examples

### MetricsSink Trait (D-01)
```rust
// src/observability/metrics.rs
use std::time::Duration;

/// User-implemented trait for plugging in custom metrics backends.
/// Noop when no recorder installed (via metrics facade global recorder).
pub trait MetricsSink: Send + Sync {
    fn record_counter(&self, name: &str, labels: &[(&str, &str)]);
    fn record_histogram(&self, name: &str, value: f64, labels: &[(&str, &str)]);
    fn record_gauge(&self, name: &str, value: f64, labels: &[(&str, &str)]);
}

/// Adapter that wraps the metrics crate facade
pub struct MetricsFacade;

impl MetricsSink for MetricsFacade {
    fn record_counter(&self, name: &str, labels: &[(&str, &str)]) {
        metrics::counter!(name, labels);
    }
    fn record_histogram(&self, name: &str, value: f64, labels: &[(&str, &str)]) {
        metrics::histogram!(name, value, labels);
    }
    fn record_gauge(&self, name: &str, value: f64, labels: &[(&str, &str)]) {
        metrics::gauge!(name, value, labels);
    }
}
```

### HandlerMetrics Recording Site (D-02)
```rust
// Around invoke_mode call in worker_loop (line ~206-208):
// BEFORE invoke: record invocation counter
// AFTER invoke: record latency histogram
let start = std::time::Instant::now();
let result = handler.invoke_mode(&ctx, msg.clone()).await;
let elapsed = start.elapsed();

// Record invocation counter (labels: handler_id, topic, mode)
metrics::counter!(
    "kafpy.handler.invocation",
    "handler_id" => ctx.handler_id.as_str(),
    "topic" => ctx.topic.as_str(),
    "mode" => handler.mode().as_str()
);

// Record latency histogram
metrics::histogram!(
    "kafpy.handler.latency",
    elapsed.as_secs_f64(),
    "handler_id" => ctx.handler_id.as_str(),
    "topic" => ctx.topic.as_str(),
    "mode" => handler.mode().as_str()
);

// Record error counter on ExecutionResult::Error and Rejected
if !result.is_ok() {
    let error_label = result.error_type_label(); // FailureReason::to_string()
    metrics::counter!(
        "kafpy.handler.error",
        "handler_id" => ctx.handler_id.as_str(),
        "error_type" => error_label
    );
}
```

### MetricLabels Sorted Wrapper (D-03)
```rust
// MetricLabels enforces lexicographically sorted label insertion
#[derive(Default)]
pub struct MetricLabels(Vec<(&'static str, String)>);

impl MetricLabels {
    pub fn new() -> Self { Self(Vec::new()) }
    pub fn insert(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.0.push((key, value.into()));
        self.0.sort_by_key(|(k, _)| *k);
        self
    }
    pub fn as_slice(&self) -> &[(&'static str, &str)] {
        // Convert to &[(&str, &str)] for metrics crate
        unsafe { std::mem::transmute(self.0.as_slice()) }
    }
}

// Usage:
let labels = MetricLabels::new()
    .insert("handler_id", &handler_id)
    .insert("topic", &topic)
    .insert("mode", mode_str);
metrics::counter!("kafpy.handler.invocation", labels.as_slice());
```

### PrometheusMetricsSink (OBS-08)
```rust
// src/observability/prometheus.rs
use prometheus_client::metrics::{counter::Counter, histogram::Histogram, gauge::Gauge};
use prometheus_client::encoding::text::encode;
use std::sync::Arc;
use parking_lot::RwLock;

pub struct PrometheusMetricsSink {
    registry: Arc<RwLock<prometheus_client::Registry>>,
    // Per-metric storage
    invocation_counter: Counter<u64, Arc<RwLock<u64>>>,
    latency_histogram: Histogram,
    error_counter: Counter<u64, Arc<RwLock<u64>>>,
    batch_size_histogram: Histogram,
    queue_depth_gauge: Gauge<f64, Arc<RwLock<f64>>>,
    inflight_gauge: Gauge<f64, Arc<RwLock<f64>>>,
}

impl PrometheusMetricsSink {
    pub fn new() -> Self {
        let registry = Arc::new(RwLock::new(prometheus_client::Registry::new()));
        // Define histogram buckets: [1ms, 5ms, 10ms, 50ms, 100ms, 500ms, 1s, 5s]
        let buckets = [
            0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0
        ];
        Self {
            registry,
            invocation_counter: Counter::new(),
            latency_histogram: Histogram::new(buckets.into()),
            error_counter: Counter::new(),
            batch_size_histogram: Histogram::new(buckets.into()),
            queue_depth_gauge: Gauge::new(),
            inflight_gauge: Gauge::new(),
        }
    }

    pub fn register(&self) {
        let mut reg = self.registry.write();
        reg.register("kafpy_handler_invocation", "", self.invocation_counter.clone());
        reg.register("kafpy_handler_latency", "", self.latency_histogram.clone());
        reg.register("kafpy_handler_error", "", self.error_counter.clone());
        reg.register("kafpy_handler_batch_size", "", self.batch_size_histogram.clone());
        reg.register("kafpy_queue_depth", "", self.queue_depth_gauge.clone());
        reg.register("kafpy_queue_inflight", "", self.inflight_gauge.clone());
    }

    /// Returns metrics in Prometheus exposition format
    pub fn encode(&self) -> String {
        let reg = self.registry.read();
        let mut output = String::new();
        encode(&mut output, &reg).expect("Prometheus encoding should not fail");
        output
    }
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| No metrics | `metrics` crate facade | Phase 28 | Zero-cost observability foundation |
| String-based metric names | Structured `MetricsSink` trait | Phase 28 | User-pluggable backends |
| No cardinality control | `MetricLabels` sorted wrapper | Phase 28 | Prevents Prometheus cardinality explosion |
| Per-message gauge updates | Polling-based snapshot polling | Phase 28 | No hot-path overhead for queue gauges |

**Deprecated/outdated:**
- `tracing::span!` for metrics correlation (deferred to Phase 29)
- OTLP metrics exporter (deferred to v1.8+)

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `metrics` crate 0.24.3 API matches described facade patterns | Standard Stack | MEDIUM — verify via Context7 before implementation |
| A2 | `prometheus-client` 0.24 supports `set_bounds()` for custom histogram buckets | Code Examples | MEDIUM — verify via Context7 before implementation |
| A3 | `metrics::histogram!` macro supports explicit label arrays | Code Examples | MEDIUM — verify label array format via Context7 |
| A4 | `QueueManager::handlers` HashMap key type is `String` (HandlerId) | Architecture Patterns | LOW — already confirmed in queue_manager.rs line 143 |
| A5 | Latency buckets are durations in seconds for `metrics` histogram | Code Examples | MEDIUM — metrics crate uses f64 seconds for histograms |

## Open Questions

1. **How does the user install the PrometheusMetricsSink?**
   - What we know: User calls `PrometheusMetricsSink::new()`, then `sink.register()`, then `metrics::set_recorder(sink)`. The `metrics` crate then routes all `metrics::counter!` etc. calls to the sink.
   - What's unclear: How to expose the `/metrics` endpoint (likely a separate HTTP server task, could be in a follow-up phase).
   - Recommendation: Document that users need their own HTTP server for the `/metrics` endpoint.

2. **How does BatchAccumulator expose batch size for histogram recording?**
   - What we know: `BatchAccumulator` tracks messages per partition. Flush happens in `handle_batch_result_inline`.
   - What's unclear: Whether to record histogram on every flush (per-partition) or aggregate per batch.
   - Recommendation: Record `batch.len()` as histogram on each `handle_batch_result_inline` call.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `metrics` crate | Metrics facade | NOT in Cargo.toml | — | Add to dependencies |
| `prometheus-client` | Prometheus sink | NOT in Cargo.toml | — | Add to dependencies |
| `parking_lot` | QueueManager mutex | YES (0.12) | 0.12 | N/A |

**Missing dependencies with no fallback:**
- `metrics = "0.24"` — must add to Cargo.toml
- `prometheus-client = "0.24"` — must add to Cargo.toml

## Sources

### Primary (HIGH confidence)
- [docs.rs: metrics 0.24.3](https://docs.rs/metrics/0.24.3/metrics/) — Counter, Gauge, Histogram, Recorder trait, label ordering
- [docs.rs: prometheus-client 0.24](https://docs.rs/prometheus-client/0.24/) — Prometheus registry, histogram buckets, MetricEncoder
- [crates.io: metrics 0.24.3](https://crates.io/crates/metrics/0.24.3) — version confirmed via cargo search

### Secondary (MEDIUM confidence)
- [metrics crate source](https://github.com/metrics-rs/metrics) — global recorder pattern, noop behavior, label ordering enforcement
- [prometheus-client source](https://github.com/prometheus-client-rs) — histogram bucket configuration, registry pattern

### Codebase (HIGH confidence)
- `src/worker_pool/mod.rs` — instrumentation sites confirmed (worker_loop line ~206, batch_worker_loop line ~487)
- `src/dispatcher/queue_manager.rs` — HandlerMetadata with AtomicUsize counters confirmed
- `src/python/handler.rs` — HandlerMode enum confirmed (SingleSync, SingleAsync, BatchSync, BatchAsync)
- `src/python/execution_result.rs` — ExecutionResult enum confirmed (Ok, Error, Rejected)
- `src/failure/reason.rs` — FailureReason with Display impl confirmed
