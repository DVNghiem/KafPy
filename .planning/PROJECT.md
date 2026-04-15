# Project: KafPy

**Type:** PyO3 Native Extension — Kafka producer/consumer framework
**Core Value:** High-performance Rust Kafka client with idiomatic Python API
**Tech Stack:** Rust + PyO3 + rdkafka + Tokio + Python

---

## What This Is

KafPy is a Python-facing Kafka framework where Rust provides the runtime/core engine and Python holds the business logic. PyO3 bridges the two. The pure-Rust consumer core handles Kafka protocol, while Python registers handlers/callbacks via bindings.

Current status (after Milestone 1 refactor):
- `src/consumer/` — pure-Rust consumer core (no PyO3 deps): `ConsumerConfigBuilder`, `OwnedMessage`, `ConsumerRunner`, `ConsumerStream`, `ConsumerTask`
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

## Context

**Last milestone (v1.0):** Refactored duplicate consumer/message code into `src/consumer/` with clean separation between pure-Rust core and PyO3 bridge.

## Current Milestone: v1.1 Dispatcher Layer

**Goal:** Build a dispatcher layer in Rust that receives owned messages from the consumer layer and routes them into per-handler bounded queues.

**Target features:**
- Topic-based routing only (no Python execution yet)
- Bounded Tokio `mpsc` queue per handler
- Backpressure via bounded queues only (no unbounded channels in hot path)
- Queue depth and inflight count tracking
- Backpressure strategy hook for future partition pause/resume with rdkafka
- Tokio Semaphore for per-handler concurrency limits (optional, don't overcomplicate)

## Active Requirements

(None yet — ship to validate)

## Out of Scope

- Python handler execution — deferred to future milestone
- Schema registry / Avro support
- Java/Node.js bindings — Python only
- Borrowed lifetimes in dispatcher APIs — all owned message flow

---

*Last updated: 2026-04-15 after Milestone 1 refactor*