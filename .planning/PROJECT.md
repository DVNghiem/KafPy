# Project: KafPy

**Type:** PyO3 Native Extension — Kafka producer/consumer framework
**Core Value:** High-performance Rust Kafka client with idiomatic Python API
**Tech Stack:** Rust + PyO3 + rdkafka + Tokio + Python

---

## What This Is

KafPy is a Python-facing Kafka framework where Rust provides the runtime/core engine and Python holds the business logic. PyO3 bridges the two. The pure-Rust consumer core handles Kafka protocol, while Python registers handlers/callbacks via bindings.

Current status (after Milestone v1.4):
- `src/consumer/` — pure-Rust consumer core: `ConsumerConfigBuilder`, `OwnedMessage`, `ConsumerRunner`, `ConsumerStream`, `ConsumerTask`
- `src/dispatcher/` — message dispatcher: `Dispatcher`, `QueueManager`, `BackpressurePolicy`, `BackpressureAction`, `ConsumerDispatcher`
- `src/python/` — Python execution lane: `PythonHandler`, `WorkerPool`, `ExecutionContext`, `ExecutionResult`, `Executor` trait
- `src/coordinator/` — offset coordinator: `OffsetTracker`, `OffsetCommitter`, `OffsetCoordinator` trait
- `src/failure/` — failure classification: `FailureReason`, `FailureCategory`, `FailureClassifier` trait
- `src/retry/` — retry policy: `RetryPolicy`, `RetrySchedule` with exponential backoff + jitter
- `src/dlq/` — DLQ routing: `DlqRouter` trait, `DlqMetadata`, `SharedDlqProducer`
- `src/pyconsumer.rs` — PyO3 bridge: `Consumer` pyclass wrapping `ConsumerRunner`
- `src/config.rs` — Python-facing `ConsumerConfig` / `ProducerConfig` (PyO3)
- `src/kafka_message.rs` — PyO3 `KafkaMessage` wrapping `OwnedMessage`
- `src/produce.rs` — PyO3 `PyProducer`
- `kafpy/__init__.py` — public Python API

## Key Decisions

| Decision | Rationale | Status |
|----------|-----------|--------|
| Rust core / Python business logic | Performance + idiomatic bindings | Active |
| rdkafka for Kafka protocol | Battle-tested, async-capable | Active |
| Tokio for async runtime | Native rdkafka compat, mpsc channels | Active |
| StreamConsumer + mpsc channel | Owned message flow, no borrowed lifetimes | Active |
| PyO3-free consumer core | Clean separation, testable without Python | Active |
| Per-topic bounded queue dispatch | Isolated backpressure per topic | Active |
| BackpressurePolicy trait | Extensible backpressure handling (Drop/Wait/FuturePausePartition) | Active |
| ConsumerDispatcher composition | Owns both ConsumerRunner + Dispatcher, wires stream->dispatch | Active |
| Py<PyAny> for callback storage | GIL-independent, sendable across threads | Active |
| spawn_blocking for GIL | Minimal GIL hold window during Python execution | Active |
| Executor trait | Future retry/commit/async/batch policies plug in here | Active |
| OffsetCoordinator trait | Separates offset tracking from Executor policy | Active |
| Highest contiguous offset commit | Only commit when all prior offsets acked | Active |
| store_offset + commit coordination | enable.auto.offset.store=false, explicit coordination | Active |
| RetryCoordinator 3-tuple | (should_retry, should_dlq, delay) controls retry and DLQ routing | Active (v1.4) |
| has_terminal per-partition gating | Once terminal on a TP, that partition stops committing until restart | Active (v1.4) |
| fire-and-forget DLQ produce | Bounded mpsc channel (~100), non-blocking, DLQ failures logged only | Active (v1.4) |
| configurable DLQ topic naming | dlq_topic_prefix per consumer, default "dlq." | Active (v1.4) |
| RoutingChain precedence | pattern→header→key→python→default with explicit default handler | Active (v1.5) |
| RoutingDecision::Route(handler_id) | topic-based routing still determines queue; handler_id annotates | Active (v1.5) |
| RoutingDecision::Drop | fast-path: drop + advance offset (no DLQ, no retry) | Active (v1.5) |
| RoutingDecision::Reject | fast-path: direct to DLQ (no RetryCoordinator) | Active (v1.5) |
| RoutingDecision::Defer | routing inconclusive → default handler dispatch | Active (v1.5) |

## Context

**Milestone v1.0:** Refactored duplicate consumer/message code into `src/consumer/` with clean separation between pure-Rust core and PyO3 bridge.

