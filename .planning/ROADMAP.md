# Roadmap: KafPy — v1.6 Execution Modes

## Milestones

- [x] **v1.0 Core Consumer Refactor** — Phases 1-5 (shipped 2026-04-15)
- [x] **v1.1 Dispatcher Layer** — Phases 6-8 (shipped 2026-04-16)
- [x] **v1.2 Python Execution Lane** — Phases 9-10 (shipped 2026-04-16)
- [x] **v1.3 Offset Commit Coordinator** — Phases 11-16 (shipped 2026-04-17)
- [x] **v1.4 Failure Handling & DLQ** — Phases 17-20 (shipped 2026-04-17)
- [x] **v1.5 Extensible Routing** — Phases 21-23 (shipped 2026-04-18)
- [ ] **v1.6 Execution Modes** — Phases 24-27

## Phases

- [ ] **Phase 24:** HandlerMode & Execution Foundation — HandlerMode enum, BatchPolicy, WorkerPool dispatch, routing integration
- [ ] **Phase 25:** Batch Accumulation & Flush — BatchAccumulator, flush on size/timeout, backpressure per-batch, result modeling
- [ ] **Phase 26:** Async Python Handlers — pyo3-async-runtimes into_future bridge, coroutine detection
- [ ] **Phase 27:** Shutdown Drain & Polish — batch drain on shutdown, GIL verification, async batch path

---

## Phase Details

### Phase 24: HandlerMode & Execution Foundation

**Goal:** HandlerMode enum gates all handler dispatch; WorkerPool dispatches by mode; batch handlers integrate with routing chain

**Depends on:** Phase 23

**Requirements:** EXEC-01, EXEC-02, EXEC-03, EXEC-11, EXEC-15

**Success Criteria** (what must be TRUE):
1. HandlerMode enum (SingleSync, SingleAsync, BatchSync, BatchAsync) exists and is stored on each PythonHandler
2. BatchPolicy struct (max_batch_size, max_batch_wait_ms) is configurable per-handler with sensible defaults
3. SingleSync handlers use existing spawn_blocking path unchanged
4. WorkerPool::worker_loop dispatches to appropriate execution path based on HandlerMode
5. RoutingDecision::Route(handler_id) queues messages to batch-capable handler if batch mode configured on that handler

**Plans:** TBD

### Phase 25: Batch Accumulation & Flush

**Goal:** Messages accumulate per handler until max_batch_size OR max_batch_wait_ms; batches flush atomically with result handling

**Depends on:** Phase 24

**Requirements:** EXEC-04, EXEC-05, EXEC-06, EXEC-09, EXEC-10, EXEC-14

**Success Criteria** (what must be TRUE):
1. BatchAccumulator accumulates messages per handler, respecting per-partition ordering (batches formed per-partition then combined)
2. Accumulator flushes immediately when max_batch_size is reached
3. Accumulator flushes when max_batch_wait_ms expires for oldest message in accumulator
4. tokio::select! races message arrival against timeout, with CancellationToken for shutdown responsiveness
5. BackpressurePolicy applied per-batch during accumulation (no new messages pulled while backpressure active)
6. BatchExecutionResult::AllSuccess triggers record_ack for each message individually
7. BatchExecutionResult::AllFailure routes all messages in batch to RetryCoordinator

**Plans:** TBD

### Phase 26: Async Python Handlers

**Goal:** Python coroutines executed as Rust Futures via pyo3-async-runtimes, releasing GIL during await

**Depends on:** Phase 25

**Requirements:** EXEC-07, EXEC-08, EXEC-13 (partial)

**Success Criteria** (what must be TRUE):
1. inspect.iscoroutinefunction() or __code__.co_flags & CO_COROUTINE used at Python registration time to detect async handlers
2. HandlerMode::SingleAsync stored for detected async handlers
3. pyo3-async-runtimes tokio::into_future converts Python coroutine to Tokio-compatible Future
4. GIL released during await in async path (not held across Rust-side orchestration)
5. Async batch handlers (BatchAsync) use into_future with Vec<OwnedMessage> batch invocation

**Plans:** TBD

### Phase 27: Shutdown Drain & Polish

**Goal:** Graceful shutdown drains accumulated batches; GIL usage verified minimal; all execution modes complete

**Depends on:** Phase 26

**Requirements:** EXEC-12, EXEC-13 (remaining), EXEC-14 (verification)

**Success Criteria** (what must be TRUE):
1. CancellationToken triggers accumulator flush before worker pool shutdown (no messages lost on graceful shutdown)
2. tokio::select! with biased cancellation branch drains accumulator before worker exits
3. GIL never held across Rust-side orchestration for any execution mode
4. All 4 HandlerMode paths (SingleSync, SingleAsync, BatchSync, BatchAsync) execute end-to-end with correct offset commit semantics

**Plans:** TBD

---

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. Foundation | v1.0 | - | Complete | 2026-04-15 |
| 6. Dispatcher Layer | v1.1 | - | Complete | 2026-04-16 |
| 9. Python Execution Lane | v1.2 | - | Complete | 2026-04-16 |
| 11. Offset Commit Coordinator | v1.3 | - | Complete | 2026-04-17 |
| 17. Failure Handling & DLQ | v1.4 | - | Complete | 2026-04-17 |
| 21. Routing Core | v1.5 | 3/3 | Complete | 2026-04-17 |
| 22. Python Integration | v1.5 | 1/1 | Complete | 2026-04-18 |
| 23. Dispatcher Integration | v1.5 | 1/1 | Complete | 2026-04-18 |
| 24. HandlerMode & Execution Foundation | v1.6 | 0/- | Not started | - |
| 25. Batch Accumulation & Flush | v1.6 | 0/- | Not started | - |
| 26. Async Python Handlers | v1.6 | 0/- | Not started | - |
| 27. Shutdown Drain & Polish | v1.6 | 0/- | Not started | - |

---

*Last updated: 2026-04-18*
