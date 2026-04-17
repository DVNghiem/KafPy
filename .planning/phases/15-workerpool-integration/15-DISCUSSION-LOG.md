# Phase 15: WorkerPool Integration - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-04-17
**Phase:** 15-workerpool-integration
**Areas discussed:** graceful_shutdown API, shutdown ordering, record_ack timing, mark_failed timing

---

## graceful_shutdown API

| Option | Description | Selected |
|--------|-------------|----------|
| Add all_partitions() to OffsetTracker (Recommended) | Iterate HashMap keys, graceful_shutdown() calls should_commit per pair | ✓ |
| Track registered partitions in coordinator | Add register/all_registered to OffsetCoordinator trait | |
| Pass topic-partitions from WorkerPool shutdown | graceful_shutdown takes &[(&str, i32)] from caller | |

**User's choice:** Add all_partitions() to OffsetTracker (Recommended)
**Notes:** Preferred for clean API — thin wrapper over existing HashMap keys.

---

## graceful_shutdown no-op behavior

| Option | Description | Selected |
|--------|-------------|----------|
| No-op if nothing ready (Recommended) | Skip partitions where should_commit() is false. Silent. | ✓ |
| Commit highest contiguous even if gaps | Commit even with gaps — aggressive, may duplicate work | |
| Warn if nothing ready | Log warning for debugging purposes | |

**User's choice:** No-op if nothing ready (Recommended)
**Notes:** Standard approach — expected behavior when no messages processed before shutdown.

---

## Shutdown ordering

| Option | Description | Selected |
|--------|-------------|----------|
| WorkerPool::shutdown() calls it (Recommended) | pool.shutdown() calls graceful_shutdown() before joining workers | ✓ |
| Consumer calls it separately | Consumer holds offset_tracker reference, calls after pool.run() | |
| Both — pool then Consumer | pool cancels workers, Consumer then finalizes commits | |

**User's choice:** WorkerPool::shutdown() calls it (Recommended)
**Notes:** Co-locates shutdown logic with coordinator owner.

---

## record_ack timing

| Option | Description | Selected |
|--------|-------------|----------|
| After queue.ack, fire-and-forget (Recommended) | queue_manager.ack first, then record_ack fire-and-forget | ✓ |
| Before queue.ack, then queue.ack | record_ack first — risk: coordinator succeeds but queue fails | |
| Before queue.ack, blocking | Wait for record_ack to complete, then queue.ack | |

**User's choice:** After queue.ack, fire-and-forget (Recommended)
**Notes:** Consistent with queue ack ordering pattern.

---

## mark_failed timing

| Option | Description | Selected |
|--------|-------------|----------|
| After queue ack, fire-and-forget (Recommended) | After error trace, fire-and-forget | ✓ |
| Blocking | mark_failed should block until confirmed | |
| Fire-and-forget independent of queue | No queue involvement — just track failed offsets | |

**User's choice:** After queue ack, fire-and-forget (Recommended)
**Notes:** Consistent with record_ack pattern (D-04).

---

## Claude's Discretion

The exact line-level placement of `record_ack`/`mark_failed` calls within the match arms — left to executor's judgment for idiomatic Rust placement.

## Deferred Ideas

None.
