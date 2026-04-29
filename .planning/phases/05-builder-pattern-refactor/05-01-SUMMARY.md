---
phase: "05-builder-pattern-refactor"
plan: "01"
subsystem: "config"
tags:
  - builder-pattern
  - pyclass
  - pyo3
  - rust
dependency_graph:
  requires: []
  provides:
    - src/config.rs: ProducerConfigBuilder pyclass
    - src/config.rs: ConsumerConfigBuilder pyclass
    - src/config.rs: BuildError enum
  affects: []
tech_stack:
  added:
    - pyo3 0.27.2 (already in use)
    - std::sync::RwLock (already in use)
  patterns:
    - Fluent builder pattern with interior mutability
    - pyclass with Sync bounds via RwLock
key_files:
  created: []
  modified:
    - path: "src/config.rs"
      lines_added: 451
      description: "Added BuildError enum, ProducerConfigBuilder, and ConsumerConfigBuilder pyclass implementations"
decisions:
  - id: "RWLock-over-RefCell"
    rationale: "pyo3 0.27 #[pyclass] requires Sync bound; RefCell is not Sync, so RwLock is required for interior mutability"
  - id: "manual-Clone-impl"
    rationale: "RwLock doesn't derive Clone, so manual Clone impl using helper function to clone inner values"
  - id: "build-uses-mut-self"
    rationale: "Cannot move self out of Python interpreter; build(&mut self) borrows and extracts values via RwLock"
  - id: "String-over-impl-Into"
    rationale: "Python functions cannot have impl Trait arguments; concrete String type is required"
must_haves:
  truths:
    - "Python users can configure ConsumerConfig via a fluent builder instead of 24 positional args"
    - "Python users can configure ProducerConfig via a fluent builder instead of 17 positional args"
    - "Builder.build() returns an error with clear message for missing required fields"
    - "All #[allow] suppressions in src/config.rs are audited with explanations"
  artifacts:
    - path: "src/config.rs"
      provides: "ConsumerConfigBuilder and ProducerConfigBuilder pyclass implementations"
      min_lines: 250
    - path: "src/config.rs"
      provides: "BuildError enum with missing-field and no-topics variants"
      contains: "pub enum BuildError"
metrics:
  duration: "~15 minutes"
  completed_date: "2026-04-29"
---

# Phase 05 Plan 01 Summary

## One-liner

Added fluent builder patterns for Python-facing `ConsumerConfig` and `ProducerConfig` in `src/config.rs` using pyo3 pyclass with RwLock interior mutability.

## What Was Done

### Tasks Completed

**Task 1 + 2 + 3: Added BuildError, ProducerConfigBuilder, and ConsumerConfigBuilder to src/config.rs**

- Added `BuildError` enum with `MissingField(&'static str)` and `NoTopics` variants
- Added `ProducerConfigBuilder` pyclass with all 17 fields from `ProducerConfig`
  - Uses `RwLock<Option<T>>` for optional fields to satisfy Sync bounds
  - `#[new]` constructor with `#[pyo3(signature = ())]` for zero-argument initialization
  - Builder methods for each field: `brokers()`, `message_timeout_ms()`, etc.
  - `build(&mut self) -> PyResult<ProducerConfig>` with validation
- Added `ConsumerConfigBuilder` pyclass with all 24 fields from `ConsumerConfig`
  - Same RwLock pattern for interior mutability
  - Builder methods for all fields including `topics()`, `add_topic()`
  - `build(&mut self) -> PyResult<ConsumerConfig>` validates brokers, group_id, and topics

**Task 4: Audited #[allow] suppressions in src/config.rs**

Added explanatory comments to both `#[allow(clippy::too_many_arguments)]` attributes:
- ConsumerConfig `#[new]` (line 69): "Kept for backward compatibility — Python callers using positional args still work. The builder pattern is the preferred API..."
- ProducerConfig `#[new]` (line 236): Same comment

### Key Technical Decisions

1. **RwLock over RefCell**: pyo3 0.27 #[pyclass] requires types to implement `Sync`. `RefCell` does not implement `Sync`, so `RwLock` was used for interior mutability on optional/collection fields.

2. **Manual Clone impl**: Since `RwLock` doesn't derive `Clone`, manual `impl Clone` was added for both builders using helper functions that clone the inner values.

3. **build(&mut self)**: Cannot use `self` move out of Python interpreter in pyo3 0.27. The `build()` method uses `&mut self` and extracts values via `RwLock::read()`.

4. **Concrete String over impl Into<String>**: Python functions cannot have `impl Trait` arguments, so all builder methods use concrete `String` type.

## Verification

```bash
cargo check --lib 2>&1 | tail -5
# Output: Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.06s

grep -n "ConsumerConfigBuilder\|ProducerConfigBuilder" src/config.rs | wc -l
# Output: 14
```

## Deviations from Plan

**Rule 3 - Auto-fix blocking issue**: The original builder implementation used `impl Into<String>` and `self` move pattern which pyo3 0.27 does not support. This required switching from RefCell to RwLock (for Sync), adding manual Clone impl, and using `&mut self` for all builder methods.

**Rule 2 - Auto-add missing functionality**: Added `#[pyo3(signature = ())]` to builder constructors which was not in the original plan but required for zero-arg `#[new]` methods in pyo3.

## Known Stubs

None.

## Threat Flags

None — added only Python-facing builder structs and methods, no changes to trust boundaries.

---

## Commit

- `7f7b304`: feat(05-builder-pattern-refactor): add ConsumerConfigBuilder and ProducerConfigBuilder pyclass to src/config.rs
