# Architecture Research: KafPy Observability Layer

**Domain:** Rust/PyO3 Kafka Consumer Observability Integration
**Researched:** 2026-04-18
**Confidence:** HIGH

## Executive Summary

KafPy's observability layer integrates three complementary systems: structured logging via the existing `tracing` facade, metrics via the `metrics` crate facade, and OpenTelemetry tracing via `tracing-opentelemetry`. The architecture follows a facade/abstraction pattern so users can wire their own backends. Key integration points are the `worker_loop` (handler invocation), `BatchAccumulator` (batch-level metrics), `ConsumerDispatcher` (Kafka-level metrics), and `QueueManager` (queue introspection).

## Existing Architecture

```
ConsumerDispatcher
    └── ConsumerRunner (Kafka stream)
    └── Dispatcher
            └── QueueManager (queue_depth, inflight atomics)
                    └── HandlerEntry { sender, metadata }
    └── RoutingChain (optional)

WorkerPool
    └── worker_loop / batch_worker_loop
            └── PythonHandler (invoke_mode dispatch)
            └── Executor (post-execution policy)
            └── OffsetCoordinator (ack tracking)
            └── RetryCoordinator (retry state)
            └── DlqRouter + SharedDlqProducer
    └── CancellationToken (shutdown)
```

**Current logging:** `tracing` facade with `tracing-subscriber` fmt layer. Structured fields via `tracing::info!`, `tracing::warn!`, `tracing::error!` macros scattered throughout `worker_loop`, `ConsumerDispatcher::run`, and `OffsetTracker`.

**Current metrics:** None -- queue depth and inflight tracked via `AtomicUsize` but not exposed as metrics.

---

## Recommended Observability Architecture

### System Overview

```
+-------------------------------------------------------------------------+
|                         KafPy Observability Layer                        |
+-------------------------------------------------------------------------+
|                                                                          |
|  +-------------------------------------------------------------------+  |
|  |                    Metrics Recorder (Facade)                      |  |
|  |  +----------+  +----------+  +----------+  +----------+           |  |
|  |  | Handler  |  |  Batch   |  |  Kafka   |  | Worker   |           |  |
|  |  | Metrics  |  | Metrics  |  | Metrics  |  |  Pool    |           |  |
|  |  +----+-----+  +----+-----+  +----+-----+  +----+-----+           |  |
|  |       |             |             |             |                  |  |
|  |  +----+-------------+-------------+-------------+----+             |  |
|  |  |              Metrics Trait Objects                |             |  |
|  |  |     (Counter, Histogram, Gauge per handler)      |             |  |
|  |  +----------------------+--------------------------+             |  |
|  +--------------------------+---------------------------------------+  |
|                             |                                               |
|  +--------------------------+---------------------------------------+  |
|  |         Tracing Layer     |  (tracing + tracing-opentelemetry)   |  |
|  |  +-----------------------+------------------------+             |  |
|  |  |              OpenTelemetry Spans                  |             |  |
|  |  |  per message  |  per handler invoke  |  per batch |             |  |
|  |  +-------------------------------------------------+             |  |
|  +-------------------------------------------------------------------+  |
|                                                                          |
|  +-------------------------------------------------------------------+  |
|  |                      Structured Log Sink                            |  |
|  |         (tracing-subscriber Layer -> user-configured backend)       |  |
|  +-------------------------------------------------------------------+  |
|                                                                          |
+-------------------------------------------------------------------------+
```

### Component Responsibilities

| Component | Responsibility | Implementation |
|-----------|----------------|----------------|
| `Observable` trait | Defines metrics + tracing contract for all instrumented components | New trait in `src/observability/` |
| `HandlerMetrics` | Per-handler: invocation count, latency histogram, error count, batch size | `metrics` crate counters + histograms |
| `BatchMetrics` | Per-batch: size distribution, flush reason, accumulation time | `metrics` crate histograms |
| `KafkaMetrics` | Kafka-level: consumer lag per TP, assignment size, committed vs HW | `rdkafka` stats callback + `metrics` gauges |
| `WorkerPoolMetrics` | Pool-level: idle/active/busy workers, queue depths | `metrics` gauges |
| `TracingLayer` | Bridges `tracing` spans to OpenTelemetry via `tracing-opentelemetry` | `tracing_subscriber::Layer` |
| `MetricsLayer` | Exposes `metrics` recordings via OpenTelemetry Meter | `tracing-opentelemetry::MetricsLayer` |
| `ObservableDecorator` | Attaches metrics + span context to `ExecutionContext` | New struct wrapping context |

