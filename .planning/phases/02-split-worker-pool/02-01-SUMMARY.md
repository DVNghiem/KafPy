---
phase: "02-split-worker-pool"
plan: "02-01"
type: "execute"
wave: "1"
subsystem: "worker_pool"
tags: ["refactor", "module-split"]
dependency_graph:
  requires: []
  provides: ["PerPartitionBuffer", "BatchAccumulator"]
  affects: ["worker_pool/mod.rs", "python/mod.rs", "worker_pool/batch_loop.rs"]
tech_stack:
  added: []
  patterns: ["module-extract", "newtype-constructor"]
key_files:
  created:
    - "src/worker_pool/accumulator.rs"
    - "src/python/batch.rs"
  modified:
    - "src/worker_pool/mod.rs"
    - "src/python/mod.rs"
decisions:
  - id: "SPLIT-A-05"
    decision: "Rename PartitionAccumulator to PerPartitionBuffer to clarify general-purpose nature"
  - id: "SPLIT-A-01"
    decision: "Extract PartitionAccumulator to worker_pool/accumulator.rs"
  - id: "SPLIT-A-06"
    decision: "Move BatchAccumulator to python/batch.rs"
metrics:
  duration_minutes: 5
  completed_date: "2026-04-20"
---

# Phase 02 Plan 01: Extract PerPartitionBuffer + BatchAccumulator

## One-liner

Extracted PerPartitionBuffer and BatchAccumulator into focused modules, with PerPartitionBuffer renamed from PartitionAccumulator for clarity.

## Tasks Completed

| Task | Name | Commit | Files |
| ---- | ---- | ------ | ----- |
| 1 | Create worker_pool/accumulator.rs with PerPartitionBuffer | 50506bf | src/worker_pool/accumulator.rs |
| 2 | Move BatchAccumulator to python/batch.rs | 50506bf | src/python/batch.rs |
| 3 | Wire re-exports and import paths | 50506bf | src/worker_pool/mod.rs, src/python/mod.rs |

## Deviations from Plan

None - plan executed exactly as written.

## Behavior Preserved

- Fixed-window timer semantics unchanged (deadline set on first message, never reset)
- `is_deadline_expired()` polling behavior unchanged
- `flush_partition` and `flush_all` semantics unchanged
- Mutex interior mutability pattern preserved

## Encapsulation Improvements

- Added `PerPartitionBuffer::new()` constructor to eliminate direct struct field access
- Added `PerPartitionBuffer::deadline()` getter to replace direct field access in BatchAccumulator
- `messages` and `deadline` fields are now private

## Verification

- `cargo check --lib` passes with no errors (79 warnings, pre-existing)
- `cargo clippy --all-targets` passes with no errors
- Old `PartitionAccumulator` name removed from worker_pool/mod.rs
- `PerPartitionBuffer` re-exported from worker_pool/mod.rs
- `BatchAccumulator` re-exported from python/mod.rs

## Self-Check: PASSED

- PerPartitionBuffer struct exists in worker_pool/accumulator.rs
- BatchAccumulator struct exists in python/batch.rs
- Commit 50506bf verified in git history
