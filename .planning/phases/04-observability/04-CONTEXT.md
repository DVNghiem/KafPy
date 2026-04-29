# Phase 4: Observability — Context

**Gathered:** 2026-04-29
**Status:** Ready for planning

## Phase Boundary

Phase 4 adds comprehensive observability: tracing spans per handler execution, message throughput/latency metrics, consumer lag gauges, queue depth gauges, and DLQ volume metrics. Batch handler support (PY-05) is also included.

## Prior Phase Decisions (from Phase 3)

- W3C trace context (`traceparent` header → `trace_id`, `span_id`) already implemented in `inject_trace_context()`
- `ExecutionContext` carries `trace_id`, `span_id`, `trace_flags` fields
- `HandlerConcurrency` via `Arc<Semaphore>` per handler already implemented
- Context manager (`with Consumer as c:`) already implemented

## Implementation Decisions Needed

### OBS-01/02 — Tracing Export

**Status:** `tracing` crate spans already exist in `src/observability/tracing.rs`. W3C trace context injection from Kafka headers already done in Phase 3.

**Decision needed:** How are traces exported?
- **Option A:** `tracing-subscriber` with console fmt (dev-friendly, no extra deps)
- **Option B:** OTLP exporter (standard for production, requires collector)
- **Option C:** Both — console for dev, OTLP configurable for prod

### OBS-03/04/05/06/07 — Metrics Export

**Decision needed:** Metrics export format?
- **Option A:** Prometheus-compatible endpoint (`/metrics`) via `prometheus` crate — standard, queryable
- **Option B:** OpenTelemetry metrics — more powerful but heavier
- **Option C:** Both — internal OTLP, exposed as Prometheus endpoint

### OBS-05 — Consumer Lag

**Decision needed:** How to compute consumer lag?
- Lag = high-watermark offset − committed offset per partition
- Requires calling `consumer.position()` + `consumer.highwater()` on rdkafka

### PY-05 — Batch Handler Support

Already in scope (PY-05). From Phase 3 PLAN: `@handler(topic="...", batch=True)` delivers batches to handler as `List[(message, context)]`. Need to confirm: batch-level ack/nack or per-message?

## Canonical References

- `src/observability/tracing.rs` — existing span infrastructure
- `src/observability/metrics.rs` — existing metrics infrastructure
- `src/observability/runtime_snapshot.rs` — existing runtime introspection
- `src/python/handler.rs` — `HandlerMode::BatchSync/BatchAsync` already exist
- `.planning/phases/03-python-handler-api/03-PLAN.md` — PY-05 batch handler spec

## Specific Ideas

From ROADMAP.md Phase 4:
- Span attributes: topic, partition, offset, handler_name, attempt
- Metrics: message throughput (msg/s per topic/handler), processing latency histograms (p50/p95/p99), consumer lag, queue depth gauges, DLQ volume
- Batch handler: `@handler(topic="...", batch=True)` — delivered as list of (message, context) tuples

## Deferred Ideas

- OBS-08 (OpenTelemetry OTLP exporter) → v2
- OBS-09 (Prometheus exporter endpoint) → covered by Option A above
- ADV-01 (State persistence) → future phase
- ADV-02 (Windowed aggregations) → future phase
- ADV-03 (Schema registry) → future phase
- ADV-04 (Exactly-once semantics) → v2