**Milestone v1.1:** Built dispatcher layer — `Dispatcher` routes `OwnedMessage` to per-topic bounded Tokio mpsc channels, with `QueueManager` tracking queue depth/inflight, `BackpressurePolicy` trait for extensible backpressure, and `ConsumerDispatcher` integrating with `ConsumerRunner`.

**Milestone v1.2:** Built Python execution lane — `PythonHandler` stores `Py<PyAny>` callbacks, `WorkerPool` pulls from handler queues and invokes via `spawn_blocking`, `Executor` trait for future policy extensibility.

**Milestone v1.3:** Offset commit coordinator — per-topic-partition ack tracking via `OffsetTracker`, highest-contiguous-offset commit logic via `OffsetCommitter`, out-of-order completion handling, `store_offset()` + `commit()` coordination for at-least-once delivery.

**Milestone v1.4:** Failure Handling & DLQ — Phases 17-20 complete. FailureReason taxonomy, RetryPolicy with exponential backoff, DLQ routing with metadata envelope, per-partition commit gating with terminal state tracking, graceful shutdown DLQ flush.

**Current milestone (v1.5):** Extensible Routing — Pattern/header/key routing with optional Python fallback, Rust as fast-path owner.

**v1.5 shipped:** Phase 21 (RoutingCore), Phase 22 (PythonRouter), Phase 23 (DispatcherIntegration). RoutingChain wired into ConsumerDispatcher with all 4 RoutingDecision variants handled (Route→handler queue, Drop→offset advance, Reject→DLQ, Defer→default).

## Validated Requirements

- ✓ Per-topic bounded Tokio mpsc channel dispatch — v1.1
- ✓ Non-blocking send() with DispatchOutcome/DispatchError — v1.1
- ✓ Queue depth and inflight tracking per handler — v1.1
- ✓ BackpressurePolicy trait with BackpressureAction — v1.1
- ✓ ConsumerDispatcher integrating ConsumerRunner + Dispatcher — v1.1
- ✓ Tokio Semaphore per handler concurrency limiting — v1.1
- ✓ Py<PyAny> callback storage (GIL-independent) — v1.2
- ✓ WorkerPool with configurable N workers — v1.2
- ✓ ExecutionResult normalized to Rust — v1.2
- ✓ Executor trait for future policies — v1.2
- ✓ Per-topic-partition OffsetTracker with ack tracking — v1.3
- ✓ Execution completion events wired from ExecutionResult — v1.3
- ✓ Out-of-order completion handling with buffering — v1.3
- ✓ Highest contiguous acknowledged offset calculation — v1.3
- ✓ store_offset() + commit() coordination for at-least-once delivery — v1.3
- ✓ No duplicate commits when offset hasn't advanced — v1.3
- ✓ OffsetCoordinator trait separating offset tracking from Executor policy — v1.3
- ✓ Arc<dyn OffsetCoordinator> passed to WorkerPool — v1.3
- ✓ RetryPolicy struct with max_attempts, base_delay, max_delay, jitter_factor — v1.4
- ✓ RetrySchedule with exponential backoff + jitter — v1.4
- ✓ ConsumerConfig::default_retry_policy wired — v1.4
- ✓ PythonHandler per-handler retry_policy override — v1.4
- ✓ RetryCoordinator tracking per-message state — v1.4
- ✓ worker_loop retry scheduling (record_ack only on final success) — v1.4
- ✓ DlqMetadata struct with all 7 envelope fields — v1.4
- ✓ DlqRouter trait with DefaultDlqRouter — v1.4
- ✓ ConsumerConfig dlq_topic_prefix (configurable DLQ naming) — v1.4
- ✓ RetryCoordinator returns should_dlq signal — v1.4
- ✓ SharedDlqProducer fire-and-forget with bounded channel — v1.4
- ✓ worker_loop DLQ routing integrated (Error + Rejected arms) — v1.4
- ✓ PartitionState.has_terminal flag (set once, never cleared) — v1.4
- ✓ should_commit returns false when has_terminal=true (per-partition blocking) — v1.4
- ✓ flush_failed_to_dlq drains all failed (retryable + terminal) to DLQ — v1.4
- ✓ WorkerPool::shutdown calls flush_failed_to_dlq before graceful_shutdown — v1.4

- ✓ Pattern/header/key routing with Python fallback — v1.5
- ✓ Routing precedence: pattern → header → key → python → default — v1.5
- ✓ RoutingDecision trait: route, drop, reject, defer — v1.5
- ✓ Integration with existing handler queues + backpressure — v1.5

## Active Requirements

(None yet — plan next milestone)

## Out of Scope

- Advanced rebalance logic — interfaces only, deferred
- Schema registry / Avro support — deferred
- Java/Node.js bindings — Python only

---

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---

*Last updated: 2026-04-18 after v1.5 milestone*