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
- [x] **v1.8 Public API Foundation** — Phases 33-37 (shipped 2026-04-20)
- [x] **v2.0 Code Quality Refactor** — Phases 01-06 (shipped 2026-04-20)

## Completed Milestones

<details>
<summary>✅ v1.0 Core Consumer Refactor (Phases 1-5) — SHIPPED 2026-04-15</summary>

- [x] Phase 1: Foundation (5/5 plans) — completed 2026-04-15
- [x] Phase 2: Dispatcher Layer (3/3 plans) — completed 2026-04-16
- [x] Phase 3: Python Execution Lane (2/2 plans) — completed 2026-04-16
- [x] Phase 4: Offset Commit Coordinator (6/6 plans) — completed 2026-04-17
- [x] Phase 5: Failure Handling & DLQ (4/4 plans) — completed 2026-04-17

</details>

<details>
<summary>✅ v1.1 Dispatcher Layer (Phases 6-8) — SHIPPED 2026-04-16</summary>

- [x] Phase 6: Dispatcher Layer — completed 2026-04-16
- [x] Phase 7: Queue Manager — completed 2026-04-16
- [x] Phase 8: Backpressure Policy — completed 2026-04-16

</details>

<details>
<summary>✅ v1.2 Python Execution Lane (Phases 9-10) — SHIPPED 2026-04-16</summary>

- [x] Phase 9: Python Handler — completed 2026-04-16
- [x] Phase 10: Worker Pool — completed 2026-04-16

</details>

<details>
<summary>✅ v1.3 Offset Commit Coordinator (Phases 11-16) — SHIPPED 2026-04-17</summary>

- [x] Phase 11: Offset Tracker — completed 2026-04-17
- [x] Phase 12: Offset Committer — completed 2026-04-17
- [x] Phase 13: Coordinator Integration — completed 2026-04-17
- [x] Phase 14: Out-of-Order Handling — completed 2026-04-17
- [x] Phase 15: Store Offset Coordination — completed 2026-04-17
- [x] Phase 16: Commit Verification — completed 2026-04-17

</details>

<details>
<summary>✅ v1.4 Failure Handling & DLQ (Phases 17-20) — SHIPPED 2026-04-17</summary>

- [x] Phase 17: Failure Classification — completed 2026-04-17
- [x] Phase 18: Retry Policy — completed 2026-04-17
- [x] Phase 19: DLQ Routing — completed 2026-04-17
- [x] Phase 20: Terminal State Gating — completed 2026-04-17

</details>

<details>
<summary>✅ v1.5 Extensible Routing (Phases 21-23) — SHIPPED 2026-04-18</summary>

- [x] Phase 21: Routing Core (3/3 plans) — completed 2026-04-17
- [x] Phase 22: Python Router (1/1 plan) — completed 2026-04-18
- [x] Phase 23: Dispatcher Integration (1/1 plan) — completed 2026-04-18

</details>

<details>
<summary>✅ v1.6 Execution Modes (Phases 24-27) — SHIPPED 2026-04-18</summary>

- [x] Phase 24: HandlerMode & Execution Foundation (1/1 plan) — completed 2026-04-18
- [x] Phase 25: Batch Accumulation & Flush (3/3 plans) — completed 2026-04-18
- [x] Phase 26: Async Python Handlers (2/2 plans) — completed 2026-04-18
- [x] Phase 27: Shutdown Drain & Polish (1/1 plan) — completed 2026-04-18

</details>

<details>
<summary>✅ v1.7 Observability Layer (Phases 28-32) — SHIPPED 2026-04-18</summary>

- [x] Phase 28: Metrics Infrastructure (1/1 plan) — completed 2026-04-18
- [x] Phase 29: Tracing Infrastructure (1/1 plan) — completed 2026-04-18
- [x] Phase 30: Kafka-Level Metrics (1/1 plan) — completed 2026-04-18
- [x] Phase 31: Runtime Introspection (1/1 plan) — completed 2026-04-18
- [x] Phase 32: Structured Logging (1/1 plan) — completed 2026-04-18

</details>

<details>
<summary>✅ v1.8 Public API Foundation (Phases 33-37) — SHIPPED 2026-04-20</summary>

