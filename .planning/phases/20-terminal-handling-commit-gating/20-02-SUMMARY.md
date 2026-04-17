# Phase 20 Plan 02: Terminal Handling & Commit Gating — Summary

**Phase:** 20-terminal-handling-commit-gating
**Plan:** 02
**Status:** COMPLETE
**Completed:** 2026-04-17

---

## One-liner

Implemented `flush_failed_to_dlq` in `OffsetTracker`, wired it into `WorkerPool::shutdown` before `graceful_shutdown`, with fire-and-forget DLQ routing.

---

## Objective

Implement `flush_failed_to_dlq` in OffsetTracker, wire it into WorkerPool::shutdown, and update `graceful_shutdown` to flush all failed (retryable + terminal) to DLQ before final commit.

---

## Tasks Executed

| # | Name | Commit | Files |
|---|------|--------|-------|
| 1 | Add flush_failed_to_dlq to OffsetCoordinator trait and OffsetTracker | 20b1db0 | src/coordinator/offset_coordinator.rs, src/coordinator/offset_tracker.rs |
| 2 | Wire flush_failed_to_dlq into WorkerPool::shutdown | f0b3d6e | src/worker_pool/mod.rs |
| 3 | Update graceful_shutdown docs | (bundled in 20b1db0) | src/coordinator/offset_tracker.rs |

Tasks 1 and 3 were implemented together in a single atomic commit (both in offset_tracker.rs).
Task 2 was implemented separately in worker_pool/mod.rs.

---

## Changes Made

### src/coordinator/offset_coordinator.rs

**Task 1 — OffsetCoordinator trait:**
- Added `flush_failed_to_dlq` method to trait with full doc comment
- Added imports: `crate::dlq::{DlqRouter, SharedDlqProducer}`

### src/coordinator/offset_tracker.rs

**Task 1 — flush_failed_to_dlq implementation:**
- Iterates all partitions via `all_partitions()`
- For each partition with `failed_offsets`, produces all failed offsets to DLQ
- Fire-and-forget via `dlq_producer.produce_async()` — no broker ack blocking
- Uses `last_failure_reason` from `PartitionState` for DLQ metadata
- Uses empty payload (known limitation — OffsetTracker does not store original message)
- Logs warning when producing empty DLQ message (limitation documented)

**Task 3 — graceful_shutdown docs:**
- Added doc comment explaining flush-before-commit ordering (D-02)
- Notes that `graceful_shutdown` is called AFTER `flush_failed_to_dlq` by WorkerPool::shutdown

### src/worker_pool/mod.rs

**Task 2 — shutdown wiring:**
- `WorkerPool::shutdown` now calls `flush_failed_to_dlq` before `graceful_shutdown`
- Ordering: cancel -> flush -> commit -> join_set shutdown
- Per D-02: ensures all retryable + terminal failures drained to DLQ before final commit

---

## Decisions Made

| Decision | Description |
|----------|-------------|
| D-02 | Graceful shutdown flushes ALL failed (retryable + terminal) to DLQ before final commit — `flush_failed_to_dlq` drains `failed_offsets` BTreeSet which contains both types |

---

## Known Limitations

**Payload-less DLQ flush:** `OffsetTracker` does not store original message payloads or keys. When `flush_failed_to_dlq` re-produces failed messages to DLQ, it uses empty payloads. This is a pre-existing design limitation — original message content is not available at flush time. The warning log documents this: `"flush_failed_to_dlq: original payload not available, producing empty DLQ message"`.

---

## Verification

| Check | Result |
|-------|--------|
| `cargo build --lib` | PASS (22 pre-existing warnings, 0 errors) |
| `cargo clippy --lib` | PASS (0 new errors — pre-existing PyO3 linking error in test binary) |

---

## Deviations from Plan

**None** — plan executed as written.

---

## Deferred Items

- PyO3 linking error in test binary (pre-existing, documented in STATE.md)
- Phase 20 Plan 03 (if any): not yet planned

---

## Threat Flags

| Flag | File | Description |
|------|------|-------------|
| None | — | Fire-and-forget produce has bounded channel (~100) with drop-on-full behavior — acceptable for shutdown flush (T-20-03 accepted risk) |

---

## Self-Check

- [x] `flush_failed_to_dlq` added to `OffsetCoordinator` trait
- [x] `flush_failed_to_dlq` implemented in `OffsetTracker` — iterates all partitions, produces failed offsets via `produce_async`
- [x] `WorkerPool::shutdown` calls `flush_failed_to_dlq` before `graceful_shutdown`
- [x] `graceful_shutdown` doc comment clarifies flush-before-commit ordering
- [x] `cargo build --lib` passes
- [x] `cargo clippy --lib` shows no new errors
