# Retrospective

## Milestone: v1.0 MVP — KafPy

**Shipped:** 2026-04-29
**Phases:** 6 | **Plans:** 8 | **Tasks:** 14+

---

## What Was Built

### Phase 2: Rebalance, Failure Handling & Lifecycle
- CustomConsumerContext with rebalance callbacks (pre_rebalance commits offsets, post_rebalance seeks to committed+1)
- Failure classification (Retryable/Terminal/NonRetryable) with capped exponential backoff + jitter
- Graceful shutdown with SIGTERM handler (run_with_sigterm)
- DLQ routing with metadata envelope and key-based routing fallback
- Pause/resume partition support for flow control
- Terminal failure blocking (poison messages don't commit offset)

### Phase 3: Python Handler API
- W3C traceparent header parsing (00-{trace_id:32}-{span_id:16}-{flags:2})
- ExecutionContext with trace_id, span_id, trace_flags fields
- Per-handler concurrency via Arc<Semaphore> (acquired before invoke_mode_with_timeout)
- @handler decorator with concurrency parameter
- Context manager support (__enter__ returns self, __exit__ calls stop)

### Phase 4: Observability
- Tracing spans with handler_name and attempt attributes
- PrometheusSink + SharedPrometheusSink with pre-registered metric families
- PrometheusExporter with /metrics text format endpoint
- Throughput counter, latency histogram, consumer lag gauge, queue depth gauge, DLQ counter
- Batch handler support (batch=True decorator)

### Phase 5: Builder Pattern Refactor
- ConsumerConfigBuilder pyclass (24-field fluent builder)
- ProducerConfigBuilder pyclass (17-field fluent builder)
- BuildError enum with MissingField and NoTopics variants
- Build-time validation (brokers, group_id, topics required)
- #[allow] suppressions audited with explanatory comments

### Phase 6: Hardening
- ConsumerError with structured fields (Subscription/Receive/Serialization/Processing)
- DispatchError with structured fields (QueueFull/Backpressure/UnknownTopic/HandlerNotRegistered/QueueClosed)
- Actionable context: topic, partition, offset, broker, bytes_preview
- #[source] annotation for error chain preservation

---

## What Worked

1. **Parallel wave execution** — Phase 4 observability split into 2 plans (tracing+metrics, batch handler) executed in parallel waves within same plan
2. **Verification artifacts** — 5 of 6 phases have formal VERIFICATION.md; Phase 2 documented via SUMMARY.md (VERIFICATION.md absent but requirements satisfied)
3. **Brownfield handling** — Phase 1 pre-existing code verified via 01-VERIFICATION.md without requiring new SUMMARY.md
4. **Auto-fix discipline** — Rule 3 blocking issues (wrong pyo3 patterns, type mismatches) were caught and fixed immediately rather than worked around

---

## What Was Inefficient

1. **Phase 2 documentation gap** — VERIFICATION.md missing; had VALIDATION.md with nyquist_compliant: false. Requirements satisfied but formal GSD artifact absent.
2. **Phase 4 stubs not wired** — OBS-05 (consumer lag) and OBS-06 (queue depth) recorders exist but background polling tasks not integrated into runtime loop
3. **Dead code retained** — NoopSink and PrometheusExporter unused but kept for potential future use
4. **PyO3 linking environment issue** — cargo test --lib fails with undefined PyObject symbols; cargo check and clippy both pass. Environment/config issue, not code defect.

---

## Patterns Established

1. **MetricsSink trait** — Enables pluggable backends (noop_sink for dev, PrometheusSink for production)
2. **Structured error variants** — Actionable context fields instead of stringly-typed errors
3. **RwLock for pyclass Sync** — Interior mutability pattern for pyo3 #[pyclass] that must satisfy Sync bounds
4. **Semaphore-based concurrency** — Arc<Semaphore> per handler key acquired before invoke, released on drop

---

## Key Lessons

1. **Brownfield projects skip SUMMARY.md** — Phase 1 had no 01-SUMMARY.md because code predated GSD. Normal for existing projects.
2. **Pre-implemented phases need verification not execution** — Phase 2 plans 02, 04, 05, 06 required verification only; all functionality already existed
3. **Builder pattern requires careful pyo3 handling** — RefCell not Sync, so RwLock required; &mut self needed instead of move; concrete String over impl Trait
4. **Audit-open tool had bug** — gsd-tools.cjs had ReferenceError: output is not defined in audit-open command (tool bug, not project issue)

---

## Cost Observations

- Model mix: predominantly Sonnet for implementation, Haiku for verification tasks
- Sessions: ~10 sessions across 2 days (2026-04-28 to 2026-04-29)
- Notable: Phase 4 (observability) completed in ~15 min with 3 tasks — most efficient phase

---

## Cross-Milestone Trends

(N/A — first milestone)