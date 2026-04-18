# Phase 25: Batch Accumulation & Flush - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-04-18
**Phase:** 25-batch-accumulation-flush
**Areas discussed:** BatchAccumulator structure, Flush trigger, Backpressure, Batch result extraction, PartialFailure

---

## Area 1: BatchAccumulator Structure

| Option | Description | Selected |
|--------|-------------|----------|
| Dedicated struct | BatchAccumulator in worker_pool/ — cleaner unit testing | ✓ |
| Embedded in worker_loop | Phase 24 precedent — logic inline | |

**User's choice:** Dedicated struct — "follow big tech" pattern; matches OffsetTracker/QueueManager precedent

---

## Area 2: Flush Trigger

| Option | Description | Selected |
|--------|-------------|----------|
| Recalculate on each message | Timer resets on each arrival; max_batch_wait_ms per-message | |
| Fixed window (Recommended) | Timer starts on first message; deadline fixed; consistent timing | ✓ |

**User's choice:** Fixed window — consistent flush timing regardless of arrival pace

---

## Area 3: Backpressure During Accumulation

| Option | Description | Selected |
|--------|-------------|----------|
| Block accumulation | Pause recv() until backpressure clears; timer keeps running | |
| Flush immediately (Recommended) | Flush batch on backpressure, then block; accumulator never overflows | ✓ |
| Timer handles it | Ignore backpressure; timer fires eventually | |

**User's choice:** Flush immediately on backpressure — clean separation, no overflow

---

## Area 4: Batch Result Extraction

| Option | Description | Selected |
|--------|-------------|----------|
| Inline iteration (Recommended) | worker_loop matches result and iterates per-message | ✓ |
| Batch-aware executor | executor.execute_batch() handles iteration internally | |

**User's choice:** Inline iteration — explicit, easy to trace/log each ack

---

## Area 5: PartialFailure

| Option | Description | Selected |
|--------|-------------|----------|
| Skip PartialFailure (Recommended) | Only AllSuccess/AllFailure; PartialFailure is v1.7+ extension | ✓ |
| Implement partials now | Implement PartialFailure in Phase 25 too | |

**User's choice:** Skip PartialFailure — v1.7+ extension point, not in scope

---

*Phase: 25-batch-accumulation-flush*
*Discussion complete: 2026-04-18*
