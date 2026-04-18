# Project Research Summary

**Project:** KafPy Observability Layer
**Domain:** Rust/PyO3 Kafka Consumer Observability Integration
**Researched:** 2026-04-18
**Confidence:** MEDIUM-HIGH

## Executive Summary

KafPy's observability layer integrates three complementary systems: structured logging via the existing `tracing` facade, metrics via the `metrics` crate facade, and OpenTelemetry tracing via `tracing-opentelemetry`. The architecture follows a facade/abstraction pattern so users can wire their own backends without KafPy claiming global state. Key instrumentation points are the `worker_loop` (handler invocation), `BatchAccumulator` (batch-level metrics), `ConsumerDispatcher` (Kafka-level metrics), and `QueueManager` (queue introspection).

Experts build this type of observability layer by starting with the metrics facade (zero-cost when no recorder is installed), then layering OpenTelemetry tracing on top for correlation, and finally exposing runtime introspection to Python via PyO3. The critical insight from research is that KafPy must never call `set_global_default()` or `set_global_recorder()` -- it must expose traits that users plug into their own observability infrastructure.

The main risks are async span lifetime bugs (using `.enter()` across await points), metric cardinality explosion from non-deterministic label ordering, and trace context loss across the PyO3 GIL boundary. These are all preventable with proper patterns documented in the pitfalls research.

## Key Findings

### Recommended Stack

The observability layer builds on KafPy's existing PyO3 + Tokio stack using three facade crates that are already partially in use or well-understood. The `metrics` crate provides a global recorder facade (zero-cost noop when not installed). The `tracing` facade handles structured logging. The `tracing-opentelemetry` crate bridges spans to any OTLP-compatible backend.

**Core technologies:**
- `metrics` crate (facade) -- counters, histograms, gauges with global recorder pattern -- zero-cost when disabled
- `tracing` facade -- structured logging already in use, needs field enrichment at key integration points
- `tracing-opentelemetry` + `opentelemetry-otlp` -- OTLP exporter for distributed tracing, feature-gated optional
- `rdkafka` stats callback -- consumer lag, assignment size, committed offsets via periodic snapshot

### Expected Features

**Must have (table stakes):**
- Structured logging via `tracing` with field-rich events (handler_id, topic, partition, offset, queue_depth) -- already uses tracing facade, needs enrichment
- Handler invocation counter -- increment per `invoke_mode` call, labels: handler_id, topic, mode
- Handler latency histogram -- Duration from invoke start to ExecutionResult, configurable buckets
- Error counter per handler -- increment on Error/Rejected results, labels: handler_id, error_type
- Queue depth / inflight gauges -- expose AtomicUsize values already tracked in QueueManager
- Python `status()` method -- `consumer.status()` returns dict with worker states, queue depths, per handler

**Should have (competitive):**
- `MetricsSink` trait + noop implementation -- pluggable backend, users wire Prometheus/DataDog/OTLP
- Prometheus sink adapter -- implements MetricsSink using prometheus-client crate
- Kafka consumer lag gauge -- rdkafka position() vs highwater per topic-partition
- OpenTelemetry trace hooks -- `enable_otel()` opt-in spans on handler invoke, commit, DLQ route
- Batch size histogram -- flush size distribution per partition

**Defer (v2+):**
- OTLP exporter sink -- full OTLP protocol export
- Alerting rules library -- pre-built Prometheus alerting rules
- Trace context propagation -- inject trace context into Kafka message headers

### Architecture Approach

The observability layer uses a layered architecture: `metrics` facade for numerical data, `tracing` facade for structured logs, and `tracing-opentelemetry` Layer for distributed tracing. A polling-based approach updates gauge metrics (queue depth, inflight) from periodic snapshots rather than inline updates to avoid hot-path overhead. Span context propagates via W3C tracecontext headers across the PyO3 GIL boundary, not via in-memory Span references (which are not Send+Sync).