- [x] Phase 33: Public API Conventions (2/2 plans) — completed 2026-04-20
- [x] Phase 34: Configuration Model (1/1 plan) — completed 2026-04-20
- [x] Phase 35: Handler Registration & Runtime (1/1 plan) — completed 2026-04-20
- [x] Phase 36: Error Handling (1/1 plan) — completed 2026-04-20
- [x] Phase 37: Documentation & Packaging (1/1 plan) — completed 2026-04-20

</details>

<details>
<summary>✅ v2.0 Code Quality Refactor (Phases 01-06) — SHIPPED 2026-04-20</summary>

- [x] Phase 01: Extract Duplicated Logic (1/1 plan) — completed 2026-04-20
- [x] Phase 02: Split worker_pool/ God Module (2/2 plans) — completed 2026-04-20
- [x] Phase 03: Split dispatcher/ and Extract runtime/ (1/1 plan) — completed 2026-04-20
- [x] Phase 04: State Machine Extraction (2/2 plans) — completed 2026-04-20
- [x] Phase 05: Split coordinator/ Module (1/1 plan) — completed 2026-04-20
- [x] Phase 06: Type Safety (1/1 plan) — completed 2026-04-20

</details>

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
| 24. HandlerMode & Execution Foundation | v1.6 | 1/1 | Complete | 2026-04-18 |
| 25. Batch Accumulation & Flush | v1.6 | 3/3 | Complete | 2026-04-18 |
| 26. Async Python Handlers | v1.6 | 2/2 | Complete | 2026-04-18 |
| 27. Shutdown Drain & Polish | v1.6 | 1/1 | Complete | 2026-04-18 |
| 28. Metrics Infrastructure | v1.7 | 1/1 | Complete | 2026-04-18 |
| 29. Tracing Infrastructure | v1.7 | 1/1 | Complete | 2026-04-18 |
| 30. Kafka-Level Metrics | v1.7 | 1/1 | Complete | 2026-04-18 |
| 31. Runtime Introspection | v1.7 | 1/1 | Complete | 2026-04-18 |
| 32. Structured Logging | v1.7 | 1/1 | Complete | 2026-04-18 |
| 33. Public API Conventions | v1.8 | 2/2 | Complete | 2026-04-20 |
| 34. Configuration Model | v1.8 | 1/1 | Complete | 2026-04-20 |
| 35. Handler Registration & Runtime | v1.8 | 1/1 | Complete | 2026-04-20 |
| 36. Error Handling | v1.8 | 1/1 | Complete | 2026-04-20 |
| 37. Documentation & Packaging | v1.8 | 1/1 | Complete | 2026-04-20 |
| 01. Extract Duplicated Logic | v2.0 | 1/1 | Complete   | 2026-04-24 |
| 02. Split worker_pool/ | v2.0 | 2/2 | Complete | 2026-04-20 |
| 03. Split dispatcher/ + Extract runtime/ | v2.0 | 1/1 | Complete | 2026-04-20 |
| 04. State Machine Extraction | v2.0 | 2/2 | Complete | 2026-04-20 |
| 05. Split coordinator/ | v2.0 | 1/1 | Complete | 2026-04-20 |
| 06. Type Safety | v2.0 | 1/1 | Complete | 2026-04-20 |
| 07. Verify DUP Requirements | v2.0 | 1/1 | Gap Closure | - |
| 08. Verify SPLIT-A Requirements | v2.0 | 1/6 | Gap Closure | - |
| 09. Verify SPLIT-B Requirements | v2.0 | 1/4 | Gap Closure | - |
| 10. Verify STATE Requirements | v2.0 | 1/5 | Gap Closure | - |
| 11. Verify SPLIT-C Requirements | v2.0 | 1/5 | Gap Closure | - |
| 12. Verify TYPES Requirements | v2.0 | 1/5 | Gap Closure | - |
| 13. Verify GATES Requirements | v2.0 | 1/5 | Gap Closure | - |

### Phase 1: architecture document

**Goal:** [To be planned]
**Requirements**: TBD
**Depends on:** Phase 0
**Plans:** 0 plans

Plans:
- [ ] TBD (run /gsd-plan-phase 1 to break down)

---

*Last updated: 2026-04-20 after v2.0 milestone completion*