---

## New Components

### 1. `src/observability/mod.rs` -- Observability Module Root

```rust
//! Observability facade -- metrics, tracing, and structured logging.
//!
//! Provides a pluggable backend architecture:
//! - Metrics: `metrics` crate facade (noop when no recorder installed)
//! - Tracing: `tracing` facade + optional OpenTelemetry layer
//! - Logging: `tracing-subscriber` layered on top of `tracing`

pub mod metrics;
pub mod tracing;
pub mod facade;
```

### 2. `src/observability/metrics.rs` -- Metrics Domain

```rust
/// Per-handler metric instruments.
/// All metrics use a "kafpy.handler." prefix with handler_id label.
pub struct HandlerMetrics {
    /// Number of handler invocations (success + error separately).
    pub invocations: Counter,
    /// Latency from message dispatch to execution complete.
    pub latency_ms: Histogram,
    /// Batch sizes observed at invocation time.
    pub batch_size: Histogram,
    /// Current inflight messages for this handler (gauge).
    pub inflight: Gauge,
    /// Queue depth at dispatch time.
    pub queue_depth: Gauge,
}

/// Kafka-level metrics.
/// Prefix: "kafpy.kafka."
pub struct KafkaMetrics {
    /// Consumer lag per topic-partition (high_watermark - committed_offset).
    pub consumer_lag: IntGauge,
    /// Number of assigned partitions.
    pub assignment_size: Gauge,
    /// Committed offset per topic-partition.
    pub committed_offset: IntGauge,
}

/// Worker pool metrics.
/// Prefix: "kafpy.pool."
pub struct WorkerPoolMetrics {
    pub idle_workers: Gauge,
    pub active_workers: Gauge,
    pub busy_workers: Gauge,
}
```

**Integration:** `HandlerMetrics` is created per `handler_id` via `Arc<MetricsGuard>` stored in `ExecutionContext`. The guard holds weak references; actual recording happens only if a global recorder is installed (facade pattern).

### 3. `src/observability/tracing.rs` -- OpenTelemetry Tracing Integration

```rust
/// Initializes OpenTelemetry tracing with OTLP exporter.
/// Call once at module init, before creating any consumers.
pub fn init_otel_tracing(service_name: &str, endpoint: &str) -> Result<(), ObservabilityError> {
    // 1. Build OTLP exporter (grpc-tonic)
    // 2. Create SdkTracerProvider with batch processor
    // 3. Create tracing-opentelemetry::layer()
    // 4. Compose with existing tracing_subscriber layers
    // 5. Set global tracer + propagator
}

/// Span names follow semantic conventions:
/// - "kafpy.handler.invoke" -- handler execution
/// - "kafpy.batch.invoke" -- batch execution
/// - "kafpy.dlq.produce" -- DLQ produce
/// - "kafpy.offset.commit" -- offset commit
/// - "kafpy.dispatcher.dispatch" -- message dispatch
```

**Key insight:** `tracing-opentelemetry` `MetricsLayer` (with `metrics` feature) automatically converts specially-named `tracing` events to OpenTelemetry metrics. This bridges the `metrics` facade to OpenTelemetry without double-instrumentation.

### 4. `src/observability/runtime.rs` -- Runtime Introspection

```rust
/// Exposes internal state for debugging/monitoring.
/// Thread-safe snapshot of WorkerPool + QueueManager + OffsetTracker state.
pub struct RuntimeSnapshot {
    pub workers: Vec<WorkerState>,  // idle / active / busy
    pub handlers: Vec<HandlerState>, // queue_depth, inflight, capacity
    pub partitions: Vec<PartitionState>, // committed_offset, pending, failed
}

/// Python-callable via PyO3 so users can inspect state from Python.
pub fn get_runtime_snapshot() -> Py<PyDict>;
```

