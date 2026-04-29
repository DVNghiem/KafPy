# KafPy — Rust-Core, Python-Logic Kafka Consumer Framework

## What This Is

A high-performance Kafka consumer framework that keeps the runtime core in Rust while allowing application teams to write all business logic in Python. Rust owns execution control, concurrency, memory management, and operational safety; Python owns message handlers and developer ergonomics.

## Core Value

Python developers can write Kafka message handlers easily while Rust controls the hard runtime problems (concurrency, backpressure, retries, DLQ, offset tracking, graceful shutdown).

## Requirements

### Validated

(None yet — ship to validate)

### Active

- [ ] Rust-based Kafka consumer engine managing full message-consumption lifecycle
- [ ] Python handler registration via PyO3 boundary
- [ ] Topic-based routing with bounded internal queues
- [ ] Per-handler bounded queues with backpressure
- [ ] At-least-once delivery with partition-aware offset tracking
- [ ] Retryable vs non-retryable failure classification with backoff/jitter
- [ ] DLQ handoff after retries exhausted
- [ ] Graceful shutdown, queue draining, rebalance-safe partition handling
- [ ] Python-first developer API (@handler decorator style)
- [ ] Configurable: retry policies, batch size, flush timing, concurrency, DLQ, observability
- [ ] Observability: metrics, tracing, runtime introspection

### Out of Scope

- Rust-only framework with thin Python bindings
- Fake abstraction layer with unused config
- Rule engine for everything
- Platform exposing more features than implemented

## Context

**Existing project: KafPy** — This is a brownfield project. The codebase already has:
- Python/Rust hybrid architecture with PyO3 bindings
- Consumer module (`kafpy/consumer.py`)
- Config module (`kafpy/config.py`)
- Handlers module (`kafpy/handlers.py`)
- Runtime module (`kafpy/runtime.py`)
- Rust source in `src/` (consumer, config, logging, observability, worker pool)

The idea document describes the target architecture. The current codebase represents an early implementation that needs verification and completion against this spec.

## Constraints

- **Tech Stack**: Rust runtime + Python business logic, PyO3 for bindings, rdkafka for Kafka ingestion
- **Execution Model**: Real concurrency via Tokio, but Python execution respects GIL (controlled boundary, not unlimited parallelism)
- **Memory Model**: Bounded queues, explicit backpressure — predictable memory under pressure
- **Delivery**: At-least-once first, not exactly-once

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Rust owns runtime, Python owns business logic | Performance vs productivity | — Pending |
| Bounded channels for all queues | Prevent memory blowup under backpressure | — Pending |
| Python handlers via PyO3, not ctypes | Type safety, async support, ergonomics | — Pending |
| @handler decorator as primary API | Familiar, Pythonic, explicit routing | — Pending |

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

## Current Milestone: v1.1 Lint & Hardening

**Goal:** Resolve all clippy warnings and improve code quality for production readiness.

**Target features:**
- Zero clippy warnings (19→0: dead code, too_many_arguments, enum names, module inception, etc.)
- ConsumerConfig builder pattern (24-arg constructor → fluent builder)
- ProducerConfig builder pattern (17-arg constructor → fluent builder)
- Better ergonomics for Python API consumers

---

*Last updated: 2026-04-29 after lint & hardening*