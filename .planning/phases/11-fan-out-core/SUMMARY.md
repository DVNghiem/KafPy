# Phase 11 Summary: Fan-Out Core

**Built:** 2026-04-29
**Requirements:** FANOUT-01, FANOUT-02, FANOUT-03

## What Was Built

### New File: `src/worker_pool/fan_out.rs`

**`BranchResult`** — Enum capturing sink branch outcomes:
- `Ok` — handler completed successfully
- `Error { reason, exception }` — handler raised exception
- `Timeout { timeout_ms }` — handler timed out

**`SinkConfig`** — Topic + handler pair for a single fan-out branch.

**`FanOutConfig`** — Per-handler fan-out config with `Vec<SinkConfig>`, `max_fan_out` (capped at 64), optional `FanOutSlotManager`.

**`CallbackRegistry`** — std::sync::Mutex-wrapped HashMap of `branch_id -> Box<dyn Fn(BranchResult)>`.
- `on_sink_complete(callback)` registers a callback and returns a unique `branch_id`
- `emit(branch_id, result)` invokes the callback and removes it

**`FanOutTracker`** — Per-message dispatch tracker:
- `new(max_fan_out)` — caps at 64
- `try_acquire_slot()` — CAS loop, returns false when exhausted
- `release_slot()` — decrements inflight
- `is_exhausted()` — true when inflight >= max
- `on_sink_complete()` / `emit_completion()` — callback plumbing

**`FanOutSlotManager`** — Semaphore-based bounded concurrency:
- `new(max_fan_out, topic)` — caps at 64
- `acquire().await` — returns `Ok(())` or `Err(BackpressureAction::PausePartition)`
- `available()` — current permit count

### Modified: `src/python/handler.rs`

- Added `fan_out: Option<Arc<FanOutConfig>>` field to `PythonHandler`
- Added `fan_out_config()` getter — returns `Option<&FanOutConfig>`
- Added `set_fan_out(config)` setter
- Updated constructors to initialize `fan_out: None`

### Modified: `src/worker_pool/worker.rs`

- Added `JoinSet` import
- Added fan-out dispatch after primary handler result handling:
  - Primary ACK fires immediately (FANOUT-03)
  - If `handler.fan_out_config()` is present, spawn sink futures via `JoinSet`
  - Each sink calls `emit_completion(branch_id, result)` on completion
  - `release_slot()` called after each sink branch completes
  - `FanOutTracker` shared across all sink branches via `Arc`
  - Backpressure check via `is_exhausted()` before spawning (FANOUT-02)

### Modified: `src/worker_pool/mod.rs`

- Added `pub mod fan_out;`

## Acceptance Criteria Met

| Criterion | Status |
|-----------|--------|
| `FanOutTracker::new(128).max_fan_out == 64` | Verified in tests |
| `try_acquire_slot` returns false when inflight >= max | Verified in tests |
| `release_slot` decrements inflight | Verified in tests |
| `CallbackRegistry` generates unique branch_ids | Verified in tests |
| `FanOutSlotManager::new(128).available() == 64` | Verified in tests |
| `acquire().await` returns `Err(BackpressureAction::PausePartition)` when exhausted | Verified in tests |
| Primary ACK fires before sink futures complete | Implemented (ACK in match block before JoinSet spawn) |
| `cargo check` passes | Verified |
| `cargo build --lib` passes | Verified |

## Key Design Decisions

1. **Static per-handler sink registration** — sinks declared at handler registration time
2. **Callback-based sink failure tracking** — fire-and-forget per branch, primary ACKed immediately
3. **Backpressure on overflow** — `PausePartition` returned when slots exhausted (not reject)
4. **Per-handler max_fan_out** — each handler declares its own ceiling up to global max 64
5. **std::sync::Mutex for CallbackRegistry** — brief synchronous lock during mutation, no `.await` across boundary

## Files Changed

- `src/worker_pool/fan_out.rs` — new
- `src/python/handler.rs` — modified (fan_out field + accessors)
- `src/worker_pool/worker.rs` — modified (JoinSet dispatch)
- `src/worker_pool/mod.rs` — modified (pub mod fan_out)