# Project: KafPy

**Type:** PyO3 Native Extension — Kafka producer/consumer framework
**Core Value:** High-performance Rust Kafka client with idiomatic Python API
**Tech Stack:** Rust + PyO3 + rdkafka + Tokio + Python

---

## What This Is

KafPy is a Python-facing Kafka framework where Rust provides the runtime/core engine and Python holds the business logic. PyO3 bridges the two. The pure-Rust consumer core handles Kafka protocol, while Python registers handlers/callbacks via bindings.

**Current module structure:**
- `src/consumer/` — ConsumerConfigBuilder, OwnedMessage, ConsumerRunner, ConsumerStream, ConsumerTask
- `src/dispatcher/` — Dispatcher, QueueManager, BackpressurePolicy, ConsumerDispatcher
- `src/worker_pool/` — WorkerPool, worker_loop, batch_worker_loop, PerPartitionBuffer, WorkerState, BatchState
- `src/python/` — PythonHandler, WorkerPool, ExecutionContext, ExecutionResult, Executor trait
- `src/offset/` — OffsetTracker, OffsetCoordinator, OffsetCommitter (split from coordinator/)
- `src/shutdown/` — ShutdownCoordinator, ShutdownPhase (split from coordinator/)
- `src/retry/` — RetryPolicy, RetryCoordinator (split from coordinator/)
- `src/coordinator/` — thin re-export layer (backward-compatible)
- `src/runtime/` — RuntimeBuilder for constructing ConsumerRuntime
- `src/routing/` — RoutingChain, Router, HandlerId (newtype), RoutingDecision
- `src/observability/` — MetricsSink, PrometheusMetricsSink, LogTracer, RuntimeSnapshot
- `src/dlq/` — DlqRouter, DlqMetadata, SharedDlqProducer
- `src/failure/` — FailureReason, FailureCategory, FailureClassifier
- `src/error.rs` — unified error re-exports (DispatchError, ConsumerError, CoordinatorError, PyError)
- `src/pyconsumer.rs` — PyO3 Consumer bridge (348→207 lines via RuntimeBuilder)
- `kafpy/__init__.py` — public Python API with __all__

## Key Decisions

| Decision | Rationale | Status |
|----------|-----------|--------|
| Rust core / Python business logic | Performance + idiomatic bindings | ✓ Validated |
| rdkafka for Kafka protocol | Battle-tested, async-capable | ✓ Validated |
| Tokio for async runtime | Native rdkafka compat, mpsc channels | ✓ Validated |
| PyO3-free consumer core | Clean separation, testable without Python | ✓ Validated |
| Per-topic bounded queue dispatch | Isolated backpressure per topic | ✓ Validated |
| BackpressurePolicy trait | Extensible backpressure (Drop/Wait/FuturePausePartition) | ✓ Validated |
| ConsumerDispatcher composition | Owns ConsumerRunner + Dispatcher, wires stream→dispatch | ✓ Validated |
| Py<PyAny> for callback storage | GIL-independent, sendable across threads | ✓ Validated |
| spawn_blocking for GIL | Minimal GIL hold window during Python execution | ✓ Validated |
| Executor trait | Future retry/commit/async/batch policies plug in here | ✓ Validated |
| OffsetCoordinator trait | Separates offset tracking from Executor policy | ✓ Validated |
| Highest contiguous offset commit | Only commit when all prior offsets acked | ✓ Validated |
| store_offset + commit coordination | enable.auto.offset.store=false, explicit coordination | ✓ Validated |
| RetryCoordinator 3-tuple | (should_retry, should_dlq, delay) controls retry and DLQ routing | ✓ Validated |
| has_terminal per-partition gating | Once terminal on a TP, that partition stops committing until restart | ✓ Validated |
| fire-and-forget DLQ produce | Bounded mpsc channel (~100), non-blocking, DLQ failures logged only | ✓ Validated |
| configurable DLQ topic naming | dlq_topic_prefix per consumer, default "dlq." | ✓ Validated |
| RoutingChain precedence | pattern→header→key→python→default with explicit default handler | ✓ Validated |
| RoutingDecision::Route(handler_id) | topic-based routing determines queue; handler_id annotates | ✓ Validated |
| RoutingDecision::Drop | fast-path: drop + advance offset (no DLQ, no retry) | ✓ Validated |
| RoutingDecision::Reject | fast-path: direct to DLQ (no RetryCoordinator) | ✓ Validated |
| RoutingDecision::Defer | routing inconclusive → default handler dispatch | ✓ Validated |
| HandlerId newtype wrapper | Prevents accidental interchange with topic names | ✓ Validated (v2.0) |
| ExecutionAction enum | Consolidates Error/Rejected retry/DLQ branching | ✓ Validated (v2.0) |
| coordinator/ split | offset/, shutdown/, retry/ with backward-compatible re-exports | ✓ Validated (v2.0) |
| WorkerState/BatchState enums | Explicit state machines replace Option/Bool state flags | ✓ Validated (v2.0) |

## Context

**Milestone v1.0:** Refactored duplicate consumer/message code into `src/consumer/` with clean separation between pure-Rust core and PyO3 bridge.

**Milestone v1.1:** Built dispatcher layer — `Dispatcher` routes `OwnedMessage` to per-topic bounded Tokio mpsc channels, with `QueueManager` tracking queue depth/inflight, `BackpressurePolicy` trait for extensible backpressure, and `ConsumerDispatcher` integrating with `ConsumerRunner`.

