---
phase: 41-result-output
plan: 02
type: execute
wave: 2
subsystem: benchmark
tags: [benchmark, report, comparison, markdown, diff]
dependency_graph:
  requires: [41-01]
  provides: [output]
  affects: [benchmark]
tech_stack:
  added: []
  patterns: [comparison, report-generation]
key_files:
  created:
    - src/benchmark/output.rs
  modified:
    - src/benchmark/mod.rs
    - Cargo.toml
decisions:
  - Comparison uses configurable threshold (default 5%) for regression detection
  - Markdown reports use named format arguments to avoid positional argument errors
  - Memory delta shown as absolute bytes, not percentage in comparison reports
---

# Phase 41 Plan 02: BenchmarkReport and ComparisonReport - Summary

## One-liner
Human-readable BenchmarkReport and ComparisonReport with baseline comparison, regression detection, and machine-readable diff JSON.

## Completed Tasks

| Task | Name | Commit | Files |
|------|------|--------|-------|
| 1 | Define ComparisonReport struct and implement compare() function | 992f264 | src/benchmark/output.rs |
| 2 | Implement BenchmarkReport as human-readable markdown | 992f264 | src/benchmark/output.rs |
| 3 | Implement machine-readable diff JSON and write functions | 992f264 | src/benchmark/output.rs |
| 4 | Add tests for comparison logic, markdown output, and diff serialization | 992f264 | src/benchmark/output.rs |

## Implementation Details

### BenchmarkReport
- `From<&BenchmarkResult>` conversion extracts key fields
- `to_markdown()` produces formatted markdown table with:
  - Scenario name and timestamp
  - Duration and total messages
  - Key metrics table (throughput, latency P50/P95/P99, error rate, memory delta)

### ComparisonReport
- `compare(base, current, threshold_pct)` computes deltas and detects regressions
- Latency increase > threshold = regression
- Throughput decrease < -threshold = regression
- Error rate increase > threshold = regression

### DiffReport
- `DiffEntry` with metric name, base/current values, delta_pct, absolute_delta, is_regression
- `write_diff_json()` produces `bench-diff-{scenario}-{timestamp}.json`

### Write Functions
- `write_markdown_report()` - `bench-report-{scenario}-{timestamp}.md`
- `write_comparison_markdown()` - `bench-compare-{scenario}-{timestamp}.md`
- `write_diff_json()` - `bench-diff-{scenario}-{timestamp}.json`

## Verification
- `cargo check --lib` passes with no errors
- Unit tests implemented but cannot run due to PyO3 linking in test mode (pre-existing infrastructure issue)

## Requirements Covered
- OUT-03: Human-readable BenchmarkReport in markdown format
- OUT-04: ComparisonReport with delta % and regression highlighting
- OUT-06: Machine-readable diff JSON output

## Key Metrics
- Delta computation: `(current - base) / base * 100.0`
- Default regression threshold: 5.0%
- Memory delta shown as absolute bytes (not percentage)

## Combined with Plan 01
Both plans 41-01 and 41-02 were implemented together since they share the same output.rs file. The commit 992f264 contains the complete implementation for both plans.