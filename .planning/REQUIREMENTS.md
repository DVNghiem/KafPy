# Requirements — Milestone v1.6

**Goal:** Improve throughput by supporting batch and async Python handlers while preserving delivery guarantees. Rust owns queue management, batching timers, worker lifecycle, backpressure, and commit orchestration. Python handlers are sync callables or async coroutines.

---

## v1.6 Requirements

### Execution Modes

- [ ] **EXEC-01**: `HandlerMode` enum — `SingleSync`, `SingleAsync`, `BatchSync`, `BatchAsync`. Gating abstraction for all handler dispatch. Detected at registration time, stored on handler.

- [ ] **EXEC-02**: `BatchPolicy` struct — `max_batch_size: usize`, `max_batch_wait_ms: u64`. Optional per-handler configuration. Default: no batching (single-message mode).

- [ ] **EXEC-03**: Sync Python handler support — existing `spawn_blocking` path unchanged for `SingleSync`. New `BatchSync` handler receives `Vec<OwnedMessage>` and returns `BatchResult`.

### Batch Accumulation

- [ ] **EXEC-04**: `BatchAccumulator` struct — accumulates messages per handler until `max_batch_size` OR `max_batch_wait_ms` timeout. Respects per-partition ordering (messages from same partition accumulate together, batches formed per-partition then combined). Fixed-window timeout (not sliding).

- [ ] **EXEC-05**: Batch flush on size — when accumulator reaches `max_batch_size`, flush to handler immediately.

- [ ] **EXEC-06**: Batch flush on timeout — when `max_batch_wait_ms` expires for oldest message in accumulator, flush entire accumulator to handler. Uses `tokio::select!` with `CancellationToken` for shutdown responsiveness.

### Async Python Handlers

- [ ] **EXEC-07**: `pyo3-async-runtimes` `into_future` bridge — Python coroutine → Rust `Future` that releases GIL during await. No `spawn_blocking` for async handlers. GIL held transiently only during future polling.

- [ ] **EXEC-08**: Coroutine detection at registration — use `inspect.iscoroutinefunction()` or `__code__.co_flags & CO_COROUTINE` to detect async handlers at Python registration time. Store `HandlerMode::SingleAsync` or `HandlerMode::BatchAsync` accordingly.

### Batch Result Modeling

- [ ] **EXEC-09**: `BatchExecutionResult` enum — `AllSuccess(Vec<Offset>)`, `AllFailure(FailureReason)`, `PartialFailure(Vec<Offset>, Vec<(OwnedMessage, FailureReason)>)`. Only `AllSuccess` and `AllFailure` in v1; `PartialFailure` is an extension point documented but not implemented.

- [ ] **EXEC-10**: Batch success → each message calls `offset_coordinator.record_ack()` individually (same as single-message). Batch failure → all messages in batch flow to retry/DLQ via existing `RetryCoordinator`.

### Integration

- [ ] **EXEC-11**: `WorkerPool` dispatches by `HandlerMode` — `SingleSync` uses existing `spawn_blocking`, `SingleAsync` uses `into_future`, `BatchSync` uses `spawn_blocking` with `Vec<OwnedMessage>`, `BatchAsync` uses `into_future` with `Vec<OwnedMessage>`.

- [ ] **EXEC-12**: Batch timeout drain on shutdown — `CancellationToken` triggers accumulator flush before worker pool shutdown. No messages lost on graceful shutdown.

- [ ] **EXEC-13**: GIL minimal usage — GIL never held across Rust-side orchestration work. Async path releases GIL at every `.await`. Sync batch path acquires/releases GIL per batch, not per message.

- [ ] **EXEC-14**: Backpressure preserved — `BackpressurePolicy` applied per-batch, not per-message within batch. If backpressure triggers during batch accumulation, no new messages are pulled until backpressure clears.

- [ ] **EXEC-15**: Routing chain integration — `RoutingDecision` applied per-message before batch accumulation. Batches formed after routing decision. `RoutingDecision::Route(handler_id)` uses batch handler if configured.

---

## Out of Scope

- Per-message outcome tracking within batches (PartialFailure) — extension point for v1.7+
- Cross-partition batch aggregation — batches respect partition boundaries
- Sliding window batch timeout — fixed window only for v1.6
- Python event loop lifecycle management for async handlers beyond basic `into_future`
- Batch handlers as streaming generators (yielding partial results) — extension point

---

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| EXEC-01 | Phase 24 | Pending |
| EXEC-02 | Phase 24 | Pending |
| EXEC-03 | Phase 24 | Pending |
| EXEC-04 | Phase 25 | Pending |
| EXEC-05 | Phase 25 | Pending |
| EXEC-06 | Phase 25 | Pending |
| EXEC-07 | Phase 26 | Pending |
| EXEC-08 | Phase 26 | Pending |
| EXEC-09 | Phase 25 | Pending |
| EXEC-10 | Phase 25 | Pending |
| EXEC-11 | Phase 24 | Pending |
| EXEC-12 | Phase 27 | Pending |
| EXEC-13 | Phase 26 | Pending |
| EXEC-14 | Phase 25 | Pending |
| EXEC-15 | Phase 24 | Pending |

---

*Last updated: 2026-04-18*
