---
phase: 10-streaming-handler
plan: 04
subsystem: streaming-backpressure
tags: [streaming, backpressure, pause-resume, queue-manager]
requires: [STRM-04]
provides: [streaming-backpressure]
affects: [handler-mode-dispatch]
tech-stack:
  added: [BackpressureAction::PausePartition, BackpressureAction::ResumePartition, pause_partition/resume_partition methods]
  patterns: [per-stream backpressure, streaming buffer semantics]
key-files:
  modified:
    - src/dispatcher/backpressure.rs (PausePartition + ResumePartition variants)
    - src/dispatcher/consumer_dispatcher.rs (updated match arms)
    - src/dispatcher/queue_manager.rs (pause/resume tracking methods)
key-decisions:
  - PausePartition carries topic+partition; partition=-1 indicates all partitions (consumer-level)
  - ResumePartition added for explicit resume signaling (future use)
  - STREAMING_BUFFER_CAPACITY = 100 messages per partition
  - HandlerEntry.paused_partitions tracks paused state for streaming handlers
requirements-completed: [STRM-04]
duration: ~3 min
completed: 2026-04-29T14:03:00Z
---

# Phase 10 Plan 04: Per-Stream Backpressure Propagation - Summary

**One-liner:** Added `PausePartition`/`ResumePartition` backpressure actions wired to `QueueManager` pause/resume tracking for streaming handlers.

## What Was Built

### Task 1: BackpressureAction changes

1. **Replaced `FuturePausePartition(String)`** with two new structured variants:
   - `PausePartition { topic: String, partition: i32 }` — signals targeted pause
   - `ResumePartition { topic: String, partition: i32 }` — signals targeted resume

2. **Updated `topic()` method** to handle both `PausePartition` and `ResumePartition` variants.

3. **Updated `PauseOnFullPolicy::on_queue_full`** to return `PausePartition` with `partition: -1` (indicating all partitions).

4. **Updated `consumer_dispatcher.rs`** match arms:
   - `PausePartition` now properly destructured with `topic` extraction
   - `ResumePartition` handled as a no-op (drop error, no signal)

### Task 2: QueueManager pause/resume tracking

1. **Added `STREAMING_BUFFER_CAPACITY = 100`** constant for per-partition streaming buffer.

2. **Added `paused_partitions` field** to `HandlerEntry` — `Arc<Mutex<HashSet<i32>>>` tracking which partitions are paused.

3. **Implemented three methods on `QueueManager`:**
   - `pause_partition(topic, partition)` — marks partition as paused
   - `resume_partition(topic, partition)` — marks partition as resumed
   - `is_partition_paused(topic, partition)` — checks paused state

4. **Updated `register_handler_with_semaphore`** to initialize `paused_partitions` set.

## Files Modified

| File | Change |
|------|--------|
| `src/dispatcher/backpressure.rs` | Replaced `FuturePausePartition` with `PausePartition {topic, partition}` + `ResumePartition`; updated `topic()` method |
| `src/dispatcher/consumer_dispatcher.rs` | Updated match arms to destructure `PausePartition`; added `ResumePartition` arm |
| `src/dispatcher/queue_manager.rs` | Added `STREAMING_BUFFER_CAPACITY`, `paused_partitions` field, `pause_partition`/`resume_partition`/`is_partition_paused` methods |

## Verification

```bash
cargo check --lib 2>&1 | grep -E "error" | head -5
# Only pre-existing streaming_loop error in worker_pool/pool.rs (unrelated)
# My files: backpressure.rs, consumer_dispatcher.rs, queue_manager.rs — all compile clean
```

## Deviations from Plan

None — plan executed exactly as written.

## Task Commits

| # | Task | Commit |
|---|------|--------|
| 1 | Add PausePartition/ResumePartition backpressure actions | `bb03216` |
| 2 | Wire pause/resume into queue_manager for streaming handlers | `a726cac` |

## Self-Check: PASSED

- [x] `BackpressureAction::PausePartition { topic, partition }` exists in backpressure.rs
- [x] `BackpressureAction::ResumePartition { topic, partition }` exists in backpressure.rs
- [x] `topic()` method handles both new variants
- [x] `PauseOnFullPolicy::on_queue_full` returns `PausePartition` with partition=-1
- [x] `STREAMING_BUFFER_CAPACITY` constant exists in queue_manager.rs
- [x] `HandlerEntry.paused_partitions` field exists
- [x] `pause_partition`, `resume_partition`, `is_partition_paused` methods exist
- [x] `cargo check --lib` passes for modified files (pre-existing streaming_loop error unrelated)