---

## Modified Components

### 1. `ExecutionContext` (in `src/python/context.rs`)

**Change:** Add optional metrics guard and span context.

```rust
pub struct ExecutionContext {
    pub topic: String,
    pub partition: i32,
    pub offset: i64,
    pub worker_id: usize,
    // NEW:
    pub metrics: Option<Arc<HandlerMetrics>>,
    pub span: Option<Span>,
}
```

**Rationale:** `ExecutionContext` already flows through `worker_loop` and `batch_worker_loop`. Attaching metrics here means every handler invocation automatically gets instrumentation without changing call signatures.

### 2. `worker_loop` (in `src/worker_pool/mod.rs`)

**Change:** Record metrics at key points. Wrap handler invocation in a span.

```rust
async fn worker_loop(...) {
    // At message pickup:
    let span = tracer.span_builder("kafpy.handler.invoke")
        .with_attribute("handler_id", handler_id.clone())
        .with_attribute("topic", msg.topic.clone())
        .with_attribute("partition", msg.partition)
        .start(&tracer);

    let result = handler.invoke_mode(&ctx, msg.clone()).instrument(span).await;

    // After execution, record metrics:
    metrics.latency_ms.record(elapsed_ms as f64, &[
        KeyValue::new("handler_id", handler_id.clone()),
        KeyValue::new("result", result.variant_name()),
    ]);
    metrics.invocations.increment(&[
        KeyValue::new("handler_id", handler_id.clone()),
        KeyValue::new("result", result.variant_name()),
    ]);
}
```

**Rationale:** `worker_loop` is the hot path. Instrumenting here covers all 4 `HandlerMode` variants since `invoke_mode` is the single dispatch point.

### 3. `batch_worker_loop` (in `src/worker_pool/mod.rs`)

**Change:** Record batch-size histogram and flush-reason counter.

```rust
// At flush:
metrics.batch_size.record(batch.len() as f64, &[
    KeyValue::new("handler_id", handler_id.clone()),
    KeyValue::new("flush_reason", flush_reason.as_str()), // "size" | "deadline" | "shutdown"
]);

// Track accumulation time per partition:
let accumulation_time_ms = deadline.map(|d| d.elapsed().as_millis() as f64).unwrap_or(0.0);
metrics.accumulation_ms.record(accumulation_time_ms, &[
    KeyValue::new("handler_id", handler_id.clone()),
]);
```

### 4. `ConsumerDispatcher::run` (in `src/dispatcher/mod.rs`)

**Change:** Add dispatch span with topic/partition/offset attributes.

```rust
// Around each send_with_policy call:
let span = tracer.span_builder("kafpy.dispatcher.dispatch")
    .with_attribute("topic", msg.topic.clone())
    .with_attribute("partition", msg.partition)
    .with_attribute("offset", msg.offset)
    .start(&tracer);

let (outcome, pause_signal) = self.dispatcher.send_with_policy_and_signal(msg, policy)
    .instrument(span).await;
```

### 5. `QueueManager` (in `src/dispatcher/queue_manager.rs`)

**Change:** Expose queue depth and inflight as `metrics::Gauge` readings via periodic sweep.

```rust
impl QueueManager {
    /// Returns snapshot of all handler queue states for metrics recording.
    pub fn queue_snapshots(&self) -> Vec<HandlerQueueSnapshot> {
        let guard = self.handlers.lock();
        guard.iter().map(|(id, entry)| HandlerQueueSnapshot {
            handler_id: id.clone(),
            queue_depth: entry.metadata.get_queue_depth(),
            inflight: entry.metadata.get_inflight(),
            capacity: entry.metadata.capacity,
        }).collect()
    }
}
```

**Rationale:** `QueueManager` already has all this data in atomic counters. Exposing snapshots enables a periodic metrics recorder (e.g., every 10s) to update gauges without adding polling infrastructure to the hot path.

### 6. `OffsetTracker` (in `src/coordinator/offset_tracker.rs`)

