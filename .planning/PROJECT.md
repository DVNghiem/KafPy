# KafPy — Rust-Core, Python-Logic Kafka Consumer Framework

## What This Is

A high-performance Kafka consumer framework where Rust owns the runtime core (concurrency, memory, Kafka protocol) and Python owns business logic (message handlers). Rust handles execution control, concurrency, memory management, and operational safety while Python provides message handlers and developer ergonomics.

## Core Value

Python developers can write Kafka message handlers easily while Rust controls the hard runtime problems (concurrency, backpressure, retries, DLQ, offset tracking, graceful shutdown).

## Current State

**v1.1 Async & Concurrency Hardening SHIPPED.** The framework now supports:
- Rayon work-stealing thread pool for blocking sync handlers (non-blocking poll)
- Handler timeout with structured metadata and Prometheus metrics
- Middleware chain (Logging, Metrics) per handler via @handler(middleware=[...])
- Streaming handler support (@stream_handler) for persistent async iterables with backpressure

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
- ✓ Rayon work-stealing pool for blocking sync handlers (non-blocking poll) — v1.1
- ✓ Handler timeout with metadata and Prometheus counter — v1.1
- ✓ Handler middleware chain (Logging, Metrics) — v1.1
- ✓ Streaming handler support (@stream_handler) — v1.1

### Active

- [ ] Fan-Out: One message triggers multiple async handlers/sinks in parallel
- [ ] Fan-In: Multiple async sources merged into single handler (round-robin, priority)
- [ ] Async fan-out/fan-in (multiple async sources in one handler)

### Out of Scope

- Rust-only framework with thin Python bindings — This project IS the Pythonic layer
- Fake abstraction layer with unused config — Public APIs must reflect real runtime behavior
- Rule engine for everything — Business logic belongs in Python handlers
- Platform exposing more features than implemented — Minimal, honest, maintainable codebase
- Complex DSL or custom language — "Just Python" — no new DSL to learn
- Heavyweight infrastructure (ZooKeeper) — Single process, pip install and go
- Multi-language support (JS, Java, Go) — Python focus only initially
- Built-in ML/Data science libraries — Users bring their own
- Blocking middleware in async context — GIL hold during middleware blocks Tokio event loop
- Fan-out with distributed transactions — Two-phase commit across handlers is not at-least-once semantics
- Streaming handler with exactly-once — Checkpointing state complexity explodes
- Middleware that mutates messages — Hidden mutations break debugging and predictability
- Unbounded fan-out — No limit causes resource exhaustion

## Next Milestone: v2.0 Fan-Out/Fan-In

**Goal:** Enable parallel multi-sink production and multi-source consumption patterns.

**Target features:**
- Fan-Out: One message triggers multiple async handlers/sinks in parallel via JoinSet
- Fan-In: Multiple async sources merged into single handler (round-robin, priority)
- Python API for both patterns

## Context

**v1.1 shipped:** Rust consumer engine with Rayon thread pool, handler timeout, middleware chain, streaming handlers.
**Tech stack:** Rust runtime + Python business logic, PyO3 bindings, rdkafka for Kafka ingestion.
**v1.0 shipped:** Rust consumer engine with bounded queues, backpressure, retry/DLQ, Prometheus metrics, W3C tracing.

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
| Rayon work-stealing pool for sync handlers | Prevent poll cycle blocking, heartbeat/rebalance risk | ✓ Validated v1.1 |
| Tokio stays as async runtime | No change to existing async infrastructure | ✓ Validated v1.1 |
| Python GIL calls via spawn_blocking | GIL safety preserved | ✓ Validated v1.1 |
| Oneshot channels for Tokio-Rayon communication | Prevent deadlock (no Tokio APIs from Rayon) | ✓ Validated v1.1 |
| Streaming handler with four-phase state machine | Lifecycle: start/subscribe, run/loop, stop/drain, error recovery | ✓ Validated v1.1 |
| PausePartition/ResumePartition for backpressure | Slow consumer signal to Kafka, fast producer doesn't overflow memory | ✓ Validated v1.1 |

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
*Last updated: 2026-04-29 after v1.1 milestone shipped*