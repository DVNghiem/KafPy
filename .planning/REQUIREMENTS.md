# Requirements: KafPy v1.9 — Benchmark & Hardening

**Milestone:** v1.9
**Goal:** Production hardening plus credible benchmark infrastructure and reports — measurable, reproducible performance characterization across handler modes, retry scenarios, and workload profiles.

---

## Scenario Definitions (SCEN-##)

- [ ] **SCEN-01**: `Scenario` trait defines WHAT to benchmark: target topic, message rate, payload size, duration, warmup messages.
- [ ] **SCEN-02**: `WorkloadProfile` enum variants: `ThroughputFocused`, `LatencyFocused`, `FailureFocused`, `BatchComparison`, `HandlerModeComparison`.
- [ ] **SCEN-03**: `ThroughputScenario`: configurable messages_per_second, payload_bytes, num_messages or duration.
- [ ] **SCEN-04**: `LatencyScenario`: single-message latency measurement under steady-state load.
- [ ] **SCEN-05**: `FailureScenario`: configurable failure rate (%), retry behavior, DLQ routing exercised.
- [ ] **SCEN-06**: `BatchVsSyncScenario`: compares `BatchSync` vs `SingleSync` handler modes under identical workload.
- [ ] **SCEN-07**: `AsyncVsSyncScenario`: compares `SingleAsync` vs `SingleSync` under identical workload.
- [ ] **SCEN-08**: All scenarios are configurable via Python dict or TOML file; new scenarios addable without touching runner core.

## Result Models (RES-##)

- [ ] **RES-01**: `BenchmarkResult` struct captures per-run measurements: total_messages, duration_ms, throughput_msg_s, latency_p50_ms, latency_p95_ms, latency_p99_ms, error_rate, memory_delta_bytes.
- [ ] **RES-02**: `AggregatedResult` for multi-run aggregation: mean, stddev, min, max across runs.
- [ ] **RES-03**: `PercentileBuckets` struct with configurable percentiles (default: p50, p95, p99, p999).
- [ ] **RES-04**: `ScenarioConfig` echoes the scenario parameters back in the result for reproducibility.
- [ ] **RES-05**: All result types implement `Serialize`/`Deserialize` via `serde` for JSON output.
- [ ] **RES-06**: `CsvSerializable` trait for tabular output; `BenchmarkResult` and `AggregatedResult` both implement it.

## Measurement Infrastructure (MEAS-##)

- [ ] **MEAS-01**: `MeasurementStats` struct: counter, sum, sum_squared, min, max, count — for computing mean/variance/percentiles online.
- [ ] **MEAS-02**: `HistogramRecorder` using t-digest algorithm for accurate high-percentile computation at scale.
- [ ] **MEAS-03**: `LatencyTimer` scoped timer using `Instant` — records nanosecond precision latency samples.
- [ ] **MEAS-04**: `ThroughputMeter` — tracks messages/bytes count and elapsed time; computes rate on demand.
- [ ] **MEAS-05**: `MemorySnapshot` using `RuntimeSnapshot` — heap_allocated delta between two snapshots.
- [ ] **MEAS-06**: All measurement code is off the hot path — aggregated via background task, not per-message.
- [ ] **MEAS-07**: Warmup phase: first N messages (configurable, default 1000) are excluded from latency/throughput metrics.
- [ ] **MEAS-08**: `MetricLabels` from existing observability subsystem reused for all benchmark metric labels.

## Benchmark Runner (RUN-##)

- [ ] **RUN-01**: `BenchmarkRunner` orchestrates: scenario setup, warmup, measurement window, teardown.
- [ ] **RUN-02**: `run_scenario(scenario: Scenario) -> BenchmarkResult` is the primary entry point.
- [ ] **RUN-03**: Runner accepts a `MetricsSink` (existing interface) for metric emission; benchmark has its own sink for self-measurement.
- [ ] **RUN-04**: `BenchmarkContext` passed to scenario: provides message generator, config access, result writer.
- [ ] **RUN-05**: Graceful termination: runner drains inflight messages and commits offsets before shutting down.
- [ ] **RUN-06**: `BenchmarkRunner` is `Send + Sync` so it can be used from Python async context via PyO3.
- [ ] **RUN-07**: Python CLI entry point: `python -m kafpy.benchmark run --scenario throughput --output ./results`.

## Result Output (OUT-##)

- [ ] **OUT-01**: JSON output: one file per run, named `bench-{scenario}-{timestamp}.json`.
- [ ] **OUT-02**: CSV output: `bench-{scenario}-{timestamp}.csv` with one row per measurement interval.
- [ ] **OUT-03**: `BenchmarkReport` human-readable summary: Markdown table with scenario name, key metrics, comparison vs baseline (if baseline file present).
- [ ] **OUT-04**: `compare(base: &BenchmarkResult, current: &BenchmarkResult) -> ComparisonReport` — shows delta %, highlights regressions.
- [ ] **OUT-05**: Report output path configurable; defaults to `./benchmark_results/`.
- [ ] **OUT-06**: Machine-readable diff output: `bench-diff-{scenario}-{timestamp}.json` with before/after values and delta.

## Hardening Checks (HARD-##)

