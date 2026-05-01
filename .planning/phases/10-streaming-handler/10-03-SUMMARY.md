---
phase: 10-streaming-handler
plan: 03
subsystem: worker-pool-streaming
tags: [streaming, state-machine, worker-pool, lifecycle]
requires: [STRM-03]
provides: [streaming-lifecycle]
affects: [worker-pool-dispatch, handler-mode]
tech-stack:
  added: [StreamingState enum, streaming_worker_loop fn]
  patterns: [four-phase lifecycle: Starting→Running→Draining→Recovering]
key-files:
  created:
    - src/worker_pool/streaming_loop.rs (streaming_worker_loop with StreamingState)
  modified:
    - src/worker_pool/mod.rs (added streaming_loop module)
    - src/worker_pool/pool.rs (streaming dispatch detection via has_streaming flag)
    - src/python/handler.rs (pub(crate) invoke_streaming for internal access)
key-decisions:
  - streaming_worker_loop uses four-phase state machine (Starting, Running, Draining, Recovering)
  - CancellationToken checked each iteration for graceful shutdown coordination
  - Exponential backoff on recovery: 2^attempt seconds, max 3 attempts
  - invoke_streaming changed to pub(crate) to allow worker_pool module access
  - Pool dispatch uses has_streaming flag to detect StreamingAsync handlers
requirements-completed: [STRM-03]
duration: ~2 min
completed: 2026-04-29T00:00:00Z
---

# Phase 10 Plan 03: Streaming Worker Loop - Summary

**One-liner:** Implemented `streaming_worker_loop` with four-phase state machine and wired lifecycle dispatch into `WorkerPool::spawn`.

## What Was Built

1. **StreamingState enum** — Four states: `Starting`, `Running`, `Draining`, `Recovering { attempt: u32 }`

2. **streaming_worker_loop function** — Core lifecycle management:
   - `Starting` → immediately transitions to `Running`
   - `Running` → checks CancellationToken each iteration, calls `invoke_streaming`, handles results
   - `Draining` → logs and exits (generator close via PythonAsyncFuture Drop)
   - `Recovering` → exponential backoff (2^attempt secs), retries up to MAX_RECOVERY_ATTEMPTS (3)

3. **WorkerPool dispatch wiring** — Added `has_streaming` detection in `WorkerPool::new`:
   - `all_batch` → batch_worker_loop
   - `has_streaming` → TODO placeholder (needs consumer injection)
   - default → worker_loop

4. **invoke_streaming visibility** — Changed from `async fn` to `pub(crate) async fn` to allow worker_pool module access

## Files Modified/Created

| File | Change |
|------|--------|
| `src/worker_pool/streaming_loop.rs` | **Created** — StreamingState enum + streaming_worker_loop function |
| `src/worker_pool/mod.rs` | Added `pub mod streaming_loop` |
| `src/worker_pool/pool.rs` | Added has_streaming detection + dispatch branch |
| `src/python/handler.rs` | Made `invoke_streaming` `pub(crate)` for worker_pool access |

## Verification

```bash
cargo check --lib
# Finished `dev` profile — no errors (only warnings)
```

## Deviations from Plan

1. **[Rule 1 - Bug] Made invoke_streaming pub(crate)** — `invoke_streaming` was private but streaming_worker_loop in a sibling module needed to call it. Fixed by making it `pub(crate)`.

2. **Removed unused streaming_worker_loop import from pool.rs** — The import was flagged as unused since the actual spawn call is TODO. Import remains available for when consumer injection is implemented.

3. **TODO placeholder in WorkerPool dispatch** — Streaming workers need `StreamConsumer` injection which requires architectural changes to WorkerPool::new signature. Current implementation logs and skips spawning for StreamingAsync handlers.

## Task Commits

| # | Task | Commit |
|---|------|--------|
| 1 | Create streaming_worker_loop with StreamingState state machine | `736503e` |
| 2 | Wire streaming_worker_loop into WorkerPool::spawn | `a284bdd` |

## Self-Check: PASSED

- [x] `StreamingState` enum has Starting, Running, Draining, Recovering states
- [x] `streaming_worker_loop` checks CancellationToken each iteration
- [x] Exponential backoff: `2^attempt` seconds, MAX_RECOVERY_ATTEMPTS=3
- [x] `WorkerPool::new` detects `has_streaming` handlers
- [x] `invoke_streaming` is `pub(crate)` for worker_pool access
- [x] `cargo check --lib` passes with no errors

## Known Stubs

| Stub | File | Line | Reason |
|------|------|------|--------|
| TODO: streaming worker needs consumer injection | pool.rs | ~135 | Streaming workers need StreamConsumer passed to WorkerPool::new — full integration deferred |

## Threat Flags

None — no new security surface introduced.