**Change:** Expose committed offset + consumer lag via `KafkaMetrics`.

```rust
/// Periodic snapshot for metrics -- called by a background task.
/// Returns (topic, partition, committed_offset, high_watermark).
pub fn offset_snapshots(&self) -> Vec<PartitionOffsetSnapshot> {
    // Iterate all partitions, look up high watermark from runner
}
```

**Note:** High watermark requires `ConsumerRunner::position()` or stats callback. `OffsetTracker` already holds `Arc<ConsumerRunner>` for commits; expose it for lag calculation.

### 7. `PythonHandler` (in `src/python/handler.rs`)

**Change:** No structural change. The `invoke_mode` method is already the instrumented entry point from `worker_loop`. Metrics recording happens at the `worker_loop` call site, not inside `PythonHandler` itself.

### 8. `lib.rs` / `logging.rs`

**Change:** Refactor `Logger::init()` to accept an optional `ObservabilityConfig` that configures OTLP, Prometheus, or user-provided exporters.

```rust
/// Observability configuration -- passed to Logger::init() or a new Observability::init()
pub struct ObservabilityConfig {
    pub service_name: String,
    pub otlp_endpoint: Option<String>,      // OTLP gRPC endpoint
    pub prometheus_port: Option<u16>,        // Prometheus scrape endpoint
    pub log_format: LogFormat,              // json / pretty / simple
    pub log_level: String,                   // env-filter string
}
```

---

## Data Flow Changes

### Message Flow with Observability

```
Kafka Message
    |
    v
ConsumerDispatcher::run()
    | span: "kafpy.dispatcher.dispatch" {topic, partition, offset}
    v
Dispatcher::send_with_policy()
    | record: queue_depth gauge (snapshot)
    v
QueueManager -> HandlerChannel
    |
    v
worker_loop picks up message
    | span: "kafpy.handler.invoke" {handler_id, topic, partition}
    | record: latency_ms start_time
    v
PythonHandler::invoke_mode()
    |
    +-- SingleSync -> invoke() -> spawn_blocking -> Python callback
    +-- SingleAsync -> invoke_async() -> PythonAsyncFuture
    +-- BatchSync -> invoke_batch() -> spawn_blocking
    +-- BatchAsync -> invoke_batch_async() -> PythonAsyncFuture
    |
    v
ExecutionResult
    | span: set_status(Ok | Error)
    | record: latency_ms (stopwatch), invocations counter, batch_size histogram
    v
Executor::execute() -> OffsetCoordinator::record_ack / record_failure
    |
    v
OffsetTracker
    | span: "kafpy.offset.commit" (at commit time)
    | record: committed_offset gauge, consumer_lag gauge
    v
QueueManager::ack()
    | record: inflight gauge (decrement)
    v
DLQ routing if needed
    | span: "kafpy.dlq.produce"
```

---

## Architectural Patterns

### Pattern 1: Metrics Facade with Lazy Initialization

**What:** Use the `metrics` crate facade. Instrumented code records through `metrics::counter!()`, `metrics::histogram!()` macros. Actual recording happens only if a global recorder is installed.

**When:** For library code like KafPy where you don't know what backend the user will choose.

**Trade-offs:**
- Pro: Zero overhead when no recorder (noop implementation is atomic load + compare)
- Pro: Users choose their own backend (Prometheus, OpenTelemetry, Datadog, etc.)
- Con: Labels (handler_id, topic, partition) must be provided at every call site

**Example:**
```rust
// In worker_loop:
metrics::histogram!("kafpy.handler.latency_ms", elapsed_ms as f64, "handler_id" => handler_id.clone());

// At startup (user code):
metrics::_recorder(MetricsRecorder::OpenTelemetry(meter));
```

### Pattern 2: Tracing Span Hierarchy

**What:** Parent span = dispatch, child span = handler invocation, nested child = Python callback. `tracing`'s `instrument()` combinator propagates context automatically.

**When:** For correlating metrics across async boundaries.

