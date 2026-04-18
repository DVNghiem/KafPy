# Pitfalls Research: Observability Layer for KafPy

**Domain:** Rust/PyO3 Observability — structured logging, metrics, and OpenTelemetry tracing for a Kafka consumer framework
**Researched:** 2026-04-18
**Confidence:** MEDIUM — Based on tracing-subscriber docs (HIGH), tracing docs (HIGH), opentelemetry-rust GitHub (MEDIUM), opentelemetry-rust docs (MEDIUM), metrics crate docs (HIGH), PyO3 docs (HIGH)

---

## Critical Pitfalls

### Pitfall 1: Async Span Lifetime — `Span::enter` Across Await Points

**What goes wrong:**
Spans appear to work but produce incorrect or broken traces. The span appears to never close, or parent-child relationships are inverted. In KafPy's worker loop where `spawn_blocking` calls Python handlers, spans may span multiple task switches, producing garbled trace trees.

**Why it happens:**
The `Span::enter()` pattern returns a drop guard that exits the span when dropped. In async code, if this guard is held across an `.await` point, the span will exit prematurely when the future is polled again, and re-enter on the next poll — breaking the temporal causality that tracing relies on.

```rust
// WRONG — drop guard held across await
async fn handler(&self, msg: OwnedMessage) {
    let span = span!(Level::INFO, "handle_message").enter();
    self.process(msg).await; // span exits here on suspend!
    // span re-enters on next poll with incorrect timing
}
```

**How to avoid:**
- Use `#[instrument]` attribute macro on async functions (it uses `Span::in_scope()` internally)
- Use `span.in_scope(async { ... })` pattern instead of `.enter()`
- In manual code, use `Span::in_scope(|| blocking_call())` for synchronous blocks within async contexts

**Warning signs:**
- Trace spans with durations spanning thousands of tasks
- Parent spans that appear to contain entire consumer lifetimes instead of individual operations
- Trace trees where child spans outlive their parents

**Phase to address:**
Core observability implementation phase — instrument the worker loop first with `#[instrument]` or `in_scope()`, not `.enter()`.

---

### Pitfall 2: Global Subscriber Conflict — Double Initialization

**What goes wrong:**
The application either silently drops all traces/metrics after the first subscriber is set, or panics with "a global default subscriber has already been set." In KafPy context, if the Python side sets a logging subscriber and Rust also tries to set one, one will fail or silently not work.

**Why it happens:**
`tracing_subscriber::set_global_default()` can only be called once per process. Subsequent calls return `Err` that is often ignored, or panic in debug mode. Similarly, `metrics::set_global_recorder()` can only be called once — metrics emitted before installation are permanently lost.

**How to avoid:**
- Never call `set_global_default()` from library code — only from binary/application code
- Use `try_init()` instead of `init()` to handle already-initialized gracefully
- Check return value of `set_global_default()` even if using `try_init()`
- Wrap the global subscriber in a `OnceCell` or `Lazy` to ensure deterministic initialization order
- For metrics: design a facade that can be initialized lazily, or document that the recorder must be installed before first use

**Warning signs:**
- Telemetry data appears in some processes/modules but not others
- "already initialized" errors appearing in logs
- Telemetry only working for some components

**Phase to address:**
Observability initialization phase — establish who owns global state (Python binary vs Rust library) before any instrumentation code runs.

---

### Pitfall 3: PyO3 GIL Bindings Breaking Observability Spans

**What goes wrong:**
Spans appear in Rust code but vanish when execution crosses into Python callbacks. Trace context is lost, parent-child relationships break, and spans for Python handler execution show no parent. In KafPy specifically, `spawn_blocking` calls Python handlers break trace continuity.

**Why it happens:**
PyO3's `Bound<'py, T>` and `Py<T>` are tied to the GIL lifetime `'py`. When you `spawn_blocking`, the GIL is released during the Python call. If your tracing span relies on GIL-bound context (e.g., storing `Py<PyAny>` spans or Python thread-local trace context), that context is inaccessible or invalid during the blocking call. OpenTelemetry context propagation between Rust async tasks and Python threads requires explicit handling.