**Milestone v1.2:** Built Python execution lane — `PythonHandler` stores `Py<PyAny>` callbacks, `WorkerPool` pulls from handler queues and invokes via `spawn_blocking`, `Executor` trait for future policy extensibility.

**Milestone v1.3:** Offset commit coordinator — per-topic-partition ack tracking via `OffsetTracker`, highest-contiguous-offset commit logic via `OffsetCommitter`, out-of-order completion handling, `store_offset()` + `commit()` coordination for at-least-once delivery.

**Milestone v1.4:** Failure Handling & DLQ — FailureReason taxonomy, RetryPolicy with exponential backoff, DLQ routing with metadata envelope, per-partition commit gating with terminal state tracking, graceful shutdown DLQ flush.

**Milestone v1.5:** Extensible Routing — Pattern/header/key routing with optional Python fallback. RoutingChain wired into ConsumerDispatcher with all 4 RoutingDecision variants.

**Milestone v1.6:** Execution Modes — HandlerMode enum with 4 variants (SingleSync, SingleAsync, BatchSync, BatchAsync), BatchAccumulator with fixed-window flush, PythonAsyncFuture for async handler support, graceful shutdown drain.

**Milestone v1.7:** Observability Layer — MetricsSink trait + Prometheus adapter, OTLP tracing with W3C tracecontext propagation, KafkaMetrics for consumer lag/assignment size, RuntimeSnapshot, structured logging with LogTracer.

**Milestone v1.8:** Public API Foundation — PascalCase naming, snake_case methods, type hints, private Rust internals, `__all__` definitions, frozen configuration dataclasses, decorator-based handler registration, KafPy runtime lifecycle, Python exception hierarchy, README/guides packaging.

**Milestone v2.0:** Code Quality Refactor — Consolidated worker_pool/ from 1309-line god module to 4 focused files (mod.rs 96 lines, accumulator.rs, worker.rs, batch_loop.rs, pool.rs, state.rs). Split coordinator/ into offset/, shutdown/, retry/ modules. Added HandlerId newtype for type safety. Unified error re-exports in src/error.rs. All behavior preserved, no API changes.

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
- ✓ HandlerMode enum (SingleSync, SingleAsync, BatchSync, BatchAsync) — v1.6
- ✓ BatchAccumulator with fixed-window flush (size + timeout) — v1.6
- ✓ Sync Python handlers via spawn_blocking — v1.6
- ✓ Async Python handlers via pyo3-async-runtimes into_future — v1.6
- ✓ BatchExecutionResult (AllSuccess / AllFailure / PartialFailure) — v1.6
- ✓ GIL never held across Rust-side orchestration — v1.6
- ✓ Graceful shutdown drain: flush pending + commit before exit — v1.6
- ✓ MetricsSink trait with zero-cost facade (metrics crate) — v1.7
- ✓ Prometheus adapter (HandlerMetrics + KafkaMetrics per TP) — v1.7
- ✓ MetricLabels with lexicographically sorted label ordering — v1.7
- ✓ OTLP tracing via opentelemetry-otlp with W3C tracecontext propagation — v1.7
- ✓ Span context propagates at spawn_blocking boundary — v1.7
- ✓ Consumer lag, assignment size, committed offset gauges via rdkafka stats — v1.7
- ✓ RuntimeSnapshot (zero-cost when not called) — v1.7
- ✓ Structured logging with LogTracer, per-component levels, consistent field names — v1.7
- ✓ Public API conventions (PascalCase, snake_case, __all__, private internals) — v1.8
- ✓ Frozen configuration dataclasses (ConsumerConfig, RoutingConfig, RetryConfig, BatchConfig, ConcurrencyConfig) — v1.8
- ✓ Decorator-based and explicit handler registration — v1.8
- ✓ KafPy runtime lifecycle (start/stop/run) — v1.8
- ✓ Python exception hierarchy (KafPyError base) — v1.8
- ✓ README, docs/, kafpy/guides/ packaging — v1.8
- ✓ worker_pool/ split: mod.rs 1309→96 lines, accumulator/worker/batch_loop/pool/state files — v2.0
- ✓ ExecutionAction enum + handle_execution_failure() helper extraction — v2.0
- ✓ message_to_pydict() and flush_partition_batch() helper extractions — v2.0
- ✓ ConsumerDispatcher extracted to own file — v2.0
- ✓ RuntimeBuilder extracted to runtime/ module (pyconsumer.rs 348→207 lines) — v2.0
- ✓ coordinator/ split into offset/, shutdown/, retry/ — v2.0
- ✓ WorkerState and BatchState enums for explicit state machines — v2.0
- ✓ RetryState enum in RetryCoordinator — v2.0
- ✓ HandlerId newtype wrapper in routing/context.rs — v2.0
- ✓ Unified error re-exports in src/error.rs — v2.0
- ✓ Send+Sync compile-time assertions — v2.0

## Active Requirements

- TBD — v3.0 requirements to be defined

## Out of Scope

- Advanced rebalance logic — interfaces only, deferred
- Schema registry / Avro support — deferred
- Java/Node.js bindings — Python only
- Multi-consumer groups — deferred
- High-throughput producer — deferred
- Admin client — deferred
- Stream processing — deferred

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

*Last updated: 2026-04-20 after v2.0 Code Quality Refactor milestone*