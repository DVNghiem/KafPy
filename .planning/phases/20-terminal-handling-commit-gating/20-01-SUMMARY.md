# Phase 20 Plan 01: Terminal Handling & Commit Gating — Summary

**Phase:** 20-terminal-handling-commit-gating
**Plan:** 01
**Status:** COMPLETE
**Completed:** 2026-04-17

---

## One-liner

Added `has_terminal` flag to `PartitionState`, wired commit gating in `should_commit`, and connected terminal detection from `worker_loop` failure paths via `mark_failed`.

---

## Objective

Add `has_terminal` flag to `PartitionState`, implement commit gating in `should_commit`, and wire terminal detection from `worker_loop` failure handling into `mark_failed`.

---

## Tasks Executed

| # | Name | Commit | Files |
|---|------|--------|-------|
| 1 | Add `has_terminal` to PartitionState | 9b78f27 | src/coordinator/offset_tracker.rs |
| 2 | Gate should_commit when has_terminal=true | (bundled above) | src/coordinator/offset_tracker.rs |
| 3 | Wire has_terminal in mark_failed | (bundled above) | src/coordinator/offset_tracker.rs |

All 3 tasks implemented in a single atomic commit due to file-level edit atomicity.

---

## Changes Made

### src/coordinator/offset_tracker.rs

**Task 1 — PartitionState:**
- Added `has_terminal: bool` field to `PartitionState` struct
- Initialized to `false` in `PartitionState::new()`
- Added `set_terminal(&mut self)` method (idempotent — D-03: set once, never clears)

**Task 2 — should_commit gating:**
- Updated `should_commit` to return `false` when `has_terminal=true` for that partition
- Per-partition blocking — other partitions unaffected (D-01)
- Refactored `map_or(false, |s| {...})` to `is_some_and(|s| {...})` for clippy compliance

**Task 3 — mark_failed wiring:**
- Renamed `_reason` parameter to `reason` (removed underscore — now used)
- Added `if reason.category() == FailureCategory::Terminal { state.has_terminal = true; }`
- D-05: `has_terminal` set in `worker_loop` failure paths via `mark_failed` with Terminal category

**Import change:**
- Added `FailureCategory` to `use crate::failure::{FailureCategory, FailureReason};`

---

## Decisions Made

| Decision | Description |
|----------|-------------|
| D-01 | Commit gating is per-partition — `has_terminal=true` on TP-0 does NOT block TP-1 |
| D-03 | `has_terminal=true` is set once and never cleared for partition lifetime |
| D-05 | `has_terminal` set via `mark_failed` when `reason.category() == FailureCategory::Terminal` |

---

## Verification

| Check | Result |
|-------|--------|
| `cargo build --lib` | PASS (22 pre-existing warnings, 0 errors) |
| `cargo clippy --lib` | PASS (0 new errors — pre-existing PyO3 linking error in test binary) |
| `grep -n "has_terminal" offset_tracker.rs` | 7 occurrences confirmed |

---

## Deviations from Plan

**None** — plan executed exactly as written.

---

## Deferred Items

- PyO3 linking error in test binary (pre-existing, documented in STATE.md)
- Phase 20 Plan 02: graceful_shutdown flush (separate plan)

---

## Threat Flags

| Flag | File | Description |
|------|------|-------------|
| None | — | No new surface introduced — changes only add state field and early-return logic |

---

## Self-Check

- [x] All 3 tasks implemented and committed
- [x] `PartitionState` has `has_terminal: bool` (initialized false)
- [x] `set_terminal()` method added (idempotent)
- [x] `should_commit` returns false when `has_terminal=true`
- [x] `mark_failed` sets `has_terminal=true` when `FailureCategory::Terminal`
- [x] `cargo build --lib` passes
- [x] `cargo clippy --lib` shows no new errors
- [x] `FailureCategory::Terminal` exported via `use` from `src/failure/reason.rs`
