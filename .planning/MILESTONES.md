# Milestones

## v1.3 offset-coordinator (Shipped: 2026-04-17)

**Phases completed:** 6 phases, 6 plans, 3 tasks

**Key accomplishments:**

- One-liner:
- Executed:
- ConsumerRunner::store_offset async method via spawn_blocking, ConsumerConfig with enable_auto_offset_store (default false)
- Phase:
- Objective:
- Objective:

---

## v1.2 Python Execution Lane (Shipped: 2026-04-16)

**Phases completed:** 2 phases, 4 plans, 0 tasks

**Key accomplishments:**

- Completed:
- Completed:
- Completed:
- Completed:

---

## v1.1 — Dispatcher Layer (Shipped: 2026-04-16)

**Phases completed:** 3 phases, 6 plans

**What shipped:**

- `src/dispatcher/mod.rs` — `Dispatcher` struct with `register_handler()`, `send()`, `send_with_policy()`
- `src/dispatcher/error.rs` — `DispatchError` enum (QueueFull, UnknownTopic, HandlerNotRegistered, QueueClosed)
- `src/dispatcher/queue_manager.rs` — `QueueManager` with AtomicUsize counters for queue_depth/inflight
- `src/dispatcher/backpressure.rs` — `BackpressurePolicy` trait + `BackpressureAction` (Drop, Wait, FuturePausePartition)
- `ConsumerDispatcher` — composition of ConsumerRunner + Dispatcher with rdkafka pause/resume wiring

**Key accomplishments:**

- Dispatcher routes OwnedMessage to per-topic bounded Tokio mpsc channels (DISP-01)
- Non-blocking send() returns Result<DispatchOutcome, DispatchError> (DISP-05)
- QueueManager tracks queue depth and inflight per handler via AtomicUsize (DISP-06, DISP-07)
- BackpressurePolicy trait enables extensible backpressure handling (DISP-09, DISP-10)
- ConsumerDispatcher integrates ConsumerRunner with Dispatcher, wires pause/resume (DISP-16, DISP-17, DISP-18)
- Tokio Semaphore per handler for concurrency limiting (DISP-15)

---

## v1.0 — Core Consumer Refactor

**Completed:** 2026-04-15

**Goal:** Consolidate duplicate consumer/message code into `src/consumer/` with clean Python-integration boundary.

**What shipped:**

- `src/consumer/` module: `ConsumerConfigBuilder`, `OwnedMessage`, `ConsumerRunner`, `ConsumerStream`, `ConsumerTask`
- Deleted duplicate `src/consume.rs` and `src/message_processor.rs`
- Updated `src/pyconsumer.rs` as PyO3 bridge to pure-Rust consumer
- Fixed all PyO3 compilation errors (GIL API, `c""` literals, `StreamExt` imports)

---

*Last updated: 2026-04-16*
