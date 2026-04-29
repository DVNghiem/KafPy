---
gsd_state_version: 1.0
milestone: v2.0
milestone_name: Fan-Out/Fan-In
current_phase: 11
status: milestone_started
last_updated: "2026-04-29T21:30:00.000Z"
progress:
  total_phases: 5
  completed_phases: 0
  total_plans: 14
  completed_plans: 0
  percent: 0
---

# KafPy Project State

**Project:** Rust-Core, Python-Logic Kafka Consumer Framework
**Last Updated:** 2026-04-29

---

## Current Position

**Milestone:** v2.0 Fan-Out/Fan-In (started)
**Current Phase:** 11 (Fan-Out Core)
**Plan:** —
**Status:** Planning phase started

---

## Shipped Milestones

| Milestone | Status |
|-----------|--------|
| v1.0 MVP | Shipped 2026-04-29 |
| v1.1 Async & Concurrency Hardening | Shipped 2026-04-29 |

---

## Project Reference

**Core value:** Python developers can write Kafka message handlers easily while Rust controls the hard runtime problems (concurrency, backpressure, retries, DLQ, offset tracking, graceful shutdown).

**v2.0 Focus:** Fan-out (one message to multiple sinks in parallel via JoinSet) and Fan-in (multiple sources merged into one handler round-robin).

---

## v2.0 Phase Structure

| Phase | Goal | Requirements |
|-------|------|--------------|
| 11 | Fan-Out Core | FANOUT-01, FANOUT-02, FANOUT-03 |
| 12 | Fan-Out Offset Commit | FANOUT-04, FANOUT-05 |
| 13 | Fan-Out Python API | FANOUT-06, OBSV-01, OBSV-02 |
| 14 | Fan-In Multiplexer | FANIN-01, FANIN-02, FANIN-03 |
| 15 | Fan-In Integration | FANIN-04, FANIN-05, OBSV-03 |

---

## Phase Dependencies (v2.0)

```
Phase 10 (Streaming Handler) [v1.1]
  └── Phase 11 (Fan-Out Core)
        └── Phase 12 (Fan-Out Offset Commit)
              └── Phase 13 (Fan-Out Python API)
                    └── Phase 14 (Fan-In Multiplexer)
                          └── Phase 15 (Fan-In Integration)
```

---

## Accumulated Context

### Key Decisions (v1.0 - v1.1)

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Rust owns runtime, Python owns business logic | Performance vs productivity | Validated |
| Bounded channels for all queues | Prevent memory blowup under backpressure | Validated |
| Python handlers via PyO3, not ctypes | Type safety, async support, ergonomics | Validated |
| @handler decorator as primary API | Familiar, Pythonic, explicit routing | Validated |
| Arc<Semaphore> per handler key for concurrency | Tokio-compatible, GIL-safe concurrency limiting | Validated |
| W3C traceparent header parsing | Standard trace context propagation | Validated |
| Builder pattern for ConsumerConfig/ProducerConfig | Replace 24/17 arg constructors with fluent API | Validated |
| Structured error fields (not stringly-typed) | Actionable error context at runtime | Validated |
| Rayon work-stealing pool for sync handlers | Prevent poll cycle blocking, heartbeat/rebalance risk | Validated v1.1 |
| Streaming handler with four-phase state machine | Lifecycle: start/subscribe, run/loop, stop/drain, error recovery | Validated v1.1 |
| Fan-Out partial success (sinks fail independently) | Preserve at-least-once, primary ACKed immediately | v2.0 |
| Fan-In round-robin (no ordering guarantee) | Simplicity, no head-of-line blocking | v2.0 |

### v2.0 Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Fan-Out before Fan-In | Fan-out is independent; fan-in depends on AsyncMessageStream from v1.1 | v2.0 |
| FanOutTracker gates offset commit | FOUT-03: at-least-once requires all branches complete before ACK | v2.0 |
| Bounded fan-out degree (max 64) | Prevent resource exhaustion from misconfigured rules | v2.0 |
| GIL serialization documented for Python handlers | PyO3 GIL means fan-out to Python is serialized | v2.0 |

---

## Technical Debt

- RayonPool::drain() and abort() are no-ops (Rayon limitation, documented)
- streaming_worker_loop State::Draining path may need production edge case testing
- PausePartition/ResumePartition backpressure untested with actual Kafka consumer

---

## Coverage

| Requirement | Phase | Status |
|-------------|-------|--------|
| FANOUT-01 | Phase 11 | Pending |
| FANOUT-02 | Phase 11 | Pending |
| FANOUT-03 | Phase 11 | Pending |
| FANOUT-04 | Phase 12 | Pending |
| FANOUT-05 | Phase 12 | Pending |
| FANOUT-06 | Phase 13 | Pending |
| FANIN-01 | Phase 14 | Pending |
| FANIN-02 | Phase 14 | Pending |
| FANIN-03 | Phase 14 | Pending |
| FANIN-04 | Phase 15 | Pending |
| FANIN-05 | Phase 15 | Pending |
| OBSV-01 | Phase 13 | Pending |
| OBSV-02 | Phase 13 | Pending |
| OBSV-03 | Phase 15 | Pending |

**v2.0 Coverage:** 14/14 requirements mapped to 5 phases

---
*Last updated: 2026-04-29 after v2.0 roadmap created*
