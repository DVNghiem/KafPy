# Benchmark & Hardening Research Summary

**Milestone:** v1.9 Benchmark & Hardening
**Researched:** 2026-04-20
**Confidence:** MEDIUM-HIGH

## Executive Summary

The benchmark and hardening system integrates with existing KafPy infrastructure through four well-defined interfaces: `MetricsSink` (zero-cost metrics collection), `RuntimeSnapshot` (memory/latency state sampling), `ConsumerRunner` (message source), and `RetryCoordinator`/`DlqRouter` (failure injection). New code lives in `src/benchmark/` as `pub(crate)` internals, invisible to the Python API. The critical risk is measurement overhead contaminating the hot path -- all instrumentation must route through the existing `MetricsSink` facade and aggregate off the critical path. The second critical risk is conflating PyO3 binding overhead with Rust core performance; Rust-native benchmark mode is required to measure the actual `ConsumerRunner` throughput.

## Stack Additions

| Technology | Purpose | Integration |
|------------|---------|-------------|
| `criterion` | Rust-native microbenchmark framework with statistical validation | For measuring Rust core throughput (ConsumerRunner, Dispatcher) without PyO3 boundary |
| `serde` + `serde_json` | Benchmark result serialization | `src/benchmark/results.rs` -- serialize BenchmarkResult to JSON/CSV |
| `prometheus-client` | Already available via existing metrics infrastructure | Reuse for benchmark Prometheus output |
| `dhat` | Heap memory allocation profiling | Optional -- detect allocation-driven performance cliffs in sustained-load scenarios |
| Python `pytest-benchmark` | Python-level benchmark runner | For Python API benchmarks (separate from Rust core benchmarks) |

**No new PyO3 crates needed** -- benchmark infrastructure lives in pure Rust, with PyO3 bindings only for the control plane entrypoint.

## Feature Table Stakes

Features users expect from any production-grade benchmark system:

| Feature | Description |
|---------|-------------|
| **Throughput measurement** | Messages/second per HandlerMode (SingleSync, SingleAsync, BatchSync, BatchAsync) |
| **Latency distribution** | p50/p95/p99/p999 latency histograms per handler |
| **Per-resource efficiency** | Throughput per CPU core, memory per message |
| **Memory snapshots** | Peak RSS, heap bytes tracked over sustained-load runs |
| **Queue depth tracking** | Backpressure-induced stalls measured and reported |
| **Failure scenario support** | Retry rates, DLQ routing counts, time-to-DLQ under failure injection |
| **Reproducibility** | Warmup phase, confidence intervals, machine spec documentation |
| **Machine-readable output** | JSON + CSV result files, versioned schema |
| **CI integration** | Benchmark runs on PR, warns on >10% regression vs baseline |

## Watch Out For

### Critical Pitfalls

1. **Benchmark Hot-Path Contamination** -- Measurement code itself becomes overhead. Route all metrics through `MetricsSink` facade. Aggregate off hot path. Validate measurement overhead by comparing throughput with/without instrumentation. Threshold: >5% throughput drop means metric is on hot path.

2. **PyO3 Binding Overhead Masking Rust Core Performance** -- Python API benchmarks measure GIL transitions, not `ConsumerRunner` throughput. Implement Rust-native benchmark mode (no PyO3 boundary) for actual core performance numbers. Label Python-facing results as "Python API" vs "Rust core".

3. **Incomplete Throughput Measurement (Happy-Path Only)** -- Benchmarks miss retry loops, DLQ routing, and backpressure stalls. Define failure injection scenarios. Track retry count, DLQ enqueue count, backpressure stall duration as first-class metrics.

4. **Unrealistic Kafka Broker Simulation** -- Single-broker single-partition testing misses partition rebalancing, cross-partition ordering issues. Require minimum 3 brokers, 3+ partitions, 2+ topics. Include rebalance mid-benchmark scenario.

5. **Label Cardinality Explosion** -- Adding partition-level or topic-level labels to metrics causes Prometheus OOM. Use low-cardinality labels: mode, scenario, worker_id only. Aggregate at handler/topic level; expose partition-level via runtime snapshot only.

6. **Benchmark Results Non-Reproducibility** -- No warmup, no confidence intervals, no system-state isolation. Implement mandatory warmup phase (3-5 runs before measurement). Report confidence intervals. Document machine spec alongside results.

### Moderate Pitfalls

7. **Backpressure Thresholds Configured Without Benchmark Data** -- Arbitrary queue_depth/concurrency limits. Sweep parameters (100, 500, 1000, 5000) to find inflection points.

8. **Ignoring Memory Allocation Patterns** -- Throughput reported without memory. Under sustained load, allocation pressure causes GC-like stutters. Track peak RSS, run 10M message sustained-load scenario.

9. **Handler Mode Benchmark With Trivial Python Handlers** -- `pass`-style handlers misrepresent real-world GIL hold time. Define realistic handlers (simulated I/O, computation) alongside trivial ones.

10. **Confusing Warmup With Actual Measurements** -- First N messages include JIT/cache/pool initialization. Implement explicit warmup phase, verify throughput is stable before starting measurement timer.

## Architecture Summary

### Integration Strategy: Observe, Don't Contaminate

The benchmark system must not alter production code paths. It:
1. Wraps/observes existing observable surfaces (MetricsSink, RuntimeSnapshot, ConsumerRunner)
2. Injects failures through the same interfaces production code uses (RetryCoordinator, DlqRouter)
3. Samples state via RuntimeSnapshot without adding overhead to hot paths

### Key Architectural Decisions

1. **Scenario trait + BenchmarkResult model separation** -- Scenario authors define WHAT to test without knowing HOW results are formatted. Result reporters output CSV/JSON without knowing scenario internals.

