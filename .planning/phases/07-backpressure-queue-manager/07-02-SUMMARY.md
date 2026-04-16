---
phase: "07"
plan: "02"
subsystem: dispatcher
tags: [backpressure, queue-manager, dispatcher, policy-trait]
dependency_graph:
  requires:
    - "07-01-SUMMARY.md (QueueManager with atomic counters)"
  provides:
    - BackpressurePolicy trait with on_queue_full hook
    - BackpressureAction enum with Drop, Wait, FuturePausePartition
    - DispatchError::Backpressure variant
    - send_with_policy method for custom backpressure policies
tech_stack:
  added: []
  patterns: [trait-based policy, enum action variants, non-blocking backpressure]
key_files:
  created:
    - src/dispatcher/backpressure.rs (BackpressurePolicy, BackpressureAction, DefaultBackpressurePolicy, PauseOnFullPolicy)
  modified:
    - src/dispatcher/error.rs (added Backpressure variant)
    - src/dispatcher/mod.rs (send_with_policy, send delegates to it)
    - src/dispatcher/queue_manager.rs (pub(crate) visibility for HandlerMetadata, HandlerEntry, handlers field)
decisions:
  - BackpressurePolicy is pub(crate) since HandlerMetadata is crate-internal
  - send_with_policy is pub(crate) to match trait visibility
  - BackpressureAction::FuturePausePartition carries topic name for Phase 8 DISP-18 pause/resume
  - QueueFull (Phase 6) remains internal; Backpressure is public API per DISP-08
metrics:
  duration: ~5 minutes
  completed: "2026-04-16"
---

# Phase 07 Plan 02: BackpressurePolicy and BackpressureAction Summary

BackpressurePolicy trait, BackpressureAction enum, and DispatchError::Backpressure implemented as specified.

## One-liner

Policy-driven backpressure: send() consults BackpressurePolicy to decide action when queue is full, returning DispatchError::Backpressure per DISP-08.

## Tasks Completed

| Task | Name | Commit | Files |
| ---- | ---- | ------ | ----- |
| 1 | Create backpressure.rs with BackpressurePolicy trait and BackpressureAction enum | 38dccfb | src/dispatcher/backpressure.rs |
| 2 | Add DispatchError::Backpressure variant | 38dccfb | src/dispatcher/error.rs |
| 3 | Modify send() to use BackpressurePolicy and return Backpressure error | 38dccfb | src/dispatcher/mod.rs, queue_manager.rs |

## Must-Haves Verification

| Truth | Status |
| ----- | ------ |
| send() returns DispatchError::Backpressure when queue full (per DISP-08) | Implemented via send_with_policy |
| BackpressurePolicy trait exists with on_queue_full hook | Implemented in backpressure.rs |
| BackpressureAction has Drop, Wait, FuturePausePartition variants | Implemented as enum |
| DefaultBackpressurePolicy returns Drop action | Implemented |
| send_with_policy returns DispatchError::Backpressure when queue full | Implemented |
| FuturePausePartition carries topic name as signal | FuturePausePartition(String) |

## Requirements Met

DISP-08, DISP-09, DISP-10

## Deviations from Plan

### Rule 3 - Auto-fixed blocking issue (visibility)

- **Issue:** `handlers` field (QueueManager), `queue_depth`/`inflight` fields (HandlerMetadata), and `metadata` field (HandlerEntry) were private, preventing `send_with_policy` in `mod.rs` from accessing them directly.
- **Fix:** Changed visibility to `pub(crate)` on all three structs and their relevant fields. Also made `BackpressurePolicy` trait and `Dispatcher::send_with_policy` method `pub(crate)` to match the crate-internal visibility of `HandlerMetadata`.
- **Files modified:** src/dispatcher/queue_manager.rs, src/dispatcher/mod.rs

## Verification

```bash
cargo build --lib  # PASSED
```

## Known Stubs

None.

## Threat Flags

| Flag | File | Description |
|------|------|-------------|
| None | - | No new threat surface introduced by this plan |

## Commits

- `38dccfb` feat(07-backpressure-queue-manager): add BackpressurePolicy trait and BackpressureAction enum

## Self-Check

- [x] src/dispatcher/backpressure.rs exists with BackpressurePolicy trait and BackpressureAction enum
- [x] DispatchError::Backpressure variant exists in error.rs
- [x] send() delegates to send_with_policy with DefaultBackpressurePolicy
- [x] send_with_policy returns DispatchError::Backpressure when queue full
- [x] BackpressureAction::FuturePausePartition carries topic name
- [x] cargo build --lib succeeds without errors
- [x] Commit 38dccfb exists in git log

## Self-Check: PASSED
