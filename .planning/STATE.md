---
gsd_state_version: 1.0
milestone: v2.0
milestone_name: Fan-Out/Fan-In
current_phase: 11
status: milestone_started
last_updated: "2026-04-29T21:20:00.000Z"
progress:
  total_phases: 0
  completed_phases: 0
  total_plans: 0
  completed_plans: 0
  percent: 0
---

# KafPy Project State

**Project:** Rust-Core, Python-Logic Kafka Consumer Framework
**Last Updated:** 2026-04-29

---

## Current Position

**Milestone:** v2.0 Fan-Out/Fan-In (started)
**Current Phase:** 11 (defining requirements)
**Plan:** —
**Status:** Defining requirements

---

## Shipped Milestones

| Milestone | Status |
|-----------|--------|
| v1.0 MVP | ✅ Shipped 2026-04-29 |
| v1.1 Async & Concurrency Hardening | ✅ Shipped 2026-04-29 |

---

## Project Reference

**Core value:** Python developers can write Kafka message handlers easily while Rust controls the hard runtime problems (concurrency, backpressure, retries, DLQ, offset tracking, graceful shutdown).

**v2.0 Focus:** Fan-out (one message → multiple sinks in parallel) and Fan-in (multiple sources → one handler round-robin).

---

## Phase Dependencies (v2.0)

```
Phase 11 (Fan-Out)
  └── Phase 12 (Fan-In)
```

---

## Accumulated Context

### Key Decisions (v1.0 - v1.1)

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Rust owns runtime, Python owns business logic | Performance vs productivity | ✓ Validated |
| Bounded channels for all queues | Prevent memory blowup under backpressure | ✓ Validated |
| Python handlers via PyO3, not ctypes | Type safety, async support, ergonomics | ✓ Validated |
| @handler decorator as primary API | Familiar, Pythonic, explicit routing | ✓ Validated |
| Arc<Semaphore> per handler key for concurrency | Tokio-compatible, GIL-safe concurrency limiting | ✓ Validated |
| W3C traceparent header parsing | Standard trace context propagation | ✓ Validated |
| Builder pattern for ConsumerConfig/ProducerConfig | Replace 24/17 arg constructors with fluent API | ✓ Validated |
| Structured error fields (not stringly-typed) | Actionable error context at runtime | ✓ Validated |
| Rayon work-stealing pool for sync handlers | Prevent poll cycle blocking, heartbeat/rebalance risk | ✓ Validated v1.1 |
| Streaming handler with four-phase state machine | Lifecycle: start/subscribe, run/loop, stop/drain, error recovery | ✓ Validated v1.1 |
| Fan-Out partial success (sinks fail independently) | Preserve at-least-once, primary ACKed immediately | v2.0 |
| Fan-In round-robin (no ordering guarantee) | Simplicity, no head-of-line blocking | v2.0 |

---

## Technical Debt

- RayonPool::drain() and abort() are no-ops (Rayon limitation, documented)
- streaming_worker_loop State::Draining path may need production edge case testing
- PausePartition/ResumePartition backpressure untested with actual Kafka consumer

---
*Last updated: 2026-04-29 after v2.0 milestone started*