**Trade-offs:**
- Pro: Automatic span propagation through `spawn_blocking` and `tokio::select!`
- Pro: `tracing-opentelemetry` `MetricsLayer` converts events to metrics without double instrumentation
- Con: Span lifetime must outlive the async operation -- use `Span::current()` for spawned tasks

### Pattern 3: Snapshot-Based Polling for Gauge Metrics

**What:** Gauge metrics (queue depth, inflight, worker state) are recorded by periodically sweeping the source of truth and recording the snapshot. Not updated inline on every operation.

**When:** For high-frequency counters that don't need per-event granularity (queue depths, pool state).

**Trade-offs:**
- Pro: No performance impact on hot path (atomics already exist, no extra work)
- Con: Slight staleness (10s polling interval typical)
- Con: Requires background task or periodic callback

**Example:**
```rust
// Background task every 10s:
tokio::spawn(async {
    let mut interval = tokio::time::interval(Duration::from_secs(10));
    loop {
        interval.tick().await;
        for snapshot in queue_manager.queue_snapshots() {
            GAUGE_QUEUE_DEPTH.set(snapshot.queue_depth as f64, &["handler_id" => snapshot.handler_id]);
        }
    }
});
```

### Pattern 4: OpenTelemetry Layer as Backend Bridge

**What:** `tracing-opentelemetry::layer()` + `SdkTracerProvider` + OTLP exporter bridges `tracing` spans to any OTLP-compatible backend (Jaeger, Tempo, CloudWatch, etc.). `MetricsLayer` bridges `tracing` events named `otel.*` to OpenTelemetry metrics.

**When:** For zero-vendor lock-in observability with a standard protocol.

**Trade-offs:**
- Pro: Industry standard, many compatible backends
- Pro: Single instrumented codebase produces traces + metrics + logs
- Con: OTLP overhead (batch processing mitigates this)
- Con: Semantic conventions require discipline

---

## Anti-Patterns to Avoid

### Anti-Pattern 1: Double Instrumentation

**What:** Instrumenting the same operation both with `metrics` counters AND with `tracing` events that carry metric names for `MetricsLayer`.

**Why:** Causes inflated counts or confusing data when both paths are active.

**Do this instead:** Choose one -- use `metrics` facade for numerical data, use `tracing` for categorical events (error types, retry reasons, routing decisions). `MetricsLayer` converts the latter to metrics automatically if you follow naming conventions.

### Anti-Pattern 2: Blocking in Span Enter/Exit

**What:** Performing I/O or expensive computation inside `Span::enter()` guards.

**Why:** `Span::enter()` is synchronous. Blocking there blocks the async executor.

**Do this instead:** Create spans using `tracer.span_builder()` + `.start()` outside the async task. Use `.instrument(future)` to attach the span to an async operation.

### Anti-Pattern 3: Storing Metric State in Instrumented Components

**What:** Adding `metrics::Counter` fields directly to `PythonHandler`, `QueueManager`, etc.

**Why:** Pollutes domain structs with observability concerns. Metrics should be external and looked up by ID.

**Do this instead:** Use the `metrics` global facade. Components don't know about metrics. Call sites use `metrics::histogram!()` with labels.

### Anti-Pattern 4: Missing Span Context on Spawned Tasks

**What:** Spawning a `tokio::spawn` without attaching the current span context.

**Why:** Child tasks appear as orphan spans in the trace.

**Do this instead:** Use `tracing::TaskContext::current()` or `tracing::future::Instrument::instrument(spawned, Span::current())`.

---

## Build Order

### Phase 1: Metrics Infrastructure (Foundation)
1. Add `metrics` crate dependency
2. Create `src/observability/metrics.rs` with `HandlerMetrics`, `KafkaMetrics`, `WorkerPoolMetrics` structs
3. Add `MetricsGuard` registry that creates instruments lazily per handler_id
4. Modify `worker_loop` to record `latency_ms` histogram and `invocations` counter at invocation result
5. Add `QueueManager::queue_snapshots()` for polling-based gauge updates

**Rationale:** Metrics are the most impactful observability signal. Start here to establish the recording pattern before adding tracing.

