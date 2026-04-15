# Roadmap: KafPy — v1.1 Dispatcher Layer

## Milestones

- **v1.0 Core Consumer Refactor** — Phases 1-5 (shipped 2026-04-15)
- **v1.1 Dispatcher Layer** — Phases 6-8 (in progress)

## Phases

- [ ] **Phase 6: Dispatcher Core** — Dispatcher struct, handler registration, bounded channels, send(), DispatchError enum
- [ ] **Phase 7: Backpressure + Queue Manager** — Queue depth/inflight tracking, backpressure policy trait, queue manager metadata
- [ ] **Phase 8: ConsumerRunner Integration** — Dispatcher integrated with consumer stream, Python boundary preserved, optional semaphore

## Phase Details

### Phase 6: Dispatcher Core
**Goal**: Dispatcher receives OwnedMessage and routes to per-handler bounded queues
**Depends on**: Phase 5 (v1.0 consumer core)
**Requirements**: DISP-01, DISP-02, DISP-03, DISP-04, DISP-05, DISP-19, DISP-20
**Success Criteria** (what must be TRUE):
  1. Dispatcher struct exists and accepts OwnedMessage from consumer layer
  2. Handler registration API allows registering handler slots by topic name with configurable queue capacity
  3. Each handler gets its own bounded Tokio mpsc channel (no unbounded in hot path)
  4. send() to handler queue returns Result<DispatchOutcome, DispatchError>
  5. DispatchError enum variants: QueueFull, UnknownTopic, HandlerNotRegistered, QueueClosed — all with thiserror Display/Debug
**Plans**: TBD

### Phase 7: Backpressure + Queue Manager
**Goal**: Backpressure tracking and policy hooks with queue manager metadata
**Depends on**: Phase 6
**Requirements**: DISP-06, DISP-07, DISP-08, DISP-09, DISP-10, DISP-11, DISP-12, DISP-13, DISP-14
**Success Criteria** (what must be TRUE):
  1. QueueManager owns all handler queues and metadata
  2. Queue depth is trackable per handler via inspection API
  3. Inflight count is trackable per handler via inspection API
  4. When queue is full, send() returns DispatchError::Backpressure (non-blocking)
  5. BackpressurePolicy trait exists with on_queue_full(topic, handler) hook returning BackpressureAction (Drop, Wait, FuturePausePartition)
**Plans**: TBD

### Phase 8: ConsumerRunner Integration
**Goal**: Dispatcher integrated with ConsumerRunner; Python boundary preserved
**Depends on**: Phase 7
**Requirements**: DISP-15, DISP-16, DISP-17, DISP-18
**Success Criteria** (what must be TRUE):
  1. Dispatcher receives messages from ConsumerRunner consumer stream
  2. Dispatcher API uses only owned types (no borrowed lifetimes) — Python integration boundary clean
  3. Architecture supports future pause_resume(topic) via rdkafka partition pause/resume
  4. Optional Tokio Semaphore per handler for concurrency limits (if implemented, must not overcomplicate)
**Plans**: TBD

## Progress

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 6. Dispatcher Core | 0/N | Not started | - |
| 7. Backpressure + Queue Manager | 0/N | Not started | - |
| 8. ConsumerRunner Integration | 0/N | Not started | - |

## Coverage

| Requirement | Phase | Status |
|-------------|-------|--------|
| DISP-01 | Phase 6 | Pending |
| DISP-02 | Phase 6 | Pending |
| DISP-03 | Phase 6 | Pending |
| DISP-04 | Phase 6 | Pending |
| DISP-05 | Phase 6 | Pending |
| DISP-06 | Phase 7 | Pending |
| DISP-07 | Phase 7 | Pending |
| DISP-08 | Phase 7 | Pending |
| DISP-09 | Phase 7 | Pending |
| DISP-10 | Phase 7 | Pending |
| DISP-11 | Phase 7 | Pending |
| DISP-12 | Phase 7 | Pending |
| DISP-13 | Phase 7 | Pending |
| DISP-14 | Phase 7 | Pending |
| DISP-15 | Phase 8 | Pending |
| DISP-16 | Phase 8 | Pending |
| DISP-17 | Phase 8 | Pending |
| DISP-18 | Phase 8 | Pending |
| DISP-19 | Phase 6 | Pending |
| DISP-20 | Phase 6 | Pending |

All 20 v1.1 requirements mapped.

---

*Last updated: 2026-04-15*