2. **`pub(crate)` benchmark module** -- Internal to the Rust crate, invisible to Python API. Thin PyO3 bindings in `src/pyconsumer.rs` delegate to `BenchmarkRunner`.

3. **Measurement via existing facades** -- Use `MetricsSink` for handler metrics, `RuntimeSnapshot::sample()` for memory, `QueueManager` for queue depth. No new instrumentation in production hot paths.

4. **Rust-native benchmark mode** -- Separate mode that measures `ConsumerRunner` + `Dispatcher` + handler execution in-process without crossing PyO3 Python-callable boundary.

5. **Hardening validation via check functions** -- `HardeningCheck` enum with `ValidationResult` struct. Checks run against `ScenarioOutcome` data after benchmark completes.

### Module Structure

```
src/benchmark/
├── mod.rs              # Public exports: Runner, Scenario trait, Result types
├── scenario.rs         # Scenario trait, WorkloadProfile, HandlerModeProfile, FailureScenario
├── runner.rs           # BenchmarkRunner orchestrator, measurement loop
├── measurement.rs      # Timer, LatencyCollector, ThroughputCollector helpers
├── results.rs          # BenchmarkResult, BenchmarkMetrics, Serializers (JSON/CSV)
└── validation.rs       # HardeningChecks enum, ValidationResult, CheckResult
```

### Build Order

1. **Phase 1: Result Models & Measurement Helpers** -- `results.rs` + `measurement.rs`. No production module dependencies.

2. **Phase 2: Scenario Definitions** -- `scenario.rs`. Depends on Phase 1 (BenchmarkResult for ScenarioOutcome).

3. **Phase 3: Benchmark Runner Core** -- `runner.rs`. Depends on Phase 1+2 and existing modules: ConsumerRunner, Dispatcher, MetricsSink, RuntimeSnapshot.

4. **Phase 4: Hardening Validation** -- `validation.rs`. Depends on Phase 1 (BenchmarkResult).

5. **Phase 5: Integration** -- `src/lib.rs` + `src/pyconsumer.rs` PyO3 bindings.

## Proposed Module Structure

```
src/benchmark/
├── mod.rs              # pub(crate) mod benchmark; exports Runner, Scenario, Result types
├── scenario.rs         # Scenario trait, WorkloadProfile, HandlerModeProfile, FailureScenario
├── runner.rs           # BenchmarkRunner { run(), stop() }, orchestrates ConsumerRunner + Dispatcher
├── measurement.rs      # Timer { wall/CPU }, LatencyCollector { p50/p95/p99/p999 }, ThroughputCollector
├── results.rs          # BenchmarkResult, BenchmarkMetrics, LatencyMetrics, MemoryMetrics, CorrectnessMetrics
│                       # Serializers: CsvSerializer, JsonSerializer (implements serde::Serialize)
└── validation.rs       # HardeningCheck enum { DlqRoutingCorrectness, RetryBehaviorMatchesPolicy,
                        #   QueueDepthInvariant, MemoryGrowthWithinBounds, HandlerLatencySla, NoMessageLoss }
                        # ValidationResult { checks, passed, summary }
```

**Python-facing control plane:** Thin `BenchmarkRunner` pyclass in `src/pyconsumer.rs` with methods:
- `run(scenario_name: &str, output_path: &Path) -> BenchmarkResult`
- `stop()`
- `list_scenarios() -> Vec<String>`

## Research Flags

- **Phase 1-2 (Result Models + Scenario):** Standard patterns, well-documented. No additional research needed.
- **Phase 3 (BenchmarkRunner):** Needs careful integration with existing ConsumerRunner + Dispatcher. Code review recommended before implementation.
- **Phase 4 (Hardening Validation):** Check design needs validation against actual RetryCoordinator/DlqRouter implementation.
- **Rust-native benchmark mode:** No precedent in current codebase. Requires spike to verify PyO3-free measurement approach works.
- **CI integration:** Benchmark regression detection (>10%) needs tooling research beyond criterion.rs.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack additions (criterion, serde) | HIGH | Well-documented, stable crates |
| Module structure | HIGH | Clean separation, follows existing KafPy patterns |
| Measurement approach | HIGH | Uses existing MetricsSink + RuntimeSnapshot facades |
| Hardening validation design | MEDIUM | Check enum is comprehensive; actual thresholds need benchmark data |
| PyO3-free Rust benchmark | MEDIUM | Architectural insight sound; needs spike to confirm |
| CI integration | LOW | Regression detection tooling not researched |

## Gaps to Address

1. **Threshold values for hardening checks** -- `QueueDepthInvariant`, `MemoryGrowthWithinBounds`, `HandlerLatencySla` need actual benchmark data to set thresholds. Currently no empirical basis.

2. **CI tooling for benchmark regression detection** -- No research done on how to compare benchmark results across commits and auto-comment on PRs.

3. **Kafka cluster setup for benchmarks** -- PITFALLS.md requires minimum 3-broker topology; how to provision this in CI environment is unresolved.

4. **Rust-native benchmark validation** -- PyO3-free benchmark mode has not been spiked; may reveal architectural issues not visible in planning.

## Sources

- ARCHITECTURE.md -- benchmark module structure, integration points, build order (HIGH confidence)
- PITFALLS.md -- 12 pitfalls with prevention strategies, phase-specific warnings (MEDIUM confidence)
- FEATURES.md -- feature landscape from observability research, relevant for measurement/validation design (MEDIUM-HIGH)
- STACK.md -- PyO3 async runtime stack, not directly relevant to benchmarking (HIGH confidence, different domain)