---
phase: 17-failure-classification
plan: 02
status: complete
wave: 2
started: 2026-04-17
completed: 2026-04-17
summary: "Wired FailureReason into ExecutionResult variants, OffsetCoordinator::mark_failed, and worker_loop structured logging. Build passes; test linking issue is pre-existing PyO3 extension-module config."
key_files_created:
  - src/failure/logging.rs
key_files_modified:
  - src/python/execution_result.rs
  - src/coordinator/offset_coordinator.rs
  - src/coordinator/offset_tracker.rs
  - src/worker_pool/mod.rs
  - src/python/executor.rs
  - src/python/handler.rs
  - src/failure/mod.rs
  - src/failure/tests.rs
verification:
  cargo_build: passed
  cargo_test: linking error (pre-existing PyO3 extension-module config, not from this plan)
---

## Plan 17-02 Summary: Wire FailureReason into ExecutionResult + WorkerPool

**Objective:** Update ExecutionResult to carry FailureReason, update OffsetCoordinator::mark_failed signature, wire structured logging into worker loop

### Tasks Completed

**Task 1: Update ExecutionResult, OffsetCoordinator trait, and add logging**

- Created `src/failure/logging.rs` with `log_failure(&ExecutionContext, &FailureReason, &str, bool)` function that dispatches to warn/error/info based on `FailureCategory`
- Updated `ExecutionResult` in `src/python/execution_result.rs` to carry `FailureReason` in `Error` and `Rejected` variants
- Updated `OffsetCoordinator` trait in `src/coordinator/offset_coordinator.rs` to add `reason: &FailureReason` parameter to `mark_failed`
- Updated `OffsetTracker::mark_failed` implementation in `src/coordinator/offset_tracker.rs` to accept and store `reason`
- Added `last_failure_reason: Option<FailureReason>` field to `PartitionState` struct
- Added `pub mod logging;` to `src/failure/mod.rs`

**Task 2: Update WorkerPool worker_loop to call log_failure and pass reason to mark_failed**

- Updated `worker_loop` Error arm to extract `FailureReason`, call `log_failure`, and pass `&reason` to `mark_failed`
- Updated `worker_loop` Rejected arm similarly
- Updated `DefaultExecutor::execute` in `src/python/executor.rs` to use new ExecutionResult field patterns
- Updated `PythonHandler::invoke` in `src/python/handler.rs` to classify Python exceptions using `DefaultFailureClassifier` and populate `FailureReason::Error` variant
- Updated test in `src/worker_pool/mod.rs` to use `FailureReason::Terminal(TerminalKind::HandlerPanic)`
- Updated test imports in `src/failure/tests.rs` to use explicit `crate::failure::*` imports instead of `use super::*`

### Key Decisions

- `ExecutionResult::Error { reason, exception, traceback }` — named struct fields for clarity over tuple variants
- `ExecutionResult::Rejected { reason, reason_str }` — uses `reason_str` for rejection message, not exception/traceback
- Panic case in `PythonHandler::invoke` uses `FailureReason::Terminal(TerminalKind::HandlerPanic)`
- `ExecutionContext` is cloned inside `spawn_blocking` closure to avoid lifetime issues with the classifier

### Verification

- `cargo build --lib` passed with warnings only
- `cargo test --lib` fails at linking stage due to PyO3 extension-module needing Python interpreter — this is a pre-existing configuration issue unrelated to plan 17-02 changes

### Notes

- PyO3 0.27 API: `error.get_type().name()` used for exception type extraction
- `DefaultFailureClassifier` instantiated inside `spawn_blocking` closure for each error classification
- Test linking error is pre-existing: `crate-type = ["cdylib", "rlib"]` with `extension-module` feature causes the test binary to fail linking against Python symbols. This is not caused by plan 17-02 changes.