---
phase: "28"
plan: "01"
type: "summary"
subsystem: observability
tags: [metrics, prometheus, instrumentation]
dependency_graph:
  requires: []
  provides: [OBS-01, OBS-02, OBS-08, OBS-09, OBS-10]
  affects: [src/worker_pool/mod.rs, src/dispatcher/queue_manager.rs]
tech_stack:
  added: [metrics 0.24.3, prometheus-client 0.24.1]
  patterns: [facade trait, lexicographically-sorted labels, polling-based gauges]
key_files:
  created:
    - src/observability/metrics.rs
    - src/observability/prometheus.rs
    - src/observability/mod.rs
  modified:
    - Cargo.toml
decisions:
  - "MetricsSink sync-only API (record_counter/record_histogram/record_gauge), no async, no describe"
  - "MetricLabels enforces lexicographic sort on key insert to prevent cardinality explosion"
  - "PrometheusMetricsSink uses Arc<RwLock<Registry>> for Send+Sync"
  - "Histogram buckets: [1ms, 5ms, 10ms, 50ms, 100ms, 500ms, 1s, 5s] in seconds"
  - "Prefix: kafpy.handler.* for handler metrics, kafpy.queue.* for queue gauges"
metrics:
  duration: "~3 minutes"
  completed_date: "2026-04-18"
---

# Phase 28 Plan 01: Metrics Infrastructure Foundation Summary

## One-liner

MetricsSink trait, MetricLabels sorted wrapper, HandlerMetrics struct, and PrometheusMetricsSink adapter using metrics 0.24 + prometheus-client 0.24.

## What Was Built

### src/observability/metrics.rs (MetricsSink, MetricLabels, HandlerMetrics, QueueSnapshot)

- **MetricsSink trait** — sync-only interface with `record_counter`, `record_histogram`, `record_gauge`. No async, no describe, no explicit noop. The `metrics` crate facade handles zero-cost noop when no recorder is installed (OBS-01).
- **MetricLabels** — `Vec<(&'static str, String)>` wrapper that sorts lexicographically on every `insert()` call. Prevents cardinality explosion from inconsistent label ordering (OBS-09, Pitfall 5).
- **HandlerMetrics** — static methods for recording `kafpy.handler.invocation`, `kafpy.handler.latency`, `kafpy.handler.error`, `kafpy.handler.batch_size` (OBS-02).
- **QueueSnapshot** — `AtomicUsize` pair for `queue_depth` and `inflight` (OBS-06).

### src/observability/prometheus.rs (PrometheusMetricsSink)

- **PrometheusMetricsSink** implements `MetricsSink` using `prometheus-client 0.24`
- Uses `Arc<RwLock<Registry>>` for thread-safe shared access
- Buckets: `[0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0, 5.0]` seconds (OBS-08, D-05)
- `encode() -> String` produces Prometheus exposition text format
- `register()` exposes all 6 metrics to the registry

### src/observability/mod.rs

- Module re-exports: `MetricsSink`, `MetricLabels`, `HandlerMetrics`, `QueueSnapshot`, `PrometheusMetricsSink`

### Cargo.toml

- Added `metrics = "0.24"`, `prometheus-client = "0.24"`

## Verification

- `cargo check --lib` passed with no errors (31 pre-existing warnings in unrelated files)

## Commit

- `58d6a9f`: `feat(28-01): add observability metrics infrastructure`

## Deviations from Plan

None — plan executed exactly as written.

## Threat Surface

| Flag | File | Description |
|------|------|-------------|
| N/A | metrics.rs | Facade delegation — no new trust boundaries |
| N/A | prometheus.rs | Registry-based metric registration — fixed bucket count prevents unbounded growth |

## Known Stubs

None.