**How to avoid:**
- Extract all data needed for span attributes *before* calling `spawn_blocking` — pass primitive types, not Python objects
- Use `tracing::info_span!("name", handler_id=%handler_id, topic=%topic)` instead of creating spans inside Python code
- Propagate trace context via `opentelemetry::global::get_text_map_propagator()` with W3C tracecontext headers across the thread boundary
- Consider using `tracing-opentelemetry` `with_context` to explicitly attach Rust span context to Python execution
- Keep Python handler instrumentation at the Rust-Python bridge boundary, not inside Python

**Warning signs:**
- Spans for `spawn_blocking` calls show zero child spans for the Python handler
- Trace parent-child relationships break at `spawn_blocking` boundaries
- Python handler spans appear as root spans instead of children

**Phase to address:**
Python handler instrumentation phase — instrument the PyO3 bridge (`worker_loop`, `spawn_blocking` call sites) with explicit context propagation, not the Python callbacks themselves.

---

### Pitfall 4: Metrics Lost Before Recorder Installation

**What goes wrong:**
Metrics (counters, histograms, gauges) are emitted during early consumer startup but never appear in the metrics backend. Consumer lag, message counts, and handler metrics all show as zeros or missing.

**Why it happens:**
The `metrics` crate uses a global recorder installed via `metrics::set_global_recorder()`. If any metric is emitted before this call (including during static initialization, module loading, or early consumer setup), it is silently discarded — there is no buffering or deferred recording.

**How to avoid:**
- Ensure the metrics recorder is installed before any KafPy consumer code can run — at application startup, before constructing any consumers
- Use a `metrics::describe_*` API at startup to declare all metric names upfront (helps catch missing recorders)
- Consider a `LazyMetrics` wrapper that queues emitted metrics until recorder is installed (with bounded queue to avoid memory leaks)
- Document clearly that users must initialize observability before creating consumers

**Warning signs:**
- First few seconds of metrics missing
- Metrics from module-level statics always zero
- Metrics only appear after some runtime event, not at startup

**Phase to address:**
Observability initialization phase — create a KafPy-specific observability setup function that must be called before any consumer is created.

---

### Pitfall 5: Label Ordering Inconsistency Creates Metric Cardinality Explosion

**What goes wrong:**
Metrics appear to have correct labels but query results show different values for logically identical metric keys. Over time, metric cardinality grows unexpectedly, causing memory issues in Prometheus or the metrics backend.

**Why it happens:**
The `metrics` crate preserves insertion order for labels — it does not sort them. Two keys with identical label pairs in different orders (`handler="a", topic="b"` vs `topic="b", handler="a"`) are treated as distinct metric keys. If handlers emit metrics with labels in non-deterministic order (e.g., iterating over a `HashMap`), this creates duplicate metric time series.

```rust
// WRONG — label order depends on iteration order
let labels = [("handler", &handler_id), ("topic", &topic)];
metric.record(&labels); // different order each time = different metric keys

// CORRECT — always emit labels in deterministic order
let labels = [
    ("handler", handler_id),  // always handler first
    ("topic", topic),         // always topic second
];
metric.record(&labels);
```

**How to avoid:**
- Always emit labels in lexicographically sorted order
- Create a helper struct `MetricLabels` that guarantees sorted order
- Never iterate over `HashMap` or unordered collections to build label arrays
- Set alert on metric cardinality growth rate

**Warning signs:**
- Prometheus cardinality growing faster than expected
- Metrics queries returning different results than application logs suggest
- Memory growth in metrics subsystem correlating with handler count

**Phase to address:**
Metrics implementation phase — define a `MetricLabels` type with sorted insertion that all metric emissions must use.

---

## Moderate Pitfalls

### Pitfall 6: Span Creation Overhead in Hot Paths

**What goes wrong:**
Adding tracing to the consumer message loop causes measurable throughput degradation (10-30% overhead). Disabling tracing restores performance, suggesting the instrumentation itself is the bottleneck.

**Why it happens:**
Even with "zero-cost" tracing, each `span!()` macro call evaluates all fields and checks subscriber interest. In KafPy's message consumption hot path (potentially millions of messages per hour), creating a span per message with attributes like `topic`, `partition`, `offset`, `handler_id` adds up. The `tracing` crate is designed to be near-zero-cost when disabled, but evaluation of field expressions (string allocations, formatting) happens before the "is anyone subscribed?" check.

