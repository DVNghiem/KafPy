# Requirements — Milestone v1.1

**Goal:** Build a dispatcher layer that receives owned messages from the consumer layer and routes them into per-handler bounded queues.

---

## v1.1 Requirements

### Dispatcher Core

- [x] **DISP-01**: Dispatcher receives `OwnedMessage` from consumer layer and routes by topic
- [x] **DISP-02**: Handler registration API — register handler slots by topic name
- [x] **DISP-03**: Per-handler bounded Tokio `mpsc` channel (configurable capacity)
- [x] **DISP-04**: Bounded queues only — no unbounded channels in hot path
- [x] **DISP-05**: `send()` to handler queue returns `Result<DispatchOutcome, DispatchError>`

### Backpressure

- [ ] **DISP-06**: Track queue depth per handler (available via inspection API)
- [ ] **DISP-07**: Track inflight count per handler
- [ ] **DISP-08**: When queue is full, `send()` returns `DispatchError::Backpressure` (not blocking)
- [ ] **DISP-09**: Backpressure strategy trait — `BackpressurePolicy::on_queue_full(topic, handler)` hook
- [ ] **DISP-10**: Strategy returns `BackpressureAction` (Drop, Wait, or FuturePausePartition)

### Queue Manager

- [ ] **DISP-11**: `QueueManager` owns all handler queues and metadata
- [ ] **DISP-12**: Queue capacity configurable per-handler at registration
- [ ] **DISP-13**: `QueueManager::get_queue_depth(topic)` returns `Option<usize>`
- [ ] **DISP-14**: `QueueManager::get_inflight(topic)` returns `Option<usize>`

### Concurrency (Optional)

- [ ] **DISP-15**: Optional Tokio `Semaphore` per handler for concurrency limit (don't overcomplicate)

### Integration

- [ ] **DISP-16**: Dispatcher integrates with `ConsumerRunner` — dispatcher receives messages from consumer stream
- [ ] **DISP-17**: Python integration boundary preserved — dispatcher API uses only owned types, no borrowed lifetimes
- [ ] **DISP-18**: Designed for future `pause_resume(topic)` method using rdkafka partition pause/resume

### Error Handling

- [x] **DISP-19**: `DispatchError` enum: `QueueFull`, `UnknownTopic`, `HandlerNotRegistered`, `QueueClosed`
- [x] **DISP-20**: All errors are `thiserror` types with `Display` and `Debug`

---

## Out of Scope

- **Python handler execution** — deferred to future milestone (DISP-02 is registration only, no callback invocation)
- **Schema registry / Avro** — not relevant to dispatcher layer
- **Borrowed lifetimes in dispatcher APIs** — all message flow uses owned types only

---

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| DISP-01 through DISP-05 | Phase 1 | — |
| DISP-06 through DISP-10 | Phase 2 | — |
| DISP-11 through DISP-14 | Phase 2 | — |
| DISP-15 | Phase 2 (optional) | — |
| DISP-16 through DISP-18 | Phase 3 | — |
| DISP-19 through DISP-20 | Phase 1 | — |

---

*Last updated: 2026-04-15*