### Phase 2: Tracing Infrastructure
1. Add `opentelemetry`, `opentelemetry_sdk`, `opentelemetry-otlp` dependencies
2. Create `src/observability/tracing.rs` with `init_otel_tracing()` function
3. Add `tracing-opentelemetry::layer()` to subscriber stack in `Logger::init()`
4. Wrap `worker_loop` handler invocation in `span.instrument(future)`
5. Add dispatch span in `ConsumerDispatcher::run()`

**Rationale:** Tracing spans provide correlation context for metrics. The span hierarchy (dispatch -> handler -> offset commit) enables distributed-tracing-style analysis.

### Phase 3: Kafka-Level Metrics
1. Wire `rdkafka` stats callback to `KafkaMetrics` consumer_lag and assignment_size gauges
2. Add `OffsetTracker::offset_snapshots()` for committed_offset per TP
3. Implement background polling task (10s interval) to update lag gauges

**Rationale:** Consumer lag is the most important Kafka-specific metric. Requires `rdkafka` integration.

### Phase 4: Runtime Introspection API
1. Create `src/observability/runtime.rs` with `RuntimeSnapshot` struct
2. Add `get_runtime_snapshot()` PyO3 function
3. Expose worker pool state (idle/active/busy) via `WorkerPoolMetrics`

**Rationale:** Python users need runtime visibility. PyO3 bridge enables `consumer.runtime_snapshot()` API.

### Phase 5: Structured Logging Refinement
1. Refactor `logging.rs` into `ObservabilityConfig` with OTLP + Prometheus + log format options
2. Ensure all `tracing::info!` / `warn!` / `error!` calls in hot paths use structured fields
3. Add `LogFormat::Json` with OTLP-compatible field names

**Rationale:** Structured logs complement metrics/tracing. Ensure fields are consistent across all three signals.

---

## Integration Points Summary

| Component | What Changes | Metrics Recorded | Tracing Spans |
|-----------|-------------|------------------|---------------|
| `ConsumerDispatcher::run` | Dispatch span added | None (Kafka metrics handle this) | `kafpy.dispatcher.dispatch` |
| `Dispatcher::send_with_policy` | Snapshot recording | `queue_depth` gauge | None |
| `worker_loop` | Latency + count recording, span wrapping | `latency_ms`, `invocations`, `batch_size` | `kafpy.handler.invoke` |
| `batch_worker_loop` | Flush reason recording, accumulation time | `batch_size`, flush reason counter, `accumulation_ms` | `kafpy.batch.invoke` |
| `QueueManager` | `queue_snapshots()` method added | `inflight`, `queue_depth` gauges | None |
| `OffsetTracker` | `offset_snapshots()` method added | `committed_offset`, `consumer_lag` gauges | `kafpy.offset.commit` |
| `ExecutionContext` | `metrics: Option<Arc<HandlerMetrics>>` field | Carried through call chain | Span context field |
| `PythonHandler` | No change (instrumented at call site) | -- | -- |
| `Logger::init` | Refactored to `ObservabilityConfig` | None | Sets up global tracer |

---

## Sources

- [OpenTelemetry Rust Getting Started](https://opentelemetry.io/docs/languages/rust/getting-started/) -- SDK initialization, tracer provider
- [OpenTelemetry Rust Exporters](https://opentelemetry.io/docs/languages/rust/exporters/) -- OTLP exporter setup
- [tracing crate docs](https://docs.rs/tracing/latest/tracing/) -- Span, instrument macro, semantic conventions
- [tracing-subscriber Layer docs](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/layer/) -- layer composition, filtering
- [tracing-opentelemetry docs](https://docs.rs/tracing-opentelemetry/latest/tracing_opentelemetry/) -- OpenTelemetryLayer, MetricsLayer
- [opentelemetry_sdk metrics docs](https://docs.rs/opentelemetry_sdk/latest/opentelemetry_sdk/metrics/) -- Meter, Counter, Histogram, SdkMeterProvider
- [metrics crate docs](https://docs.rs/metrics/latest/metrics/) -- Counter, Gauge, Histogram, Recorder trait, facade pattern

---

*Architecture research for: KafPy Observability Layer*
*Researched: 2026-04-18*