**How to avoid:**
- Use `tracing::debug_span!()` for per-message spans (can be disabled at runtime without removing code)
- Gate expensive attribute extraction behind `if span.is_enabled()` checks
- Use `tracing::span!(Level::TRACE, "path", ...)` with target filtering rather than `tracing::info_span!` for hot paths
- Consider sampling: only create full spans for 1/N messages under load
- Avoid string formatting in span fields: `topic: %topic` (toSpan) not `topic: topic.clone()` (allocates)

**Warning signs:**
- Throughput drops correlate with enabling tracing
- CPU profiling shows time in `tracing` or `tracing_subscriber` crates
- Latency p99 increases proportionally to message rate

**Phase to address:**
Performance testing phase — benchmark with and without tracing enabled, measure overhead of per-message instrumentation.

---

### Pitfall 7: OpenTelemetry Metrics vs Logs — Wrong Crate

**What goes wrong:**
Users expect to emit logs to their OpenTelemetry collector (Jaeger, Tempo, etc.) but `tracing-opentelemetry` only bridges **traces**. Logs appear as unstructured events with no `otel.*` semantic convention fields. Conversely, users expecting structured log export via OTel get no output.

**Why it happens:**
`tracing-opentelemetry` bridges tracing spans and events to OpenTelemetry traces. It explicitly does **not** support logging to OpenTelemetry collectors. The OpenTelemetry Rust SDK has a separate Logs Bridge API (`opentelemetry_sdk::logs`) for log export. `tracing` logs go to `tracing_subscriber`, not to OpenTelemetry without additional appender crates.

**How to avoid:**
- Be explicit in KafPy docs: tracing-opentelemetry provides **trace/spans** only
- If both logs and traces to OTel are needed, add `opentelemetry-appender-tracing` crate
- For Python-side logging to flow into Rust tracing, use a log crate that implements the `tracing` subscriber interface (e.g., `tracing-log`)
- Consider separating concerns: structured logging via `tracing` facade, OpenTelemetry tracing via `tracing-opentelemetry`, and metrics via `metrics` + OTLP exporter

**Warning signs:**
- Users reporting "logs not appearing in Jaeger/Tempo"
- Logs appearing as trace events instead of separate log streams
- Confusion about where log output goes

**Phase to address:**
Feature definition phase — clarify what signals KafPy exports to which backends. Do not promise OTel log export unless it is implemented.

---

### Pitfall 8: Memory Leaks from Span Context Retention in Long-Running Consumers

**What goes wrong:**
Memory usage grows steadily over days of consumer operation. Memory profiles show growth in `tracing` span storage (`sharded_slab` slabs) and/or OpenTelemetry tracer provider.

**Why it happens:**
When spans are created but not properly closed (e.g., due to the async span enter pitfall), the `tracing-subscriber` registry holds references to in-flight spans. With `sharded_slab`, unclosed spans accumulate. OpenTelemetry SDK also holds span data in memory until batch export. In a consumer running indefinitely, even small per-span leaks compound.

**How to avoid:**
- Fix the async span lifetime pitfall (Pitfall 1) — proper span scoping prevents this
- Use `tracing_subscriber::fmt::with_span_events(FmtSpan::CLOSE)` to detect unclosed spans in development
- Set OpenTelemetry batch exporter with bounded queue size: `BatchSpanProcessor::builder(exporter, runtime).with_max_queue_size(2048)`
- Add metrics for span queue depth if available
- Periodically flush OpenTelemetry providers during consumer idle periods

**Warning signs:**
- Memory grows without bound over consumer lifetime
- Span count in tracing debug output increases but never decreases
- OpenTelemetry exporter queue grows unbounded

**Phase to address:**
Observability stress testing phase — run consumer for extended period with memory profiling, verify spans close properly.

---

### Pitfall 9: PyO3 `Send + Sync` Safety Blocking Observability Primitives

**What goes wrong:**
Cannot store tracing `Span` or metrics `Recorder` in `Arc<dyn SomeTrait>` that is `Send + Sync` because `Span` is not `Send` or `Sync`. Observable components that should be shareable across threads cannot include span context.

