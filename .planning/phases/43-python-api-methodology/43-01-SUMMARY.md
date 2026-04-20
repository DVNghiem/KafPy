---
phase: 43-python-api-methodology
plan: '01'
subsystem: api
tags: [python, py03, benchmark, frozen-dataclass, public-api]

# Dependency graph
requires:
  - phase: 37
    provides: "Python exception hierarchy, KafkaMessage field access, PyO3 module foundation"
provides:
  - kafpy.benchmark public API module with frozen dataclasses
  - PyO3 bindings run_scenario_py and run_hardening_checks_py
  - run_scenario() and run_hardening_checks() public functions
  - HardeningCheck enum-like class
affects:
  - phases building on benchmark infrastructure (44+)
  - any phase consuming BenchmarkResult type

# Tech tracking
tech-stack:
  added: []
  patterns:
    - Frozen Python dataclasses (@dataclass(frozen=True)) for immutable result types
    - PyO3 #[pyfunction] wrap_pyfunction! macro for Python-callable Rust functions
    - JSON serialization roundtrip between Rust (serde_json) and Python (json.loads/dumps)

key-files:
  created:
    - kafpy/benchmark.py (rewritten as public API)
  modified:
    - src/lib.rs (added PyO3 benchmark bindings)
    - kafpy/__init__.py (re-export BenchmarkResult)

key-decisions:
  - "Used wrap_pyfunction! macro (not wrap_pyfunction_binding!) since pyo3 0.27.2 uses this naming"
  - "Return JSON string from PyO3 functions rather than Py<PyDict> for simpler Python side deserialization"
  - "HardeningCheck implemented as simple class with class attributes (not enum) for minimal dependency"
  - "CLI run() and main() functions kept in benchmark.py but excluded from __all__ to maintain clear public API boundary"

patterns-established:
  - "Public API module pattern: frozen dataclasses + public functions + __all__ for clear surface"
  - "Rust extension entry point via try/except ImportError with None fallback for graceful missing-extension handling"

requirements-completed: [PY-01, PY-02, PY-03, PY-04, PY-05]

# Metrics
duration: 8min
completed: 2026-04-20
---

# Phase 43-01: Benchmark Public API Summary

**kafpy.benchmark public API module with frozen dataclasses, PyO3 bindings for run_scenario and run_hardening_checks**

## Performance

- **Duration:** 8 min
- **Started:** 2026-04-20T00:00:00Z
- **Completed:** 2026-04-20T00:08:00Z
- **Tasks:** 3
- **Files modified:** 3

## Accomplishments
- Implemented PyO3 `#[pyfunction]` bindings `run_scenario_py` and `run_hardening_checks_py` in `src/lib.rs`
- Rewrote `kafpy/benchmark.py` as public API with frozen dataclasses: `ScenarioConfig`, `BenchmarkResult`, `BenchmarkReport`, `ValidationResult`
- Added `run_scenario()` and `run_hardening_checks()` public functions with `HardeningCheck` enum-like class
- Defined `__all__` with 7 public API items
- Re-exported `BenchmarkResult` from `kafpy/__init__.py`
- CLI entry point retained via `python -m kafpy.benchmark run`

## Task Commits

Each task was committed atomically:

1. **Task 1: Add PyO3 bindings in src/lib.rs** - `14dc3dd` (feat)
2. **Task 2: Rewrite kafpy/benchmark.py as public API module** - `14dc3dd` (feat)
3. **Task 3: Update kafpy/__init__.py** - `14dc3dd` (feat)

**Plan metadata:** `14dc3dd` (feat: complete plan)

## Files Created/Modified

- `src/lib.rs` - Added run_scenario_py and run_hardening_checks_py PyO3 #[pyfunction] bindings with tokio runtime for async benchmark execution
- `kafpy/benchmark.py` - Rewritten as public API module with frozen dataclasses, public functions, HardeningCheck class, and CLI entry point
- `kafpy/__init__.py` - Added BenchmarkResult re-export and run_scenario_py/run_hardening_checks_py to __all__

## Decisions Made

- Used `wrap_pyfunction!` macro (not `wrap_pyfunction_binding!`) for PyO3 0.27.2 compatibility
- Return JSON string from PyO3 functions rather than Py<PyDict> for simpler Python side deserialization
- HardeningCheck implemented as simple class with class attributes (not enum) for minimal dependency
- CLI run() and main() functions kept in benchmark.py but excluded from __all__ to maintain clear public API boundary

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## Next Phase Readiness

- kafpy.benchmark module is fully implemented with all 7 public API items
- Frozen dataclasses ensure immutability (PY-04 requirement met)
- PyO3 bindings compile and are callable from Python
- BenchmarkResult re-exported from top-level kafpy module

---
*Phase: 43-python-api-methodology*
*Completed: 2026-04-20*