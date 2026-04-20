# Domain Pitfalls: Benchmark & Hardening Infrastructure

**Project:** KafPy v1.9 Benchmark & Hardening
**Researched:** 2026-04-20
**Confidence:** MEDIUM (based on Rust/PyO3 domain knowledge + KafPy codebase patterns; external search tools unavailable at time of research)

---

## Critical Pitfalls

Mistakes that cause benchmark infrastructure to produce misleading data, corrupt measurements, or fail to detect real production issues.

---

### Pitfall 1: Benchmark Hot-Path Contamination

**What goes wrong:** Benchmark measurement code itself becomes a significant source of overhead, distorting results. The measurement infrastructure is so heavy that it changes the behavior of the system under test.

**Why it happens:** Adding metrics, timers, and instrumentation on the critical path is seductive but catastrophic for benchmark accuracy. If measuring a handler's throughput requires acquiring locks, copying strings, or incrementing atomic counters on every message, those costs dominate at scale. KafPy's existing `MetricsSink` facade is designed to avoid exactly this.

**Consequences:**
- Reported latency is artificially high; throughput is artificially low
- Benchmarks show regressions when instrumentation is added, even though actual performance is unchanged
- Optimization cycles target instrumentation instead of real bottlenecks

**Prevention:**
- Route all measurement data through lock-free channels (Tokio mpsc, not `Arc<Mutex<Vec>>`)
- Aggregate measurements off the hot path, not on it (KafPy's `RuntimeSnapshotTask` already polls every 10s -- follow this pattern)
- Use existing `MetricsSink` facade; the noop implementation already costs nothing
- Time only what you must time on-path; everything else goes in the background snapshot task
- Validate measurement overhead by running with instrumentation disabled and comparing baseline throughput

**Detection:**
- Compare benchmark throughput with and without metrics collection
- If enabling a metric sink causes >5% throughput drop, that metric is on the hot path
- Profile with `perf` or `tokio-console` to identify lock contention in measurement code

---

### Pitfall 2: Measuring PyO3 Binding Overhead Instead of Core Engine Performance

**What goes wrong:** Benchmark results reflect the cost of PyO3 Python-callable API boundaries (spawn_blocking, GIL transitions, pyclass conversions) rather than the Rust consumer core. Comparing results across HandlerModes (SingleSync vs BatchSync vs Async) conflates Python binding overhead with actual Kafka message processing performance.

**Why it happens:** PyO3 introduces overhead at each Python/Rust boundary. A benchmark that calls into KafPy from Python measures this overhead on every invocation. This is not representative of the pure Rust throughput of `ConsumerRunner` and `Dispatcher` processing messages at speed.

**Consequences:**
- "Sync vs Async" benchmark results are dominated by Python GIL cost, not actual handler mode differences
- Optimization efforts target Python binding code instead of the Rust core
- Users cannot understand what their actual per-message overhead is vs. the baseline Kafka consumer

**Prevention:**
- Implement a "Rust-native benchmark mode" that measures `ConsumerRunner` + `Dispatcher` + handler execution in-process, without crossing the PyO3 Python-callable boundary
- Separate PyO3 binding overhead from core throughput by running identical workload in both modes
- For the public Python-facing benchmarks, label results clearly as "Python API" measurements vs. "Rust core" measurements
- Use existing `MetricsSink` to report Rust-side latency (already instrumented via `HandlerMetrics`); do not rely solely on Python-side timing

**Detection:**
- Profile the benchmark to identify where time is spent -- if >30% of time is in PyO3 binding code, binding overhead dominates
- Compare Rust-only benchmark numbers against Python API benchmark numbers -- they should differ significantly if binding overhead is real

---

### Pitfall 3: Incomplete Throughput Measurement (Happy-Path Only)

**What goes wrong:** Benchmarks report message processing throughput but never measure the cost of retries, DLQ routing, and backpressure-induced stalls. A 99th-percentile latency spike of 500ms caused by retry backoff never appears in the average, and retry rates never appear in the report.

**Why it happens:** Measuring successful end-to-end message processing is straightforward. Measuring retry loops, DLQ routing under failure, and backpressure stalls requires more instrumentation that is often skipped in initial benchmark implementations.

**Consequences:**
- Throughput numbers look great in benchmarks but degrade 10x in production when 5% of messages encounter transient failures and trigger retry loops
- Users report latency spikes that benchmarks never predicted
- The benchmark does not validate that the hardening features (retry, DLQ, backpressure) actually work under load

**Prevention:**
- Define benchmark scenarios that include failure injection -- simulate broker stalls, transient errors, and network partitions
- Track retry count, DLQ enqueue count, and backpressure stall duration as first-class metrics in the benchmark report
- Measure latency distribution (p50, p95, p99), not just average throughput
- Add a "failure scenario" profile: e.g., "5% of messages fail with transient error, measure time-to-DLQ"

**Detection:**
- Run benchmark with a scenario that injects 5% transient failures; compare p99 latency to the no-failure scenario
- If p99 latency is not reported, the benchmark is incomplete
- If retry count is not in the output, failure handling is not being measured

---

### Pitfall 4: No Realistic Kafka Broker Simulation

**What goes wrong:** Benchmarks use a real Kafka broker but with minimal topic/partitions (1 topic, 1 partition), a single producer, and no realistic traffic patterns. The benchmark does not represent production workload characteristics.

**Why it happens:** Setting up a realistic multi-broker Kafka cluster for benchmarking is complex. Single-broker single-partition testing is much easier to wire up, but it misses entire categories of real-world issues (partition rebalancing, cross-partition ordering, consumer group coordination overhead).

**Consequences:**
- Benchmark shows throughput X; production achieves X/10 because partition count causes lock contention or consumer group management overhead
- Reassignment events during benchmarking cause undefined behavior because the code was never tested under concurrent partition ownership changes
- Batch handler performance does not scale because the workload is not partitioned

**Prevention:**
- Require minimum benchmark topology: at least 3 brokers, 3+ partitions across 2+ topics
- Vary partition count in workloads (1, 5, 20 partitions) to measure partition-aware overhead
- Include a rebalance scenario: mid-benchmark, trigger a consumer group rebalance and measure time-to-recovery
- Use `rdkafka` performance tools to establish baseline broker-side limits before attributing throughput to KafPy

**Detection:**
- If benchmark topology is not documented in the report, assume unrealistic broker setup
- If no rebalance scenario exists, the benchmark never validates rebalance behavior
- Compare partition counts across runs -- if throughput does not scale with partition count, broker setup is the bottleneck

---

### Pitfall 5: Label Cardinality Explosion in Metrics

**What goes wrong:** Adding high-cardinality labels (topic names, partition IDs, handler IDs) to metrics causes cardinality explosion in Prometheus sinks. A system with 10 topics and 5 handlers produces 50x more time series when labeled by topic+handler, and 500x more when partition is included.

**Why it happens:** KafPy's `MetricLabels` in `src/observability/metrics.rs` explicitly sorts labels lexicographically to prevent this -- but if new benchmark instrumentation adds partition-level labels without awareness of cardinality, it re-introduces the problem. Benchmark writers often want fine-grained per-partition metrics, which is the worst-case cardinality.

**Consequences:**
- Prometheus sink collapses under cardinality pressure
- Benchmark run with detailed partition-level metrics causes Prometheus OOM
- Metric sink drops are silent in non-Prometheus backends

**Prevention:**
- KafPy's `MetricLabels` already enforces sorted key order; benchmark instrumentation must follow the same pattern
- Aggregate at handler/topic level; expose partition-level only via runtime snapshot, not metrics
- Benchmark metrics should use low-cardinality labels: mode, scenario, worker_id (worker count is bounded)
- Validate with `cargo bench` + Prometheus before shipping new metrics

**Detection:**
- Check metric cardinality before deploying new benchmark metrics: number of unique label combinations should be bounded
- If cardinality scales with partition count or topic count unboundedly, it is a cardinality bug
- KafPy's existing `MetricLabels::insert()` sorts lexicographically -- benchmark code should use the same API

---

### Pitfall 6: Benchmark Results Non-Reproducibility

**What goes wrong:** The same benchmark run produces different results on different machines, different days, or different runs on the same machine. No confidence intervals are reported, no warm-up strategy exists, and no system-state isolation is enforced between runs.

**Why it happens:** No warm-up iterations, no statistical validation (single-run reporting), no OS-level resource isolation, no control over background processes on the host machine, and no PyO3 GIL warmup considerations.

**Consequences:**
- "Performance regressions" are noise 80% of the time
- Optimization decisions are based on non-reproducible data
- Benchmark claims cannot be verified by external parties

**Prevention:**
- Implement mandatory warm-up phase: at least 3-5 runs with data flowing before measurement begins (PyO3's GIL-compiled code has JIT-like warmup)
- Report confidence intervals (criterion.rs provides this natively)
- Isolate benchmark runs: clear OS caches between scenarios, use fixed CPU frequency governor
- Run benchmark scenarios in deterministic order
- Document the machine spec (CPU, RAM, OS, kernel) alongside every benchmark report

**Detection:**
- If results vary by >10% across runs on the same machine, reproducibility is broken
- If no confidence interval is reported, the benchmark is not statistically valid
- If warm-up runs are not visible in the output, the measurement includes compilation effects

---

### Pitfall 7: Benchmark Infrastructure Becoming a Maintenance Burden

**What goes wrong:** The benchmark framework accumulates bespoke scripts, hardcoded paths, inconsistent result formats, and one-off scripts that no one understands. After 6 months, running the benchmark is a fragile multi-step process that requires tribal knowledge.

**Why it happens:** Benchmark scripts are often written as throwaway code ("we'll clean it up later"). No result schema is defined, output formats are ad hoc, and there is no CI integration.

**Consequences:**
- Benchmark runs are not automated in CI; results are not comparable across commits
- New engineers cannot run benchmarks; institutional knowledge is lost
- Benchmark results are not machine-readable; manual interpretation is required

**Prevention:**
- Define a `BenchmarkResult` model with a schema (JSON + CSV output)
- Machine-readable summary file (`benchmark_results.json`) alongside human-readable report
- Integrate benchmark execution into CI (at minimum: run on PR, compare against baseline, warn on >10% regression)
- Single entrypoint: `python -m kafpy.benchmark run --scenario throughput --output ./results/`
- Document methodology in a `BENCHMARK.md` at project root

**Detection:**
- If running the benchmark requires reading multiple scripts or undocumented steps, it is already a maintenance burden
- If benchmark results are not stored in version-controlled output directory, they cannot be compared across commits
- If there is no CI job running benchmarks, the infrastructure is not reliable

---

## Moderate Pitfalls

### Pitfall 8: Backpressure Thresholds Configured Without Benchmark Data

**What goes wrong:** Backpressure thresholds (queue depth limits, concurrency limits) are set to arbitrary values without measurement. Production falls over not because messages are slow but because backpressure limits are too aggressive or too permissive.

**Why it happens:** Reasonable-looking defaults (queue depth 1000, max_concurrent_handlers 10) are copied from documentation or intuition without benchmarking against actual workload characteristics.

**Prevention:**
- Add a benchmark scenario that sweeps backpressure parameters (queue_depth: 100, 500, 1000, 5000; concurrent_handlers: 1, 5, 10, 50)
- Find the inflection point where throughput degrades; that is the optimal backpressure threshold
- Report optimal thresholds per workload profile (throughput-heavy vs. latency-sensitive)

**Detection:**
- Backpressure configuration is not mentioned in benchmark output --> arbitrary values were used
- No sweep experiment exists --> optimal thresholds were not determined

---

### Pitfall 9: Ignoring Memory Allocation Patterns

**What goes wrong:** Benchmark reports throughput but not memory allocations. Under sustained load, KafPy's message flow (`OwnedMessage` cloning, `Py<PyAny>` callback storage, `Vec` growth in dispatch channels) causes allocation pressure that manifests as GC-like stutters in production Python.

**Why it happens:** Rust's ownership model hides allocation from the Python developer, but PyO3 bindings and message cloning do allocate. Benchmarks that measure throughput but not memory growth miss allocation-driven performance cliff at scale.

**Prevention:**
- Track peak RSS memory and allocation rate alongside throughput and latency in benchmark output
- Run sustained-load scenario: 10M messages through the system; measure memory delta between start and end
- Identify allocation hotspots via `dhat` or `cargo alloc`; optimize before they become production issues
- KafPy's existing `RuntimeSnapshot` tracks queue_depth; benchmark should extend this with memory snapshots

**Detection:**
- If benchmark output does not include memory usage, allocation patterns are unknown
- Sustained-load scenario (>1M messages) is missing --> memory cliff at production scale is undetected

---

### Pitfall 10: Handler Mode Benchmark Without Realistic Python Handler Implementation

**What goes wrong:** The four `HandlerMode` variants (SingleSync, SingleAsync, BatchSync, BatchAsync) are benchmarked, but the Python handler implementations used in the benchmark are trivially different (e.g., just `pass`). Real-world Python handlers have varying computation costs, I/O patterns, and GIL hold durations that dramatically affect the relative performance of handler modes.

**Why it happens:** Benchmarking handler modes requires running actual Python code. Implementing realistic Python handlers is more effort than using `pass`-style stubs. Results are misleading because real handler code changes the GIL hold time and async behavior.

**Consequences:**
- BatchAsync looks fastest in benchmark, but a real-world async handler that calls a sync DB library performs worse than BatchSync due to executor inversion
- The benchmark validates handler mode performance with trivial handlers, not realistic ones
- Users make architectural decisions based on benchmark results that do not apply to their workload

**Prevention:**
- Define at least two Python handler implementations per mode: trivial (pass) and realistic (simulated I/O wait, computation)
- Report results per handler type separately
- In benchmark report, explicitly state which Python handler implementation was used and what it does

**Detection:**
- If Python handler code is not shown in the benchmark methodology, assume trivial handlers were used
- If handler mode results do not vary by handler implementation complexity, benchmarks are incomplete

---

### Pitfall 11: Confusing Benchmark Warmup with Actual Measurements

**What goes wrong:** The first N messages processed during "warmup" are included in the benchmark measurement, skewing results because JIT compilation, cache warming, and pool initialization are all happening during measurement.

**Why it happens:** Criterion.rs handles warmup internally for Rust benchmarks, but Python-level benchmarks (via pytest-benchmark or custom scripts) often do not. PyO3's GIL acquisition, Python bytecode interpretation warmup, and Tokio task spawning all have initialization costs that should be excluded from measurements.

**Prevention:**
- Implement explicit warmup phase in Python benchmark scripts: run at least 1000 messages before starting measurement timer
- Verify warmup is complete by checking that throughput is stable across consecutive measurement windows
- Use Criterion.rs for Rust-native benchmarks (handles warmup statistically)
- Track warmup separately and report it; never include warmup messages in throughput calculations

**Detection:**
- If first 100 messages show lower throughput than subsequent ones, warmup is leaking into measurements
- If no warmup phase is documented, measurements include compilation effects

---

### Pitfall 12: Measuring Only Absolute Throughput, Not Per-Resource Efficiency

**What goes wrong:** Benchmark reports messages/second but not CPU per message, memory per message, or network per message. A benchmark showing 100k msg/sec could be using 8 cores to achieve that vs. another configuration achieving 80k msg/sec on 2 cores (higher efficiency).

**Why it happens:** Absolute throughput is the easiest metric to measure. Normalizing by resource consumption requires more instrumentation (CPU usage, memory delta, network I/O) that is often skipped.

**Prevention:**
- Report throughput per CPU core: `messages_per_second_per_core = throughput / num_cpus_used`
- Report memory per message: `bytes_per_message = peak_rss / total_messages`
- Report network per message if Kafka broker network is a bottleneck
- Include resource efficiency in the benchmark report alongside absolute throughput

**Detection:**
- If benchmark report shows only msg/sec without resource normalization, efficiency comparisons are missing
- If machine spec (CPU cores, RAM) is not in the report, per-resource efficiency cannot be calculated

---

## Phase-Specific Warnings

| Phase Topic | Likely Pitfall | Mitigation |
|-------------|---------------|------------|
| Benchmark scenario definition | Scenarios too simple (happy-path only) | Include failure injection, backpressure, and rebalance scenarios |
| Benchmark runner infrastructure | Hot-path contamination in measurement | Route all metrics via existing MetricsSink; aggregate off hot path |
| Memory/latency/throughput measurement | No sustained-load scenario | Add 10M message sustained-load scenario; track memory delta |
| Result models + CSV/JSON output | Non-reproducible format (ad hoc schema) | Define `BenchmarkResult` schema upfront; version it |
| Hardening checks | Arbitrary thresholds without measurement basis | Sweep parameters in benchmark before setting thresholds |
| Example scripts | Scripts not integrated in CI | Single entrypoint; CI runs on every PR |

---

## Prior Work: Observability Pitfalls Not Replicated Here

The existing PITFALLS.md (v1.7 observability milestone) covers:
- Async span lifetime across await points
- Global subscriber conflict
- PyO3 GIL bindings breaking span continuity
- Metrics lost before recorder installation
- Label ordering inconsistency (solved by `MetricLabels`)
- Span creation overhead in hot paths (solved by `debug_span!`)
- Memory leaks from span context retention
- Library code setting global observability state

These are **not** repeated here. The benchmark/hardening pitfalls above are orthogonal: they address the infrastructure for measuring and hardening the system, not the observability instrumentation itself.

---

## Sources

- [Criterion RS documentation](https://bheisler.github.io/criterion.rs/book/) -- benchmarking framework for Rust (HIGH)
- [PyO3 documentation](https://pyo3.rs/) -- Python extension framework (HIGH)
- [metrics crate documentation](https://docs.rs/metrics/latest/metrics/) -- zero-cost facade (HIGH)
- KafPy `src/observability/metrics.rs` -- existing MetricsSink + MetricLabels pattern (HIGH)
- KafPy `src/observability/runtime_snapshot.rs` -- polling-based snapshot design (10s interval, off hot path) (HIGH)
- KafPy `src/consumer/` -- pure-Rust consumer core (basis for Rust-native benchmark) (HIGH)
- [Prometheus label cardinality best practices](https://prometheus.io/docs/practices/naming/) (HIGH)
- [rdkafka documentation](https://docs.confluent.io/platform/current/clients/librdkafka/html/) (MEDIUM)
- [Rust dhat memory profiler](https://www.github.com/khvzak/dhat-rs) -- allocation analysis (MEDIUM)
