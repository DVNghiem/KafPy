# KafPy — Rust-Core, Python-Logic Kafka Consumer Framework

## What This Is

A high-performance Kafka consumer framework where Rust owns the runtime core (concurrency, memory, Kafka protocol) and Python owns business logic (message handlers). Rust handles execution control, concurrency, memory management, and operational safety while Python provides message handlers and developer ergonomics.

## Core Value

Python developers can write Kafka message handlers easily while Rust controls the hard runtime problems (concurrency, backpressure, retries, DLQ, offset tracking, graceful shutdown).

## Requirements

### Validated

- ✓ Rust-based Kafka consumer engine managing full message-consumption lifecycle — v1.0
- ✓ Python handler registration via PyO3 boundary — v1.0
- ✓ Topic-based routing with bounded internal queues — v1.0
- ✓ Per-handler bounded queues with backpressure — v1.0
- ✓ At-least-once delivery with partition-aware offset tracking — v1.0
- ✓ Retryable vs non-retryable failure classification with backoff/jitter — v1.0
- ✓ DLQ handoff after retries exhausted — v1.0
- ✓ Graceful shutdown, queue draining, rebalance-safe partition handling — v1.0
- ✓ Python-first developer API (@handler decorator style) — v1.0
- ✓ Configurable: retry policies, batch size, flush timing, concurrency, DLQ, observability — v1.0
- ✓ Observability: metrics, tracing, runtime introspection — v1.0
- ✓ ConsumerConfig and ProducerConfig builder patterns — v1.0
- ✓ Structured error fields with actionable context — v1.0

### Active

- [ ] Work-stealing thread pool for blocking sync handlers (non-blocking poll)
- [ ] Streaming handler patterns (@stream_handler for persistent WebSocket/SSE)
- [ ] Handler middleware/chain (logging, metrics, retries)
- [ ] Async handler timeout (abort slow handlers)
- [ ] Async fan-in/fan-out (multiple async sources in one handler)

### Out of Scope

- Rust-only framework with thin Python bindings — This project IS the Pythonic layer
- Fake abstraction layer with unused config — Public APIs must reflect real runtime behavior
- Rule engine for everything — Business logic belongs in Python handlers
- Platform exposing more features than implemented — Minimal, honest, maintainable codebase
- Complex DSL or custom language — "Just Python" — no new DSL to learn
- Heavyweight infrastructure (ZooKeeper) — Single process, pip install and go
- Multi-language support (JS, Java, Go) — Python focus only initially
- Built-in ML/Data science libraries — Users bring their own

## Current Milestone: v1.1 Async & Concurrency Hardening

**Goal:** Prevent long-running sync handlers from blocking the poll cycle; expand Python handler API with streaming patterns, middleware, timeouts, and async fan-in/out.

**Target features:**
- Work-stealing thread pool for blocking sync handlers (non-blocking poll)
- Streaming handler patterns (@stream_handler for persistent WebSocket/SSE)
- Handler middleware/chain (logging, metrics, retries)
- Async handler timeout (abort slow handlers)
- Async fan-in/fan-out (multiple async sources in one handler)

## Context

**v1.0 shipped:** Rust consumer engine with bounded queues, backpressure, retry/DLQ, Prometheus metrics, W3C tracing.
**Problem identified:** Long-running sync handlers block the Tokio poll cycle, causing heartbeat misses and potential rebalances.
**Solution:** Work-stealing thread pool (Rayon) for blocking work + richer async handler patterns for non-blocking workloads.
**Tech stack:** Rust runtime + Python business logic, PyO3 bindings, rdkafka for Kafka ingestion.

## Constraints

- **Tech Stack**: Rust runtime + Python business logic, PyO3 for bindings, rdkafka for Kafka ingestion
- **Execution Model**: Real concurrency via Tokio, but Python execution respects GIL (controlled boundary, not unlimited parallelism)
- **Memory Model**: Bounded queues, explicit backpressure — predictable memory under pressure
- **Delivery**: At-least-once first, not exactly-once

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Rust owns runtime, Python owns business logic | Performance vs productivity | ✓ Validated |
| Bounded channels for all queues | Prevent memory blowup under backpressure | ✓ Validated |
| Python handlers via PyO3, not ctypes | Type safety, async support, ergonomics | ✓ Validated |
| @handler decorator as primary API | Familiar, Pythonic, explicit routing | ✓ Validated |
| Arc<Semaphore> per handler key for concurrency | Tokio-compatible, GIL-safe concurrency limiting | ✓ Validated |
| W3C traceparent header parsing | Standard trace context propagation | ✓ Validated |
| Builder pattern for ConsumerConfig/ProducerConfig | Replace 24/17 arg constructors with fluent API | ✓ Validated |
| Structured error fields (not stringly-typed) | Actionable error context at runtime | ✓ Validated |

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

## Current State

**v1.0 MVP shipped.** The framework delivers all 45 v1 requirements:
- Rust consumer engine with rdkafka (consumer groups, topic/regex subscription, manual/auto offset commit, graceful start/stop)
- Per-handler bounded queues with backpressure (Drop/Wait/PausePartition)
- Partition-aware offset tracking with contiguous commit semantics
- Rebalance-safe partition handling via CustomConsumerContext
- Failure classification (Retryable/Terminal/NonRetryable) with capped exponential backoff + jitter
- DLQ routing with metadata envelope (topic/partition/offset/reason/attempt_count)
- @handler decorator with sync/async support, per-handler concurrency, batch mode
- W3C trace context propagation (trace_id/span_id from traceparent headers)
- Prometheus metrics (throughput, latency, consumer lag, queue depth, DLQ volume)
- Context manager support (with Consumer(config) as c:)
- ConsumerConfigBuilder and ProducerConfigBuilder fluent APIs
- Structured error variants with actionable context fields

---
*Last updated: 2026-04-29 after v1.1 milestone started*