# Phase 38: Result Models & Measurement Infrastructure - Context

**Gathered:** 2026-04-20
**Status:** Ready for planning

<domain>
## Phase Boundary

Data models for benchmark results and measurement helpers — collecting latency/throughput/memory samples off the hot path. Pure data structures, no production dependencies.
</domain>

<decisions>
## Implementation Decisions

### Measurement Sampling Architecture
- **D-01:** Async sampling with background aggregation — per-message async channel sends samples to background aggregator task, zero hot-path overhead
- **D-02:** Samples aggregated off hot path via dedicated Tokio task — not processed until measurement window closes

### Percentile Computation
- **D-03:** t-digest algorithm for accurate high-percentile computation (MEAS-02) — `dedup_tdigest` or `tdigest` crate
- **D-04:** Accurate at high cardinality with bounded memory — not O(n log n) like exact sorted percentiles

### Memory Tracking
- **D-05:** RuntimeSnapshot heap_allocated delta only — lightweight, sufficient for hardening checks (HARD-05)
- **D-06:** No dhat allocation profiling in benchmark core — too heavy for routine benchmark runs

### CSV Output Structure
- **D-07:** Interval summary rows — one row per measurement interval with: timestamp_ms, msg_count, bytes_count, latency_p50_ms, latency_p95_ms, latency_p99_ms, error_rate
- **D-08:** NOT per-message rows — interval summaries avoid high-cardinality file bloat

### Measurement Infrastructure
- **D-09:** LatencyTimer uses `Instant` for nanosecond-precision scoped timing (MEAS-03)
- **D-10:** ThroughputMeter tracks messages/bytes count, computes rate on demand (MEAS-04)
- **D-11:** Warmup phase: first N messages (default 1000) excluded from latency/throughput metrics (MEAS-07)
- **D-12:** MetricLabels from observability subsystem reused for all benchmark metric labels (MEAS-08)

### Result Types
- **D-13:** BenchmarkResult struct: total_messages, duration_ms, throughput_msg_s, latency_p50/p95/p99_ms, error_rate, memory_delta_bytes (RES-01)
- **D-14:** AggregatedResult holds mean/stddev/min/max across multiple runs (RES-02)
- **D-15:** All result types implement Serialize/Deserialize via serde (RES-05)
- **D-16:** BenchmarkResult and AggregatedResult implement CsvSerializable (RES-06)
- **D-17:** PercentileBuckets struct with configurable percentiles (default: p50, p95, p99, p999) (RES-03)
- **D-18:** ScenarioConfig echoed in result for reproducibility (RES-04)

### Claude's Discretion
- JSON output file naming convention (`bench-{scenario}-{timestamp}.json`) — standard approach, no user preference specified
- Output path default (`./benchmark_results/`) — standard convention
- T-digest compression factor / centroid limit — use crate defaults unless profiling shows issues
</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Project
- `.planning/PROJECT.md` — Project context, core value, existing architecture
- `.planning/REQUIREMENTS.md` — v1.9 requirements (RES-01 through RES-06, MEAS-01 through MEAS-08)
- `.planning/milestones/v1.9-ROADMAP.md` — Phase 38 goal, success criteria
- `.planning/research/SUMMARY.md` — Benchmark architecture, module structure

### Observability (reuse patterns)
- `src/observability/metrics.rs` — MetricsSink trait, zero-cost facade pattern
- `src/observability/runtime.rs` — RuntimeSnapshot for memory tracking
- `src/observability/mod.rs` — Module structure for MetricLabels

### Prior Phases (patterns to follow)
- `src/failure/retry.rs` — RetryCoordinator as a good example of a data-driven struct with clear fields
- `src/dlq/mod.rs` — DlqMetadata as a good example of a metadata envelope struct
</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `MetricsSink` trait — already handles off-path aggregation, reuse for benchmark self-measurement
- `RuntimeSnapshot` — already tracks heap_allocated, can be used directly for memory_delta_bytes
- `MetricLabels` — already enforces lexicographically sorted labels, benchmark should reuse it

### Established Patterns
- Structs with clear field names and derive(Serialize, Deserialize) — follow existing pattern
- Tokio channel-based async aggregation — already used in dispatcher and worker pool
- `pub(crate)` module visibility for internals — benchmark infrastructure should be `pub(crate)` not exposed to Python

### Integration Points
- Phase 39 (Scenario) consumes MeasurementStats and HistogramRecorder from Phase 38
- Phase 40 (Runner) uses LatencyTimer, ThroughputMeter, MemorySnapshot
- Phase 41 (Output) serializes BenchmarkResult to JSON/CSV
- Phase 42 (Hardening) uses AggregatedResult for memory leak check
</code_context>

<specifics>
## Specific Ideas

- Use `dedup_tdigest` crate (not `tdigest` directly) — dedup variant handles duplicate samples better for re-run scenarios
- Background aggregation task should be spawned with `tokio::spawn` and dropped on benchmark end
- Warmup count configurable via ScenarioConfig, default 1000
</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 38-result-models*
*Context gathered: 2026-04-20*
