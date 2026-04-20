# Phase 38: Result Models & Measurement Infrastructure - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-04-20
**Phase:** 38-result-models
**Areas discussed:** Measurement sampling architecture, Percentile computation, Memory tracking, CSV output structure

---

## Measurement Sampling Architecture

| Option | Description | Selected |
|--------|-------------|----------|
| Async sampling with background aggregation | Per-message async channel sends samples to background aggregator task — zero hot-path overhead | ✓ |
| Batch-at-end (simpler, less memory-safe) | All samples queued in memory, processed at end of run — simpler but unbounded memory growth | |

**User's choice:** Async sampling with background aggregation (Recommended)
**Notes:** Already proven in MetricsSink pattern

---

## Percentile Computation

| Option | Description | Selected |
|--------|-------------|----------|
| t-digest algorithm | Dedup t-digest and tDigest (criterion uses t-digest) — accurate at high cardinality with low memory | ✓ |
| Exact sorted percentiles | Store all samples in Vec, sort for exact percentiles — O(n log n), memory grows with sample count | |
| Both (exact for small N, t-digest for large N) | Hybrid approach | |

**User's choice:** t-digest algorithm (Recommended)
**Notes:** Best for high-volume benchmarks

---

## Memory Tracking

| Option | Description | Selected |
|--------|-------------|----------|
| RuntimeSnapshot heap delta | Lightweight per-snapshot: heap_allocated delta only — low overhead, sufficient for hardening checks | ✓ |
| dhat allocation profiling | Detailed allocation tracking with dhat_alloc: identifies where allocations come from — heavier but more actionable | |

**User's choice:** RuntimeSnapshot heap delta (Recommended)
**Notes:** Already available via existing RuntimeSnapshot

---

## CSV Output Structure

| Option | Description | Selected |
|--------|-------------|----------|
| Interval summary rows | One row per measurement interval with: timestamp_ms, msg_count, bytes_count, latency_p50_ms, latency_p95_ms, latency_p99_ms, error_rate | ✓ |
| Per-message rows | One row per individual message with timestamp, latency_ns, handler, outcome — large files but raw detail | |

**User's choice:** Interval summary rows (Recommended)
**Notes:** Avoids high-cardinality file bloat

---

## Claude's Discretion

- JSON output file naming convention — standard approach
- Output path default — standard convention
- T-digest compression factor — use crate defaults

## Deferred Ideas

None — discussion stayed within phase scope.