**Major components:**
1. `src/observability/metrics.rs` -- HandlerMetrics, KafkaMetrics, WorkerPoolMetrics structs using `metrics` facade
2. `src/observability/tracing.rs` -- init_otel_tracing() function, OTLP exporter setup, span naming conventions
3. `src/observability/runtime.rs` -- RuntimeSnapshot for Python introspection via PyO3
4. `ObservableDecorator` -- attaches metrics + span context to ExecutionContext without polluting domain structs

### Critical Pitfalls

1. **Async span lifetime** -- Using `Span::enter()` across await points causes spans to exit prematurely and re-enter on next poll, breaking trace causality. Use `#[instrument]` or `span.in_scope()` instead.
2. **Global subscriber conflict** -- KafPy must never call `set_global_default()` or `set_global_recorder()`. Expose traits instead for users to plug into their own infrastructure.
3. **Metrics lost before recorder** -- Metrics emitted before recorder installation are silently discarded. Users must initialize observability before creating any consumers.
4. **Label ordering inconsistency** -- `metrics` crate preserves insertion order. Non-deterministic label order creates metric cardinality explosion. Use a sorted `MetricLabels` wrapper.
5. **PyO3 trace context propagation** -- Trace context is lost across `spawn_blocking` GIL boundary. Propagate via W3C tracecontext headers explicitly, not via in-memory Span references.

## Implications for Roadmap

Based on research, suggested phase structure:

### Phase 1: Metrics Infrastructure (Foundation)
**Rationale:** Metrics are the most impactful observability signal and the foundation everything else builds on. The `metrics` facade pattern is well-understood and zero-cost when disabled.
**Delivers:** `MetricsSink` trait, noop implementation, HandlerMetrics with invocation counter + latency histogram, QueueManager::queue_snapshots() for gauge polling
**Addresses:** Structured logging enrichment, handler invocation counter, handler latency histogram, error counter, queue depth gauges, MetricsSink trait
**Avoids:** Metric cardinality explosion (introduce MetricLabels sorted wrapper from start)

### Phase 2: Tracing Infrastructure
**Rationale:** Tracing spans provide correlation context for metrics. Must be implemented with correct patterns from the start since refactoring spans is costly.
**Delivers:** OTLP exporter setup, span wrapping in worker_loop, dispatch span in ConsumerDispatcher::run(), proper span context propagation via W3C headers
**Uses:** `tracing-opentelemetry`, `opentelemetry-otlp`
**Implements:** `src/observability/tracing.rs` with proper async patterns (no `.enter()` across await)
**Avoids:** Async span lifetime pitfall, PyO3 GIL context loss

### Phase 3: Kafka-Level Metrics
**Rationale:** Consumer lag is the most important Kafka-specific metric. Requires rdkafka stats callback integration which is a separate concern from handler instrumentation.
**Delivers:** KafkaMetrics (consumer_lag gauge, assignment_size gauge, committed_offset gauge), OffsetTracker::offset_snapshots(), background polling task (10s interval)
**Uses:** rdkafka stats callback
**Avoids:** Span overhead in hot path (Kafka metrics are polled, not per-message)

### Phase 4: Runtime Introspection API
**Rationale:** Python users need runtime visibility. PyO3 bridge enables `consumer.status()` API without exposing Rust internals.
**Delivers:** `RuntimeSnapshot` struct, `get_runtime_snapshot()` PyO3 function, Python `status()` method
**Implements:** `src/observability/runtime.rs`

### Phase 5: Structured Logging Refinement
**Rationale:** Structured logs complement metrics/tracing. Ensure all three signals use consistent field names. Last to ensure field conventions are established.
**Delivers:** ObservabilityConfig with OTLP + Prometheus + log format options, consistent field naming across all three signals
**Avoids:** OTel trace-only bridge misunderstood as log export (clarify in docs)

### Phase Ordering Rationale

