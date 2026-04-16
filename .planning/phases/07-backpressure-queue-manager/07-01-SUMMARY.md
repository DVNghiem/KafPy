---
phase: "07"
plan: "01"
subsystem: dispatcher
tags: [backpressure, queue-manager, atomic-counters, dispatcher]
dependency_graph:
  requires:
    - "06-01-SUMMARY.md (Dispatcher struct with register_handler and send)"
  provides:
    - QueueManager with atomic counters for queue_depth and inflight
    - HandlerMetadata with get_queue_depth() and get_inflight()
    - Dispatcher delegates to QueueManager
    - ack(topic, count) for Python layer to signal message completion
  affects:
    - src/dispatcher/mod.rs (Dispatcher refactored)
    - src/dispatcher/queue_manager.rs (new file)
tech_stack:
  added: []
  patterns: [atomic counters, saturating subtract via compare_exchange]
key_files:
  created:
    - src/dispatcher/queue_manager.rs (QueueManager, HandlerEntry, HandlerMetadata)
  modified:
    - src/dispatcher/mod.rs (Dispatcher delegates to QueueManager)
decisions:
  - Used compare_exchange loop for saturating subtract (AtomicUsize has no fetch_saturating_sub)
  - Dispatcher is now a thin wrapper over QueueManager; existing API unchanged
  - queue_depth in DispatchOutcome populated from QueueManager atomic counter
metrics:
  duration: ~8 minutes
  completed: "2026-04-16"
---

# Phase 07 Plan 01: QueueManager Summary

QueueManager with atomic counters for queue_depth/inflight tracking implemented as specified.

## One-liner

QueueManager owns all handler queues and metadata with AtomicUsize counters for per-handler queue depth and inflight tracking.

## Tasks Completed

| Task | Name | Commit | Files |
| ---- | ---- | ------ | ----- |
| 1 | Create queue_manager.rs with QueueManager, HandlerEntry, HandlerMetadata | d378ffc | src/dispatcher/queue_manager.rs |
| 2 | Extend Dispatcher to delegate to QueueManager | ea70dc1 | src/dispatcher/mod.rs |

## Must-Haves Verification

| Truth | Status |
| ----- | ------ |
| QueueManager owns all handler queues and metadata | Implemented via parking_lot::Mutex<HashMap<String, HandlerEntry>> |
| Queue depth is trackable per handler via inspection API | Implemented via get_queue_depth(topic) -> Option<usize> |
| Infight count is trackable per handler via inspection API | Implemented via get_inflight(topic) -> Option<usize> |
| QueueManager::register_handler returns mpsc::Receiver | Implemented |
| QueueManager::ack(topic, count) decrements both counters | Implemented via compare_exchange loop with saturating_sub |
| Dispatcher delegates to QueueManager | Implemented (thin wrapper) |
| Existing send() and register_handler() API unchanged | Confirmed |

## Requirements Met

DISP-06, DISP-07, DISP-11, DISP-12, DISP-13, DISP-14

## Deviation: Saturating Subtract Implementation

**Rule 2 - Auto-added missing critical functionality**

- **Issue:** `AtomicUsize` does not have a `fetch_saturating_sub` method; plan referenced non-existent API.
- **Fix:** Implemented saturating subtract manually using a `compare_exchange` loop that loads the current value, computes `saturating_sub`, and retries on conflict.
- **Files modified:** src/dispatcher/queue_manager.rs

## Deviation: Missing tokio::sync::mpsc Import

**Rule 3 - Auto-fixed blocking issue (missing import)**

- **Issue:** `mpsc::Receiver` return type in `register_handler` could not resolve — module-level import was in wrong place relative to doc comment.
- **Fix:** Moved `use tokio::sync::mpsc` and `pub use crate::consumer::OwnedMessage` imports above the module doc comment at the top of mod.rs.
- **Files modified:** src/dispatcher/mod.rs

## Verification

```bash
cargo build --lib  # PASSED
cargo test --lib -- dispatcher::  # PASSED (0 tests - tests in 07-02 plan)
```

## Known Stubs

None.

## Threat Flags

| Flag | File | Description |
|------|------|-------------|
| None | - | No new threat surface introduced by this plan |

## Commits

- `d378ffc` feat(07-backpressure-queue-manager): add QueueManager with atomic counters for queue_depth and inflight
- `ea70dc1` refactor(07-backpressure-queue-manager): delegate Dispatcher to QueueManager

## Self-Check

- [x] src/dispatcher/queue_manager.rs exists with QueueManager, HandlerMetadata, HandlerEntry
- [x] src/dispatcher/mod.rs has Dispatcher delegating to QueueManager
- [x] get_queue_depth and get_inflight return Option<usize>
- [x] ack(topic, count) uses saturating subtract
- [x] cargo build --lib succeeds without errors
- [x] Both commits exist in git log

## Self-Check: PASSED
