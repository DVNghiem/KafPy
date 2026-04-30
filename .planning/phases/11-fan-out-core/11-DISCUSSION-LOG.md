# Phase 11: Fan-Out Core - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-04-29
**Phase:** 11-fan-out-core
**Areas discussed:** Sink registration model, Sink failure tracking, Max fan-out overflow behavior

---

## Sink Registration Model

| Option | Description | Selected |
|--------|-------------|----------|
| Static per handler | Each handler declares sink topics upfront at registration | ✓ |
| Dynamic per message | Python code provides sink topics at dispatch time | |
| Hybrid | Static groups + dynamic membership via register_fanout | |

**User's choice:** Static per handler
**Notes:** Simple, predictable, compile-time safe

---

## Sink Failure Tracking

| Option | Description | Selected |
|--------|-------------|----------|
| Aggregate result | FanOutResult { successes, failures } returned to caller | |
| Independent tracking | Each branch stored in FanOutTracker, caller polls | |
| Callback-based | User provides on_sink_complete(branch_id, result) callback | ✓ |

**User's choice:** Callback-based
**Notes:** Fire-and-forget style per branch, primary message ACKed immediately

---

## Max Fan-Out Overflow Behavior

| Option | Description | Selected |
|--------|-------------|----------|
| Reject immediately | Message ACKs and goes to DLQ with capacity exceeded | |
| Backpressure | Kafka pauses, message waits in queue until slot frees | ✓ |
| Queue overflow | Message buffered in overflow queue, processed FIFO | |

**User's choice:** Backpressure
**Notes:** Recommended approach — aligns with existing pause/resume pattern, self-healing under load

---

## Fan-Out Degree Scope

| Option | Description | Selected |
|--------|-------------|----------|
| Global consumer config | Single max_fan_out for entire consumer | |
| Per-handler config | Each handler declares its own fan-out ceiling | ✓ |
| Per-message override | Dynamic per-message fan-out degree | |

**User's choice:** Per-handler config
**Notes:** max_fan_out is per-handler config (not global consumer config)

---

## Deferred Ideas

None — discussion stayed within phase scope.