**Why it happens:**
`tracing::Span` is not `Send` or `Sync` because it is tied to the local tracing subscriber registry. If you try to store a span in a shared `Arc` across threads, it won't compile. Similarly, if you store `Py<PyAny>` callbacks in an `Arc` that is `Send`, the compiler will complain because `Py<PyAny>` is also not `Send + Sync`.

**How to avoid:**
- Never store `Span` directly in shared state — store span *metadata* (trace ID, span ID as strings/u128) instead
- Use `span.id()` and `span.context()` to get serializable identifiers
- For cross-thread observability, propagate trace context via headers (W3C tracecontext) not via in-memory span references
- For metrics: `metrics::Recorder` trait is `Send + Sync` — use the global recorder pattern, not shared recorder references

**Warning signs:**
- Compiler errors: "the trait `Send` is not implemented for `Span`"
- Compiler errors: "cannot send `Py<PyAny>` across thread boundaries"
- Workaround attempts: `unsafe impl Send` on custom types containing span references

**Phase to address:**
Architecture phase — design observability integration points that do not require sharing span references across threads. Propagate via context headers or global registry.

---

## Minor Pitfalls

### Pitfall 10: `tracing::span!` Field Expressions Evaluated Before Filtering

**What goes wrong:**
Even with `Level::DEBUG` disabled globally, expensive string formatting or object cloning in span field expressions still happens, causing performance overhead that "disappearing" when tracing is enabled.

**Why it happens:**
Rust macro arguments are evaluated before the macro body. The `tracing` crate's filtering happens inside the macro implementation, after all field expressions have been evaluated. So `span!(Level::INFO, "op", expensive_field = compute_expensive())` calls `compute_expensive()` even if the INFO level is filtered out.

**How to avoid:**
- Use `tracing::debug_span!()` for expensive field computations (can be compiled out with `RUSTFLAGS="--cfg tracing_unstable"`)
- Use lazy `tracing::Value` implementations for expensive fields
- Use `if tracing::enabled!(Level::INFO) { ... }` guard for truly expensive computation
- Move expensive operations into the span body: `span.in_scope(|| expensive_op())`

**Warning signs:**
- Performance overhead visible even with tracing disabled
- CPU time in user-defined functions called from span! macros
- Benchmark results vary dramatically between "tracing on" and "tracing off" builds

---

### Pitfall 11: Library Code Setting Global Observability State

**What goes wrong:**
KafPy as a library ships with default observability configuration that conflicts with application-level configuration. Users cannot use their own tracing subscriber or metrics recorder because KafPy already claimed the global default.

**Why it happens:**
KafPy code (or its internal dependencies) calls `tracing_subscriber::set_global_default()` or `metrics::set_global_recorder()`. When the user's application then tries to set its own (typically with richer configuration), the call fails or is silently ignored.

**How to avoid:**
- KafPy should **never** call `set_global_default()` or `set_global_recorder()`
- KafPy should expose configuration APIs that *plug into* user-provided subscribers
- Provide `KafPyTracingExt` trait that returns `tracing::Subscriber` + `tracing_opentelemetry::Layer` for users to add to their own subscriber
- Provide `KafPyMetricsProvider` trait that returns `metrics::Recorder` for users to register with their own registry
- Use `with_default(subscriber, || ...)` for scoped tracing in tests

**Warning signs:**
- "global default subscriber already set" errors when using KafPy with application tracing
- Users reporting KafPy tracing conflicts with their observability stack
- KafPy-owned metrics overriding application metrics

**Phase to address:**
Observability API design phase — define how KafPy integrates with user-provided observability infrastructure without claiming global state.

---

### Pitfall 12: OpenTelemetry `otel.*` Reserved Field Prefix Collision

**What goes wrong:**
Custom span attributes with names like `otel.name`, `otel.status_code`, or `otel.kind` behave unexpectedly — they either get overwritten by the tracing-opentelemetry layer or cause semantic convention violations.

**Why it happens:**
`tracing-opentelemetry` reserves all fields prefixed with `otel.` for OpenTelemetry semantic conventions. These fields have special handling: `otel.name` sets span name, `otel.kind` sets span kind, `otel.status_code` sets span status. User-defined fields with these names will be consumed by the layer rather than exported as custom attributes.

**How to avoid:**
- Document and enforce: KafPy span attribute names must not start with `otel.`
- Create a prefix convention for KafPy-specific attributes: `kafpy.handler`, `kafpy.topic`, `kafpy.partition`
- Use a lint rule or attribute validation to catch `otel.` prefix usage in KafPy instrumentation

