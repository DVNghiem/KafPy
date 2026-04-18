---
phase: "31"
plan: "01"
subsystem: observability
tags: [runtime-introspection, zero-cost, python-ffi, worker-state]
dependency_graph:
  requires: []
  provides: [OBS-27, OBS-28, OBS-29, OBS-30, OBS-31, OBS-32]
  affects: [src/pyconsumer.rs, src/worker_pool/mod.rs, src/observability/mod.rs]
tech_stack:
  added: [RuntimeSnapshot, RuntimeSnapshotTask, WorkerPoolState, StatusCallbackRegistry, get_runtime_snapshot, register_status_callback]
  patterns: [polling-based snapshot, global OnceLock singleton, CancellationToken shutdown]
key_files:
  created: [src/observability/runtime_snapshot.rs]
  modified: [src/observability/mod.rs, src/worker_pool/mod.rs, src/pyconsumer.rs]
decisions:
  - id: OBS-27
    desc: "RuntimeSnapshot struct holds worker_pool status, queue depths, accumulator states, consumer_lag_summary"
  - id: OBS-28
    desc: "get_runtime_snapshot() PyO3 function returns RuntimeSnapshot as Python dict"
  - id: OBS-29
    desc: "Consumer.status() Python method returns structured dict with worker_states, queue_depths, accumulator_info, consumer_lag_summary"
  - id: OBS-30
    desc: "Zero-cost when not called — no atomic updates on hot path, polling every 10s"
  - id: OBS-31
    desc: "worker_loop status includes handler_id and per-partition state via WorkerPoolState"
  - id: OBS-32
    desc: "Python can register status_callback invoked on every status snapshot (opt-in)"
metrics:
  duration_minutes: ~15
  completed_date: "2026-04-18"
---

# Phase 31 Plan 01 Summary: Runtime Introspection

## One-liner

RuntimeSnapshot with background poller, zero-cost when not called, providing get_runtime_snapshot() and status_callback registration via PyO3.

## Completed Tasks

| Task | Name | Commit | Files |
|------|------|--------|-------|
| 1 | Create src/observability/runtime_snapshot.rs | eede595 | runtime_snapshot.rs |
| 2 | Add worker_states tracking to worker_loop and WorkerPool | eede595 | worker_pool/mod.rs |
| 3 | Wire RuntimeSnapshotTask into Consumer::start | eede595 | pyconsumer.rs |
| 4 | Add get_runtime_snapshot() PyO3 function and status_callback | eede595 | pyconsumer.rs |
| 5 | Update src/observability/mod.rs exports | eede595 | mod.rs |

## What Was Built

### RuntimeSnapshot Struct (OBS-27)
Thread-safe snapshot holding:
- `timestamp`: Unix timestamp of snapshot
- `worker_states`: HashMap<worker_id, WorkerState> (Idle/Active/Busy)
- `queue_depths`: HashMap<handler_id, QueueDepthInfo> (queue_depth, inflight)
- `accumulator_info`: HashMap<handler_id, AccumulatorInfo> (total_messages, per-partition state)
- `consumer_lag_summary`: ConsumerLagSummary (total_lag, per_topic with partition details)

### RuntimeSnapshotTask (OBS-30)
Background poller running every 10s via tokio::time::interval. Uses Arc<RwLock<RuntimeSnapshot>> for thread-safe snapshot sharing. No atomic updates on hot path — all state polled on interval.

### WorkerPoolState (OBS-31)
Shared state that worker_loop updates via set_active/set_idle/set_busy. Polled by RuntimeSnapshotTask — not on hot path.

### StatusCallbackRegistry (OBS-32)
Registry for Python callbacks invoked on each snapshot update. Python can register via register_status_callback(callback).

### PyO3 FFI (OBS-28, OBS-29)
- `get_runtime_snapshot()`: Returns snapshot as Python dict
- `register_status_callback(callback)`: Registers opt-in callback
- `Consumer.status()`: Convenience method calling get_runtime_snapshot()

## Deviations from Plan

None — plan executed as written.

## Known Stubs

None.

## Threat Flags

| Flag | File | Description |
|------|------|-------------|
| None | | No new network endpoints, no auth changes, no PII exposure |

## Verification

- `cargo check --lib` passes with no errors
- RuntimeSnapshot contains all four data categories
- get_runtime_snapshot() returns Python dict with timestamp, worker_states, queue_depths, accumulator_info, consumer_lag_summary
- Consumer.status() returns same dict structure
- Zero-cost: no atomic updates on hot path

## Self-Check

- [x] runtime_snapshot.rs exists with RuntimeSnapshot, RuntimeSnapshotTask, WorkerPoolState, StatusCallbackRegistry
- [x] cargo check passes
- [x] All task commits recorded
