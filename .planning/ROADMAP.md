# KafPy Roadmap

**Project:** Rust-Core, Python-Logic Kafka Consumer Framework

---

## Milestones

- ✅ **v1.0 MVP** — Phases 1-6 (shipped 2026-04-29)
- 📋 **Next** — TBD

---

## Phase 1: Core Consumer Engine & Configuration (brownfield)

<details>
<summary>✅ Phase 1: Core Consumer Engine & Configuration — COMPLETED</summary>

- [x] Phase 1: Core Consumer Engine & Configuration (brownfield) — verified via 01-VERIFICATION.md

</details>

---

## Phase 2: Rebalance, Failure Handling & Lifecycle

<details>
<summary>✅ Phase 2: Rebalance, Failure Handling & Lifecycle — SHIPPED 2026-04-29</summary>

- [x] Phase 2: Rebalance, Failure Handling & Lifecycle (7/7 tasks) — completed 02-SUMMARY.md
  - CustomConsumerContext with rebalance callbacks (24dc1ba)
  - Failure classification integration
  - Graceful shutdown with SIGTERM (fa44f15)
  - Pause/resume and terminal blocking
  - DLQ routing and key-based routing
  - RetryConfig mapping

</details>

---

## Phase 3: Python Handler API

<details>
<summary>✅ Phase 3: Python Handler API — SHIPPED 2026-04-29</summary>

- [x] Phase 3: Python Handler API (1/1 task) — completed 03-SUMMARY.md
  - W3C trace context injection (38ff9b2)
  - Per-handler concurrency via Arc<Semaphore> (58fdc0d)
  - Handler decorator with concurrency param (4329b2)
  - Context manager (__enter__/__exit__) support

</details>

---

## Phase 4: Observability

<details>
<summary>✅ Phase 4: Observability — SHIPPED 2026-04-29</summary>

- [x] Phase 4: Observability (2/2 tasks) — completed 04-SUMMARY.md + 04-01-SUMMARY.md
  - Tracing spans with handler_name and attempt attributes (faa7c0b)
  - PrometheusSink + PrometheusExporter infrastructure (adc15af)
  - Throughput, latency, consumer lag, queue depth, DLQ metrics (OBS-03 through OBS-07)
  - Batch handler support with batch=True decorator (e5228bd)

</details>

---

## Phase 5: Builder Pattern Refactor

<details>
<summary>✅ Phase 5: Builder Pattern Refactor — SHIPPED 2026-04-29</summary>

- [x] Phase 5: Builder Pattern Refactor (2/2 tasks) — completed 05-SUMMARY.md + 05-01-SUMMARY.md
  - ConsumerConfigBuilder and ProducerConfigBuilder pyclass (7f7b304)
  - BuildError enum with missing-field validation
  - `#[allow]` suppressions audited with explanations

</details>

---

## Phase 6: Hardening

<details>
<summary>✅ Phase 6: Hardening — SHIPPED 2026-04-29</summary>

- [x] Phase 6: Hardening (2/2 tasks) — completed 06-SUMMARY.md + 06-01-SUMMARY.md
  - ConsumerError and DispatchError with structured fields (18e1961)
  - Actionable context: topic, partition, offset, broker, bytes_preview
  - Debug impls on error-context structs

</details>

---

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1 | v1.0 | 1/1 | Complete | brownfield |
| 2 | v1.0 | 7/7 | Complete | 2026-04-29 |
| 3 | v1.0 | 1/1 | Complete | 2026-04-29 |
| 4 | v1.0 | 2/2 | Complete | 2026-04-29 |
| 5 | v1.0 | 2/2 | Complete | 2026-04-29 |
| 6 | v1.0 | 2/2 | Complete | 2026-04-29 |

**v1.0 MVP shipped.** Full milestone history at `.planning/milestones/`.

---
*Last updated: 2026-04-29 after v1.0 milestone completion*