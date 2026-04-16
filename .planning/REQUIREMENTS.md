# Requirements — Milestone v1.2

**Goal:** Build the Python execution layer that pulls messages from handler queues and invokes registered Python callbacks through PyO3-compatible boundaries.

---

## v1.2 Requirements

### Execution Foundation

- [ ] **EXEC-01**: `ExecutionResult` enum: `Ok`, `Error { exception, traceback }`, `Rejected { reason }` — normalized Rust result type
- [ ] **EXEC-02**: `ExecutionContext` — message metadata (topic, partition, offset, worker_id, timestamp)
- [ ] **EXEC-03**: `Executor` trait: `execute(ctx, message, handler) -> ExecutionResult` — pluggable execution policy
- [ ] **EXEC-04**: `ExecutorOutcome` enum: `Ack`, `Retry`, `Rejected` — outcome of execution policy decision
- [ ] **EXEC-05**: `DefaultExecutor` — fire-and-forget; logs outcome and returns `Ack`; future retry/async/batch plug in via trait
- [ ] **EXEC-06**: `PythonHandler` — wraps `Py<PyAny>` callback; `invoke(message) -> ExecutionResult` via `spawn_blocking`
- [ ] **EXEC-07**: GIL acquired via `spawn_blocking` — Tokio threads released during Python call; GIL held only inside closure

### Worker Pool

- [ ] **EXEC-08**: `WorkerPool` — configurable N workers managed via Tokio `JoinSet`
- [ ] **EXEC-09**: Each worker polls its assigned `mpsc::Receiver<OwnedMessage>` independently
- [ ] **EXEC-10**: `spawn_blocking` per-message — Tokio threads released during Python call
- [ ] **EXEC-11**: Structured logging: worker start/stop, message pickup, handler success/failure (via `tracing`)
- [ ] **EXEC-12**: Graceful shutdown: complete in-flight messages before worker exit; `CancellationToken` propagated to workers
- [ ] **EXEC-13**: `QueueManager::ack()` called on `ExecutionResult::Ok` — closes inflight counter

### PyO3 Integration

- [ ] **EXEC-14**: `PyHandler` type in `src/python/` wrapping `PythonHandler`; `add_handler(topic, callback)` in PyO3 bridge
- [ ] **EXEC-15**: `Py<PyAny>` storage — GIL-independent, Send+Sync; NOT `&PyAny` or `Bound<'_, PyAny>`
- [ ] **EXEC-16**: `callback.unbind()` at PyO3 boundary — converts to owned `Py<PyAny>` before storing
- [ ] **EXEC-17**: Python callable receives `KafkaMessage` — `topic: str`, `partition: i32`, `offset: i64`, `key: Option<bytes>`, `payload: Option<bytes>`, `timestamp: int`

### Extensibility Interfaces (Placeholders)

- [ ] **EXEC-18**: `RetryExecutor` — trait placeholder for future retry policy (backoff, max attempts); not implemented
- [ ] **EXEC-19**: `OffsetAck` — interface placeholder for future offset tracking; not implemented
- [ ] **EXEC-20**: Async Python handler — trait placeholder for `async def` callbacks via `pyo3-async-runtimes`; not implemented

---

## Out of Scope

- Full retry manager / exponential backoff — deferred to v1.x
- Offset manager / commit batching — deferred to v1.x
- DLQ (Dead Letter Queue) — deferred to v2
- `AsyncExecutor` implementation — interface only (EXEC-20)
- Schema registry / Avro support
- Java/Node.js bindings — Python only

---

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| EXEC-01 | Phase 9 | — |
| EXEC-02 | Phase 9 | — |
| EXEC-03 | Phase 9 | — |
| EXEC-04 | Phase 9 | — |
| EXEC-05 | Phase 9 | — |
| EXEC-06 | Phase 9 | — |
| EXEC-07 | Phase 9 | — |
| EXEC-08 | Phase 10 | — |
| EXEC-09 | Phase 10 | — |
| EXEC-10 | Phase 10 | — |
| EXEC-11 | Phase 10 | — |
| EXEC-12 | Phase 10 | — |
| EXEC-13 | Phase 10 | — |
| EXEC-14 | Phase 9 | — |
| EXEC-15 | Phase 9 | — |
| EXEC-16 | Phase 9 | — |
| EXEC-17 | Phase 9 | — |
| EXEC-18 | Phase 9 | — |
| EXEC-19 | Phase 9 | — |
| EXEC-20 | Phase 9 | — |

---

*Last updated: 2026-04-16*