- [ ] **HARD-01**: `HardeningCheck` enum with variants: `BackpressureThreshold`, `MemoryLeakCheck`, `GracefulShutdownCheck`, `DlqDrainCheck`, `RetryBudgetCheck`.
- [ ] **HARD-02**: `ValidationResult` struct: check name, passed (bool), details (string), suggestions (Vec<String>).
- [ ] **HARD-03**: `HardeningRunner::run_all() -> Vec<ValidationResult>` — runs all checks and returns results.
- [ ] **HARD-04**: Backpressure threshold validation: consumer honors queue_depth limits without message loss under saturated load.
- [ ] **HARD-05**: Memory leak check: no significant heap growth (> 1MB delta) over sustained 10M message run.
- [ ] **HARD-06**: Graceful shutdown check: pending messages processed and offsets committed before exit on SIGINT.
- [ ] **HARD-07**: DLQ drain check: all failed-messages delivered to DLQ topic after graceful shutdown.
- [ ] **HARD-08**: Retry budget check: messages exhaust retry budget and route to DLQ (not infinite retry loop).

## Python API Surface (PY-##)

- [ ] **PY-01**: `kafpy.benchmark` module exposes: `run_scenario`, `BenchmarkResult`, `ScenarioConfig`, `BenchmarkReport`, `run_hardening_checks`.
- [ ] **PY-02**: `ScenarioConfig` dataclass (Python): `scenario_type`, `num_messages`, `payload_bytes`, `rate`, `warmup_messages`, `failure_rate`.
- [ ] **PY-03**: `BenchmarkReport` dataclass (Python): scenario name, metrics dict, passed checks, suggestions list.
- [ ] **PY-04**: All benchmark result types frozen after construction; no mutation after result is written.
- [ ] **PY-05**: `kafpy.benchmark` has its own `__all__` listing only intended public API.

## Benchmark Methodology Notes (NOTE-##)

- [ ] **NOTE-01**: `BENCHMARK-METHODOLOGY.md` at repo root explaining: what is measured, how P50/P95/P99 are computed, assumptions and limitations.
- [ ] **NOTE-02**: Tuning checklist: `TUNING.md` with practical guidance on queue_depth, concurrent_handlers, batch_size, timeout settings based on benchmark observations.
- [ ] **NOTE-03**: Methodology doc covers: warmup exclusion, confidence intervals (when N runs > 1), reproducibility requirements (isolated Kafka, fixed partition count).
- [ ] **NOTE-04**: Assumptions documented: Kafka broker under test, network latency contribution, payload size impact on results.

---

## Future Requirements (Deferred)

- Cross-partition aggregation benchmarks (v1.10+)
- Sliding window latency percentiles (p50/p95/p99/p999 in real-time) (v1.10+)
- CI regression detection with baseline comparison (v1.10+)
- Alerting rules library export from benchmark data (v1.10+)

## Out of Scope

- Embedded Kafka (testcontainers) provisioning — benchmark assumes existing Kafka cluster
- Schema registry benchmarks (Avro/Protobuf) — deferred to schema registry support milestone
- Multi-cluster federation benchmarks — single cluster only
- Custom metric exporters beyond Prometheus-compatible (JSON/CSV only for v1.9)

---

## Traceability

| REQ-ID | Phase | Description |
|--------|-------|-------------|
| SCEN-01 | TBD | |
| SCEN-02 | TBD | |
| SCEN-03 | TBD | |
| SCEN-04 | TBD | |
| SCEN-05 | TBD | |
| SCEN-06 | TBD | |
| SCEN-07 | TBD | |
| SCEN-08 | TBD | |
| RES-01 | TBD | |
| RES-02 | TBD | |
| RES-03 | TBD | |
| RES-04 | TBD | |
| RES-05 | TBD | |
| RES-06 | TBD | |
| MEAS-01 | TBD | |
| MEAS-02 | TBD | |
| MEAS-03 | TBD | |
| MEAS-04 | TBD | |
| MEAS-05 | TBD | |
| MEAS-06 | TBD | |
| MEAS-07 | TBD | |
| MEAS-08 | TBD | |
| RUN-01 | TBD | |
| RUN-02 | TBD | |
| RUN-03 | TBD | |
| RUN-04 | TBD | |
| RUN-05 | TBD | |
| RUN-06 | TBD | |
| RUN-07 | TBD | |
| OUT-01 | TBD | |
| OUT-02 | TBD | |
| OUT-03 | TBD | |
| OUT-04 | TBD | |
| OUT-05 | TBD | |
| OUT-06 | TBD | |
| HARD-01 | TBD | |
| HARD-02 | TBD | |
| HARD-03 | TBD | |
| HARD-04 | TBD | |
| HARD-05 | TBD | |
| HARD-06 | TBD | |
| HARD-07 | TBD | |
| HARD-08 | TBD | |
| PY-01 | TBD | |
| PY-02 | TBD | |
| PY-03 | TBD | |
| PY-04 | TBD | |
| PY-05 | TBD | |
| NOTE-01 | TBD | |
| NOTE-02 | TBD | |
| NOTE-03 | TBD | |
| NOTE-04 | TBD | |

---

*Generated: 2026-04-20 — v1.9 Benchmark & Hardening*