**Warning signs:**
- Custom `otel.name` attribute not appearing in traces
- `otel.kind` being interpreted as span kind instead of custom attribute
- OTel backend showing "missing" expected semantic convention fields

---

### Pitfall 13: `metrics::describe_*` After Recorder Installation

**What goes wrong:**
Metric descriptions registered via `metrics::describe_counter!()` after the global recorder is installed are silently ignored. Users reading the KafPy documentation or code see metric descriptions that never appear in Prometheus.

**Why it happens:**
`metrics::describe_*` macros register metric descriptions with a static registry that is separate from the recorder but has the same once-per-process lifetime. If descriptions are registered after the recorder is installed, they may not be associated with the already-registered metrics.

**How to avoid:**
- Register all metric descriptions at application startup, before any consumer or recorder initialization
- In KafPy: provide a `KafPyObservability::describe_metrics()` method that users call at startup before creating consumers
- Consider using `metrics::describe_metric()` function instead of macros for dynamic description registration

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Skip `#[instrument]` on async fns | Faster to write | Broken traces, misleading latency data | Never in production |
| Use `unwrap()` on init results | Fewer error branches | Silent failures or panics in production | Only in tests |
| Emit metrics without sorted labels | Simpler code | Metric cardinality explosion, incorrect data | Only with static-known label sets |
| Log span enter/exit at DEBUG level | More detail | Verbose output, performance overhead | Only in development |
| Store span in shared state | Easier access | Not `Send+Sync`, breaks cross-thread | Never — redesign instead |
| OTel-only traces, no logs bridge | Less complexity | Users expecting unified logs | Only if docs explicitly say "traces only" |

---

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| `tracing` + Python logging | Python `logging` module not connected to Rust `tracing` | Use `tracing_log::LogTracer` or `tracing-journald` |
| `tracing-opentelemetry` + tokio | Span context lost across `spawn_blocking` | Use `tracing_opentelemetry::with_context()` explicitly |
| `metrics` + Prometheus | Histogram buckets not defined | Use `metrics::Histogram` with explicit `buckets()` configuration |
| OpenTelemetry + batch export | Forgetting `runtime.block_on()` for async exporters | Use `tokio::runtime::Handle::current().block_on()` for shutdown |
| `rdkafka` + tracing | Missing span context on poll/commit | Wrap `consumer.poll()` loop in `in_scope()` span |
| Python callbacks + Rust tracing | No trace continuity across GIL boundary | Propagate via W3C tracecontext headers explicitly |

---

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Per-message span creation | 10-30% throughput overhead | Use DEBUG-level spans, sample, gate expensive fields | Message rates > 10k/sec |
| Unbounded OTel batch queue | Memory grows with export lag | Set `max_queue_size` on `BatchSpanProcessor` | Slow export backend (network issues) |
| String allocation in span fields | GC pressure, latency spikes | Use `tracing::field::display` or `to_span()` not `.to_string()` | High-cardinality attributes |
| Global subscriber lock contention | Thread serialization under load | Use `tracing_subscriber::layer::Layer::boxed()` with `parking_lot` | High thread count, high trace volume |
| Metrics label iteration order | Cardinality explosion | Always sort labels before emission | Large handler count, many topics |

---

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| Embedding message payload in span attributes | Data leak to tracing backend | Never put message content in spans; only metadata (topic, partition, offset) |
| Trace context propagation to untrusted systems | Trace injection attacks | Validate W3C tracecontext headers before using |
| Sensitive fields in metric labels | Metric backend data exposure | Audit metric labels for PII (user IDs, session tokens) |
| Unbounded queue for batch export | Memory exhaustion DoS | Always set bounded queue size with overflow handling |

---

## UX Pitfalls

| Pitfall | User Impact | Better Approach |
|---------|-------------|-----------------|
| No default observability configuration | Users must wire everything manually | Provide `KafPyObservability::starter()` that sets up sensible defaults |
| Different metric names for same signal | Inconsistent dashboards | Standardize metric naming convention: `<module>_<signal>_<name>` |
| Missing documentation on backend support | Users don't know what backends work | Explicitly document supported backends (Prometheus, OTLP, Jaeger) |
| Trace context opt-in only | Spans only if user explicitly enables | Provide zero-cost tracing via `tracing` facade that defaults to noop when no subscriber |
| Metrics startup order confusion | "metrics not appearing" support tickets | Provide `KafPyObservability::verify()` that checks recorder is installed |

