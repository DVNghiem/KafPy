# Roadmap: KafPy — v1.2 Python Execution Lane

## Milestones

- **v1.0 Core Consumer Refactor** — Phases 1-5 (shipped 2026-04-15)
- **v1.1 Dispatcher Layer** — Phases 6-8 (shipped 2026-04-16)
- **v1.2 Python Execution Lane** — Phases 9-10 (in progress)

## Phases

### Phase 9: Execution Foundation
**Goal:** Core execution types — ExecutionResult, Executor trait, PythonHandler, PyO3 integration
**Depends on**: Phase 8 (ConsumerDispatcher)
**Requirements**: EXEC-01, EXEC-02, EXEC-03, EXEC-04, EXEC-05, EXEC-06, EXEC-07, EXEC-14, EXEC-15, EXEC-16, EXEC-17, EXEC-18, EXEC-19, EXEC-20
**Success Criteria** (what must be TRUE):
  1. ExecutionResult enum exists with Ok/Error/Rejected variants
  2. Executor trait exists with DefaultExecutor fire-and-forget impl
  3. PythonHandler stores Py<PyAny>, invoke() uses spawn_blocking
  4. GIL held only inside spawn_blocking closure
  5. PyHandler in pyconsumer.rs wraps PythonHandler
  6. ExecutorOutcome enum exists (Ack/Retry/Rejected)
  7. RetryExecutor, OffsetAck, AsyncHandler are trait placeholders
**Plans**: TBD

### Phase 10: Worker Pool
**Goal:** WorkerPool with N workers pulling from handler queues, graceful shutdown, ack integration
**Depends on**: Phase 9
**Requirements**: EXEC-08, EXEC-09, EXEC-10, EXEC-11, EXEC-12, EXEC-13
**Success Criteria** (what must be TRUE):
  1. WorkerPool spawns configurable N Tokio tasks via JoinSet
  2. Each worker polls mpsc::Receiver<OwnedMessage> independently
  3. spawn_blocking used for every Python invocation
  4. Structured logging: worker start/stop, message pickup, success/failure
  5. Graceful shutdown completes in-flight before exit
  6. QueueManager::ack() called on ExecutionResult::Ok
**Plans**: TBD

## Progress

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 9. Execution Foundation | 0/N | Not started | - |
| 10. Worker Pool | 0/N | Not started | - |

## Coverage

| Requirement | Phase | Status |
|-------------|-------|--------|
| EXEC-01 | Phase 9 | Pending |
| EXEC-02 | Phase 9 | Pending |
| EXEC-03 | Phase 9 | Pending |
| EXEC-04 | Phase 9 | Pending |
| EXEC-05 | Phase 9 | Pending |
| EXEC-06 | Phase 9 | Pending |
| EXEC-07 | Phase 9 | Pending |
| EXEC-08 | Phase 10 | Pending |
| EXEC-09 | Phase 10 | Pending |
| EXEC-10 | Phase 10 | Pending |
| EXEC-11 | Phase 10 | Pending |
| EXEC-12 | Phase 10 | Pending |
| EXEC-13 | Phase 10 | Pending |
| EXEC-14 | Phase 9 | Pending |
| EXEC-15 | Phase 9 | Pending |
| EXEC-16 | Phase 9 | Pending |
| EXEC-17 | Phase 9 | Pending |
| EXEC-18 | Phase 9 | Pending |
| EXEC-19 | Phase 9 | Pending |
| EXEC-20 | Phase 9 | Pending |

All 20 v1.2 requirements mapped.

---

*Last updated: 2026-04-16*
