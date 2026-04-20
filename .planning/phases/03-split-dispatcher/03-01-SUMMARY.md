---
phase: "03"
plan: "01"
type: execute
subsystem: dispatcher + runtime
tags: [refactor, dispatcher, runtime, pyconsumer, split]
dependency_graph:
  requires: []
  provides:
    - src/dispatcher/consumer_dispatcher.rs
    - src/runtime/mod.rs
    - src/runtime/builder.rs
  affects:
    - src/dispatcher/mod.rs
    - src/pyconsumer.rs
    - src/lib.rs
    - src/consumer/error.rs
tech_stack:
  added:
    - src/runtime/mod.rs
    - src/runtime/builder.rs
  patterns:
    - Builder pattern (RuntimeBuilder)
    - Module extraction (ConsumerDispatcher)
key_files:
  created:
    - src/dispatcher/consumer_dispatcher.rs (340 lines)
    - src/runtime/mod.rs (10 lines)
    - src/runtime/builder.rs (225 lines)
  modified:
    - src/dispatcher/mod.rs (now 243 lines, -409)
    - src/pyconsumer.rs (now 207 lines, -141)
    - src/lib.rs (+1 module)
    - src/consumer/error.rs (+BuildError variant)
decisions:
  - id: SPLIT-B-04
    description: "Assembly order invariant: config → runner → offset_tracker → dispatcher → receivers → handler_arc → executor → queue_manager → retry_coordinator → dlq_producer → dlq_router → coordinator → pool → snapshot_task → committer → dispatcher_handle → pool.run()"
  - id: SPLIT-B-05
    description: "RuntimeBuilder lives in pub(crate) module — pure Rust, no PyO3, testable without Python"
  - id: SPLIT-B-06
    description: "PyO3 boundary unchanged — Consumer pyclass still wraps RuntimeBuilder"
metrics:
  duration: "~5 min"
  completed_date: "2026-04-20"
  tasks_completed: 3
---

# Phase 03 Plan 01 Summary: Split dispatcher/ + Extract runtime/

**One-liner:** Extracted `ConsumerDispatcher` into `dispatcher/consumer_dispatcher.rs` and created `runtime/` module with `RuntimeBuilder` for runtime assembly.

## What Was Done

### Task 1: Extract ConsumerDispatcher
Split `ConsumerDispatcher` (340 lines) from `dispatcher/mod.rs` into its own file `src/dispatcher/consumer_dispatcher.rs` including:
- Full struct definition, impl block, and routing methods
- Tests module (9 tests) and `OwnedMessage::fake()` test helper
- `pub use consumer_dispatcher::ConsumerDispatcher` re-export added to `dispatcher/mod.rs`

`dispatcher/mod.rs` now 243 lines (down from 652 lines).

### Task 2: Create runtime/ module
Created `src/runtime/mod.rs` (10 lines) and `src/runtime/builder.rs` (225 lines) with:
- `RuntimeBuilder::new(config, handlers, shutdown_token)` constructor
- `RuntimeBuilder::build()` async method that assembles full runtime in fixed order (see assembly order invariant)
- `Runtime` struct with `pool`, `dispatcher_handle`, `committer_handle`, `coordinator`
- `Runtime::run()` that awaits pool, dispatcher, and committer

Added `From<BuildError>` for `ConsumerError` in `src/consumer/error.rs` to support `?` error propagation.

### Task 3: Wire pyconsumer.rs to use RuntimeBuilder
Refactored `pyconsumer.rs` (now 207 lines, down from 348 lines):
- `start()` replaced with `RuntimeBuilder::new() → build() → run()` (14 lines)
- Removed all 140+ lines of duplicated assembly logic
- Removed unused imports
- PyO3 boundary unchanged: `Consumer` pyclass interface identical

## Deviations from Plan

**None** — plan executed exactly as written.

## Deviations Auto-Fixed (Rule 1/2/3)

**1. [Rule 2 - Missing impl] Added `From<BuildError>` for `ConsumerError`**
- `RuntimeBuilder::build()` uses `?` on `build()` which returns `Result<_, BuildError>`
- Without this, `ConsumerError` would not implement `From<BuildError>` and `?` would fail
- Fix: Added `Config(#[from] BuildError)` variant and import to `consumer/error.rs`

**2. [Rule 1 - Type mismatch] Fixed `Arc<pyo3::PyAny>` vs `Arc<pyo3::Py<pyo3::PyAny>>`**
- Struct field had wrong type; constructor parameter had correct type
- Made both consistent (`Arc<Mutex<HashMap<String, Arc<pyo3::Py<pyo3::PyAny>>>>>`)

**3. [Rule 1 - Dead imports] Removed unused imports from dispatcher/mod.rs**
- `MessageTimestamp`, `KafpySpanExt` left behind after ConsumerDispatcher split
- Removed to keep module clean

## Verification

| Check | Result |
|-------|--------|
| `cargo build --lib` | PASS (100 warnings pre-existing) |
| `cargo check --lib` | PASS |
| My files pass clippy | PASS (no errors in dispatcher/, runtime/, pyconsumer.rs, lib.rs, consumer/error.rs) |
| Pre-existing clippy issues in other modules | 101 warnings (not fixed — out of scope) |

## Commits

| Hash | Task | Description |
|------|------|-------------|
| `b59802b` | Task 1 | Extract ConsumerDispatcher to its own file |
| `eedf376` | Task 2 | Add runtime/ module with RuntimeBuilder |
| `2ad7e44` | Task 3 | Make pyconsumer.rs thin wrapper using RuntimeBuilder |
| `37d6ec5` | Cleanup | Remove dead imports from dispatcher/mod.rs |

## Known Stubs

**None.**

## Threat Flags

**None.** No new security surface introduced.

## TDD Gate Compliance

Not a TDD plan — refactoring only. No behavior changes.