---

## "Looks Done But Isn't" Checklist

- [ ] **Tracing:** Span created in `worker_loop` but **not using `#[instrument]` or `in_scope()`** — verify no `.enter()` across await points
- [ ] **Tracing:** Global subscriber claimed **by library code** — verify only application code calls `set_global_default()`
- [ ] **Metrics:** Labels emitted **without sorted order** — verify `MetricLabels` type enforces ordering
- [ ] **Metrics:** Recorder potentially **not installed** before first use — verify recorder installation at app startup
- [ ] **OpenTelemetry:** Tracing-only bridge **misunderstood as log export** — verify docs clarify "traces only"
- [ ] **PyO3 bridge:** Trace context **not propagated** across `spawn_blocking` — verify W3C tracecontext headers are passed
- [ ] **PyO3 bridge:** Python handler spans **appear as root spans** — verify parent context is extracted before blocking call
- [ ] **Performance:** Per-message span **allocated strings** — verify `to_span()` or display used, not `.to_string()`
- [ ] **Memory:** Unclosed spans **accumulating** — verify `FmtSpan::CLOSE` enabled in dev, spans always exit
- [ ] **OTel:** Reserved `otel.*` prefix **used for custom fields** — verify no `otel.name`, `otel.kind` in KafPy attributes

---

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Double global subscriber init | LOW | Switch to `try_init()`, add `is_initialized()` guard |
| Lost metrics from early emission | MEDIUM | Restart consumer after recorder installed; add startup verification |
| Span enters across await | MEDIUM | Refactor to `#[instrument]` or `in_scope()`; redeploy |
| PyO3 trace context lost | HIGH | Add explicit context propagation at bridge; requires code change |
| Metric cardinality explosion | MEDIUM | Query backend for duplicate label permutations; fix emission order |
| Memory leak from unclosed spans | MEDIUM | Add span lifecycle verification; restart affected processes |
| Library claiming global state | HIGH | Refactor to facade pattern; update all call sites |

---

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| Async span lifetime | Core observability implementation | Check all spans use `#[instrument]` or `in_scope()`, not `.enter()` across await |
| Global subscriber conflict | Observability initialization | Ensure KafPy never calls `set_global_default()`; use facade pattern |
| PyO3 trace context propagation | Python handler instrumentation | Verify trace IDs present in Python callbacks via propagated headers |
| Metrics before recorder | Observability initialization | Add startup verification that recorder is installed before consumers |
| Label ordering | Metrics implementation | Use `MetricLabels` wrapper; audit all metric emissions |
| Span overhead in hot paths | Performance testing | Benchmark with/without tracing; gate expensive fields |
| Memory leaks | Stress testing | Run 24h+ with memory profiling; verify span count stabilizes |
| Send+Sync blocking | Architecture design | Verify no `Span` stored in `Arc<dyn Trait>`; use context propagation not references |
| Library global state | API design | Verify KafPy exposes traits for user to plug into their own subscriber |
| Reserved prefix collision | Observability API | Add lint/validation for `otel.*` prefix in KafPy attributes |

---

## Sources

- [tracing crate docs — async/await gotcha](https://docs.rs/tracing/latest/tracing/) (HIGH)
- [tracing-subscriber docs — global default conflicts, performance](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/) (HIGH)
- [tracing-opentelemetry docs — reserved prefixes, context propagation](https://docs.rs/tracing-opentelemetry/latest/tracing_opentelemetry/) (HIGH)
- [metrics crate docs — single recorder, label ordering](https://docs.rs/metrics/latest/metrics/) (HIGH)
- [opentelemetry-rust GitHub — known issues, OTLP configuration](https://github.com/open-telemetry/opentelemetry-rust) (MEDIUM)
- [PyO3 docs — GIL lifetime, Send+Sync](https://pyo3.rs/) (HIGH)
- [tokio-rs/tracing — best practices](https://github.com/tokio-rs/tracing) (HIGH)
