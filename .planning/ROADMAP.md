# Roadmap: KafPy

## Milestones

- [x] **v1.0 Core Consumer Refactor** — Phases 1-5 (shipped 2026-04-15)
- [x] **v1.1 Dispatcher Layer** — Phases 6-8 (shipped 2026-04-16)
- [x] **v1.2 Python Execution Lane** — Phases 9-10 (shipped 2026-04-16)
- [x] **v1.3 Offset Commit Coordinator** — Phases 11-16 (shipped 2026-04-17)
- [x] **v1.4 Failure Handling & DLQ** — Phases 17-20 (shipped 2026-04-17)
- [x] **v1.5 Extensible Routing** — Phases 21-23 (shipped 2026-04-18)
- [x] **v1.6 Execution Modes** — Phases 24-27 (shipped 2026-04-18)
- [x] **v1.7 Observability Layer** — Phases 28-32 (shipped 2026-04-18)
- [ ] **v1.8 Graceful Shutdown & Rebalance Handling** — Phases 33-35 (planned)

## Phases

### v1.8: Graceful Shutdown & Rebalance Handling

- [x] **Phase 33: ShutdownCoordinator** — ShutdownPhase enum, drain timeout, shutdown order
- [ ] **Phase 34: Rebalance Handling** — PartitionOwnership, RebalanceHandler, ownership guard in record_ack
- [ ] **Phase 35: Integration & Hardening** — Wire coordinator to all tasks, BatchAccumulator drain, biased select

### Phase 33: ShutdownCoordinator
**Goal**: Create ShutdownCoordinator with ShutdownPhase enum, drain timeout, and correct shutdown order
**Depends on**: Nothing (first phase of milestone)
**Requirements**: LSC-01, LSC-02, LSC-03, LSC-04, LSC-05
**Success Criteria:**
1. ShutdownCoordinator owns ShutdownPhase enum (Running -> Draining -> Finalizing -> Done)
2. Consumer::stop() signals ShutdownCoordinator which orchestrates component shutdown
3. Shutdown order prevents deadlock: dispatcher -> workers -> commit
4. Drain timeout default 30s with force-abort fallback
5. ConsumerRunner::close() calls rd_kafka_consumer_close() on drop
**Plans:** 1 plan
- [ ] 33-01-PLAN.md — Create ShutdownCoordinator with ShutdownPhase enum, drain timeout, and correct shutdown order

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 33. ShutdownCoordinator | v1.8 | 1/1 | Complete | 2026-04-19 |
| 34. Rebalance Handling | v1.8 | 0/1 | Not started | - |
| 35. Integration & Hardening | v1.8 | 0/1 | Not started | - |

---
*Last updated: 2026-04-19*
