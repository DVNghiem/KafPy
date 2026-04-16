# Project: KafPy

**Type:** PyO3 Native Extension — Kafka producer/consumer framework
**Core Value:** High-performance Rust Kafka client with idiomatic Python API
**Tech Stack:** Rust + PyO3 + rdkafka + Tokio + Python

---

## What This Is

KafPy is a Python-facing Kafka framework where Rust provides the runtime/core engine and Python holds the business logic. PyO3 bridges the two. The pure-Rust consumer core handles Kafka protocol, while Python registers handlers/callbacks via bindings.

Current status (after Milestone v1.1):
- `src/consumer/` — pure-Rust consumer core: `ConsumerConfigBuilder`, `OwnedMessage`, `ConsumerRunner`, `ConsumerStream`, `ConsumerTask`
- `src/dispatcher/` — message dispatcher with per-topic bounded queues: `Dispatcher`, `QueueManager`, `BackpressurePolicy`, `BackpressureAction`, `ConsumerDispatcher`
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
| ConsumerDispatcher composition | Owns both ConsumerRunner + Dispatcher, wires stream→dispatch | Active |

## Context

**Last milestone (v1.0):** Refactored duplicate consumer/message code into `src/consumer/` with clean separation between pure-Rust core and PyO3 bridge.

**Current milestone (v1.1):** Built dispatcher layer — `Dispatcher` routes `OwnedMessage` to per-topic bounded Tokio mpsc channels, with `QueueManager` tracking queue depth/inflight, `BackpressurePolicy` trait for extensible backpressure, and `ConsumerDispatcher` integrating with `ConsumerRunner`. All 20 DISP requirements shipped.

## Validated Requirements

- ✓ Per-topic bounded Tokio mpsc channel dispatch — v1.1
- ✓ Non-blocking send() with DispatchOutcome/DispatchError — v1.1
- ✓ Queue depth and inflight tracking per handler — v1.1
- ✓ BackpressurePolicy trait with BackpressureAction (Drop/Wait/FuturePausePartition) — v1.1
- ✓ ConsumerDispatcher integrating ConsumerRunner + Dispatcher — v1.1
- ✓ Tokio Semaphore per handler concurrency limiting — v1.1

## Active Requirements

(None yet — define next milestone)

## Out of Scope

- Python handler execution — deferred to future milestone
- Schema registry / Avro support
- Java/Node.js bindings — Python only
- Borrowed lifetimes in dispatcher APIs — all owned message flow

---

*Last updated: 2026-04-16 after v1.1 milestone*