- **Metrics first:** Facade pattern is simple to implement, zero-risk (noop when disabled), high value. Establishes recording patterns before adding tracing correlation.
- **Tracing second:** Must get async patterns right from the start. Span refactoring after code is instrumented is expensive. Context propagation across PyO3 boundary must be designed in, not retrofitted.
- **Kafka metrics third:** Requires rdkafka integration which is a separate subsystem. Polling-based (not per-message) so no hot-path impact.
- **Runtime introspection fourth:** Python API is the user-facing deliverable. Depends on all underlying metrics being in place.
- **Logging refinement last:** Field naming conventions should be established by now. Completes the three-signal picture.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 2 (Tracing):** OpenTelemetry context propagation across `spawn_blocking` -- niche integration pattern, needs API verification for `tracing-opentelemetry::with_context()`
- **Phase 3 (Kafka Metrics):** rdkafka stats callback thread-safety -- how to safely share state between rdkafka internal thread and Tokio runtime

Phases with standard patterns (skip research-phase):
- **Phase 1 (Metrics):** `metrics` crate facade pattern is well-documented and used in many production Rust projects
- **Phase 4 (Runtime Introspection):** PyO3 bridge patterns well-established, RuntimeSnapshot is a simple data struct

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | Uses existing pyo3 + tokio stack. `metrics` and `tracing` facades well-documented. OTel integration patterns verified via official docs. |
| Features | MEDIUM-HIGH | Feature set aligned with industry standards. MVP definition is clear. Some v2+ features (OTLP export, alerting) need more user validation. |
| Architecture | HIGH | Patterns well-established: facade for metrics, layered tracing, polling for gauges. Integration points clearly mapped. |
| Pitfalls | MEDIUM | Based on official crate docs (HIGH confidence for `tracing`, `metrics`) and GitHub issues (MEDIUM for OTel SDK edge cases). Some subtle async interactions need runtime validation. |

**Overall confidence:** MEDIUM-HIGH

### Gaps to Address

- **rdkafka stats callback integration:** Thread-safety model for sharing consumer statistics between rdkafka internal thread and Tokio runtime needs verification during Phase 3 planning. The existing `ConsumerRunner` architecture may already handle this, but the stats callback pattern was not explicitly researched.
- **OTel context propagation across `spawn_blocking`:** The `tracing-opentelemetry::with_context()` API for explicitly propagating span context across thread boundaries (to Python handlers via `spawn_blocking`) needs API-level verification -- docs suggest it works but implementation details are sparse.
- **Python handler span naming:** Whether Python-side instrumentation (if any) should follow Rust span conventions or define Python-specific semantic conventions was not explicitly addressed. Needs Python integration team input.

## Sources

### Primary (HIGH confidence)
- [tracing crate docs](https://docs.rs/tracing/latest/tracing/) -- Span, instrument macro, async/await patterns
- [tracing-subscriber Layer docs](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/layer/) -- layer composition, global default behavior
- [metrics crate docs](https://docs.rs/metrics/latest/metrics/) -- Counter, Gauge, Histogram, Recorder trait, facade pattern, label ordering
- [pyo3-async-runtimes docs](https://docs.rs/pyo3-async-runtimes/0.27.0/pyo3_async_runtimes/tokio/) -- `into_future` API, GIL behavior
- [PyO3 docs](https://pyo3.rs/) -- GIL lifetime, Send+Sync constraints
- [rdkafka STATISTICS documentation](https://github.com/confluentinc/librdkafka/blob/master/STATISTICS.md) -- consumer lag metrics format

### Secondary (MEDIUM confidence)
- [tracing-opentelemetry docs](https://docs.rs/tracing-opentelemetry/latest/tracing_opentelemetry/) -- reserved prefixes, context propagation
- [opentelemetry-rust GitHub](https://github.com/open-telemetry/opentelemetry-rust) -- known issues, OTLP configuration
- [prometheus-client Rust crate](https://docs.rs/prometheus-client/) -- idiomatic Prometheus metrics in Rust

### Tertiary (LOW confidence)
- Specific `tracing-opentelemetry::with_context()` API for thread-boundary propagation -- needs implementation verification

---
*Research completed: 2026-04-18*
*Ready for roadmap: yes*
