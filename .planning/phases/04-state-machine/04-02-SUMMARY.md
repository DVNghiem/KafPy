---
phase: "04"
plan: "02"
subsystem: coordinator
tags: [state-machine, retry, enum, exhaustive-match]
dependency_graph:
  requires:
    - "04-01"
  provides:
    - "RetryState enum replacing MessageRetryState struct"
  affects:
    - "src/coordinator/retry_coordinator.rs"
tech_stack:
  added:
    - "RetryState enum (Retrying + Exhausted variants)"
  patterns:
    - "Enum state machine (illegal states unrepresentable)"
    - "Exhaustive match arms (no wildcard _)"
key_files:
  created: []
  modified:
    - "src/coordinator/retry_coordinator.rs"
decisions:
  - "Replace MessageRetryState struct with RetryState enum with explicit Retrying/Exhausted states"
  - "RetryState methods use explicit match on both variants (exhaustive)"
  - "record_failure uses Entry pattern for atomic HashMap access"
metrics:
  duration: "2026-04-20T14:45:00Z"
  completed: "2026-04-20"
---

# Phase 04 Plan 02: RetryState enum Summary

## One-liner

Explicit `RetryState` enum (Retrying/Exhausted) replaces implicit HashMap-based retry state tracking with compiler-verified exhaustive matching.

## Completed Tasks

| Task | Name | Commit | Files |
|------|------|--------|-------|
| 1 | Create RetryState enum and refactor RetryCoordinator | f23a9cb | src/coordinator/retry_coordinator.rs |
| 2 | Verify exhaustive match arms across all phase 4 state enums | (included in f23a9cb) | src/coordinator/retry_coordinator.rs |

## What Was Built

### RetryState Enum

```rust
pub enum RetryState {
    Retrying {
        topic: String,
        partition: i32,
        offset: i64,
        attempt: usize,
        last_failure: FailureReason,
        first_failure: DateTime<Utc>,
    },
    Exhausted {
        topic: String,
        partition: i32,
        offset: i64,
        last_failure: FailureReason,
        first_failure: DateTime<Utc>,
    },
}
```

### Accessor Methods

Each accessor uses explicit match arms (no wildcard `_`):
- `topic()` - returns topic string
- `partition()` - returns partition i32
- `offset()` - returns offset i64
- `attempt()` - returns attempt count (0 for Exhausted)

### record_failure State Transitions

- First failure: `Vacant` -> inserts `RetryState::Retrying { attempt: 1, ... }`
- Subsequent failures: `Occupied(Retrying)` -> increments attempt or transitions to `Exhausted`
- After exhaustion: `Occupied(Exhausted)` -> idempotent return (false, true, None)

## Exhaustive Match Verification

All phase 4 state enums verified with exhaustive matches:

| Enum | File | Match Style |
|------|------|-------------|
| `WorkerState` | worker_pool/state.rs | `match` with explicit Idle/Processing arms |
| `BatchState` | worker_pool/state.rs | `matches!` macro (no match needed) |
| `ShutdownPhase` | coordinator/shutdown.rs | `match` with explicit 4-phase arms |
| `RetryState` | coordinator/retry_coordinator.rs | `match` with explicit Retrying/Exhausted arms |

## Deviations from Plan

None - plan executed exactly as written.

## Self-Check

- [x] RetryState enum with Retrying and Exhausted variants defined
- [x] RetryCoordinator uses RetryState instead of MessageRetryState
- [x] All match arms on RetryState are exhaustive - no wildcard `_`
- [x] All state enums in phase 4 verified with exhaustive matches
- [x] Code compiles without errors (cargo check --lib passes)
- [x] Commit created: f23a9cb
