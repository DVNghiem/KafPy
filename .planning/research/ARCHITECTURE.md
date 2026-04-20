# Architecture Research: Benchmark & Hardening Integration

**Project:** KafPy v1.9 Benchmark & Hardening
**Researched:** 2026-04-20
**Confidence:** HIGH

## Executive Summary

The benchmark and hardening system integrates with the existing KafPy Rust architecture through four well-defined interfaces: `MetricsSink` (zero-cost metrics collection), `RuntimeSnapshot` (memory/latency state sampling), `ConsumerRunner` (message source for throughput tests), and the `RetryCoordinator`/`DlqRouter` pair (failure injection for retry/retry tests). Scenario definition is cleanly separated from result reporting via a `Scenario` trait + `BenchmarkResult` model structure. New modules live in `src/benchmark/` as `pub(crate)` internals, invisible to the Python API, with PyO3 bindings exposed only for the runner control plane.

## Integration Strategy

### Guiding Principle: Observe, Don't Contaminate

The benchmark system must not alter production code paths. Instead it:
1. Wraps or observes existing observable surfaces
2. Injects failures through the same interfaces production code uses (RetryCoordinator, DlqRouter)
3. Samples state via RuntimeSnapshot without adding overhead to hot paths

This means benchmark code is structurally similar to observability code - it reads from existing instrumentation, it does not add new instrumentation into critical paths.

### Existing Observable Surfaces

| Surface | What It Provides | Benchmark Use |
|---------|-----------------|---------------|
| `MetricsSink` trait | Handler latency histograms, invocation counters, batch size histograms | Record benchmark measurements |
| `RuntimeSnapshot` | Zero-cost memory/latency snapshots when called | Sample RSS, heap, queue depths at measurement points |
| `ConsumerRunner` | Message stream from Kafka | Drive throughput/latency benchmarks |
| `ConsumerDispatcher` | Dispatch cycle with backpressure signals | Measure dispatch overhead, queue depth under load |
| `RetryCoordinator` | Per-message retry state machine | Trigger retry scenarios, measure retry cost |
| `DlqRouter` | DLQ routing decisions | Verify DLQ correctness under failure conditions |
| `WorkerPool` | Python handler invocation | Measure handler throughput per mode |
| `tracing::Span` | Structured spans with fields | Annotate benchmark phases, propagate context |

### Integration Points Summary

```
Benchmark Runner
    ├── Scenario::execute() -> drives ConsumerRunner + Dispatcher
    │   ├── uses MetricsSink adapter to record measurements
    │   ├── uses RuntimeSnapshot::sample() at measurement points
    │   └── injects failures via RetryCoordinator.record_failure()
    │
    ├── Scenario::validate() -> checks hardening requirements
    │   ├── verifies DlqRouter correctness
    │   ├── checks RetryCoordinator state
    │   └── validates queue depth / inflight invariants
    │
    └── Results::serialize() -> outputs CSV/JSON via serde
```

## Module Structure

### New Module: `src/benchmark/`

```
src/benchmark/
├── mod.rs              # Public exports: Runner, Scenario trait, Result types
├── scenario.rs         # Scenario trait, WorkloadProfile, HandlerModeProfile
├── runner.rs           # BenchmarkRunner orchestrator, measurement loop
├── measurement.rs     # Timer, CounterSnapshot, LatencySnapshot helpers
├── results.rs          # BenchmarkResult, BenchmarkMetrics, Serializers
└── validation.rs       # HardeningChecks, ValidationResult, invariants
```

**Visibility:** `pub(crate)` - internal to the Rust crate, invisible to Python API.
**Python-facing control plane:** Thin PyO3 bindings in `src/pyconsumer.rs` that delegate to `BenchmarkRunner`.

### Module Responsibilities

| File | Responsibility | Public API |
|------|---------------|------------|
| `scenario.rs` | Defines WHAT to test: workload profile, handler modes, failure scenarios | `Scenario` trait, `WorkloadProfile`, `ScenarioConfig` |
| `runner.rs` | Orchestrates benchmark execution: runs scenario, collects measurements | `BenchmarkRunner::run()`, `BenchmarkRunner::stop()` |
| `measurement.rs` | HOW to measure: Timer (wall/CPU), CounterSnapshot, LatencySnapshot | `Timer`, `MeasurementCollector` |
| `results.rs` | WHAT was measured: result models + serialization | `BenchmarkResult`, `BenchmarkMetrics`, `CsvSerializer`, `JsonSerializer` |
| `validation.rs` | Hardening checks: DLQ correctness, retry behavior, queue invariants | `HardeningCheck`, `ValidationResult` |

### Separation: Scenario vs Result Reporting

**Scenario** (`scenario.rs`) describes test configuration:
```rust
pub trait Scenario: Send + Sync {
    fn name(&self) -> &str;
    fn workload(&self) -> &WorkloadProfile;
    fn handler_mode(&self) -> HandlerModeProfile;
    fn failure_scenario(&self) -> Option<&FailureScenario>;
    async fn execute(&self, runner: &BenchmarkRunner) -> ScenarioOutcome;
    fn validate(&self, outcome: &ScenarioOutcome) -> ValidationResult;
}
```

**Result Model** (`results.rs`) captures measurements:
```rust
pub struct BenchmarkResult {
    pub scenario_name: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub duration_secs: f64,
    pub throughput: ThroughputMetrics,
    pub latency: LatencyMetrics,
    pub memory: MemoryMetrics,
    pub correctness: CorrectnessMetrics,
}
```

This separation ensures:
- Scenario authors define WHAT to test without knowing HOW results are formatted
- Result reporters can output CSV, JSON, or custom formats without knowing scenario internals
- Different scenarios produce comparable metrics through the shared `BenchmarkResult` structure

## Measurement Boundaries

### Measurement Points

```
Benchmark Run
├── setup phase          -> configure consumer, handlers, scenario
├── warmup phase         -> run N messages without measuring (establish steady state)
├── measurement phase    -> run N messages, record measurements
│   ├── message dispatch -> Timer.start() at dispatch, Timer.end() at ack
│   ├── handler invoke  -> recorded via MetricsSink facade (existing HandlerMetrics)
│   ├── queue depth     -> sampled via QueueManager::snapshot() every interval
│   └── memory/RSS      -> sampled via RuntimeSnapshot::sample() every interval
├── teardown phase       -> drain pending, commit offsets, shutdown
└── report phase         -> aggregate measurements, serialize
```

### Measurement Collectors

**Timer** - wall-clock and CPU time for benchmark phases:
```rust
pub struct Timer {
    start: Instant,
    cpu_start: std::time::ProcessTime,
}
impl Timer {
    pub fn elapsed(&self) -> Duration;
    pub fn cpu_elapsed(&self) -> Duration;
}
```

**LatencyCollector** - p50/p95/p99/p999 latency from handler invocations:
```rust
pub struct LatencyCollector {
    samples: Vec<Duration>,
}
impl LatencyCollector {
    pub fn record(&mut self, duration: Duration);
    pub fn percentiles(&self) -> LatencyPercentiles;
}
```

**ThroughputCollector** - messages/second measured over sliding window:
```rust
pub struct ThroughputCollector {
    window_duration: Duration,
    window_messages: AtomicUsize,
    total_messages: AtomicUsize,
}
```

### Memory Measurement

Uses `RuntimeSnapshot` (existing infrastructure):
```rust
let snapshot = RuntimeSnapshot::sample();
// snapshot.rss_bytes, snapshot.heap_bytes, snapshot.active_tasks
// Zero-cost when not called - only measures when explicitly sampled
```

### Queue Depth Measurement

Uses `QueueManager` (existing infrastructure):
```rust
// QueueManager already tracks queue_depth and inflight per handler via atomics
// Benchmark runner samples these atomics every N ms during measurement phase
```

## Component Boundaries

### BenchmarkRunner

Owns benchmark orchestration. Coordinates:
- ConsumerRunner (message source)
- Dispatcher (dispatch measurement)
- WorkerPool (handler invocation measurement via MetricsSink)
- RetryCoordinator (failure injection)
- DlqRouter (correctness verification)

**Key invariant:** BenchmarkRunner does NOT import or modify any production module's internal state. It only:
1. Calls public APIs on existing modules
2. Reads from MetricsSink facade
3. Calls RuntimeSnapshot::sample() at measurement points

### Scenario Executor

Runs inside the benchmark loop. Each scenario:
1. Configures consumer + handlers from WorkloadProfile
2. Injects failures if FailureScenario is set
3. Collects measurements via MeasurementCollector
4. Returns ScenarioOutcome (raw data)

### Result Aggregator

Aggregates ScenarioOutcome into BenchmarkResult:
- Computes percentiles, means, stddev
- Produces CSV/JSON output
- Runs HardeningChecks against ScenarioOutcome

## Data Flow

```
ScenarioConfig
    |
    v
BenchmarkRunner::run(config)
    |
    ├── ConsumerRunner::new()  --> Kafka
    |                                |
    ├── Dispatcher::new()            |
    |    |                           |
    |    └── register_handler() --> HandlerQueue
    |
    ├── WorkerPool::new() --> PythonHandler (via spawn_blocking)
    |
    └── RetryCoordinator::new() --> DlqRouter
                                         |
                                         v
Scenario::execute() --> measurement_loop
    |
    ├── every N ms: RuntimeSnapshot::sample() -> memory
    ├── every N ms: QueueManager::snapshot()  -> queue_depth
    ├── per message: Timer.start() / Timer.end() -> latency
    └── per ack: MetricsSink facade -> throughput
                          |
                          v
              MeasurementCollector (in-memory)
                          |
                          v
              Aggregator::aggregate() -> BenchmarkResult
                          |
                          v
              Serializer::to_csv() / Serializer::to_json()
```

## Integration with Existing Modules

### ConsumerRunner - Message Source

Benchmark wraps `ConsumerRunner` to drive messages through the system:
```rust
// Benchmark creates a ConsumerRunner from scenario config
let runner = ConsumerRunner::new(consumer_config)?;

// Benchmark drives the stream, measuring dispatch latency
let stream = runner.stream();
while let Some(msg) = stream.next().await {
    let timer = Timer::start();
    // dispatch to handler...
    timer.record_dispatch_end();
}
```

**No modifications to `ConsumerRunner`** - benchmark uses existing public API.

### Dispatcher - Dispatch Measurement

Benchmark measures dispatch overhead via `DispatchOutcome`:
```rust
let outcome = dispatcher.send(message)?;
benchmark.record_dispatch(
    outcome.offset,
    outcome.queue_depth,
    timer.elapsed(),
);
```

Backpressure events are tracked:
```rust
if outcome.is_err() {
    // BackpressureEvent recorded
}
```

### MetricsSink - Handler Measurement

Benchmark uses the existing `MetricsSink` facade:
```rust
// Benchmark provides a MetricsSink implementation that records to local collector
struct BenchmarkMetricsSink {
    latency_collector: Arc<Mutex<LatencyCollector>>,
}

impl MetricsSink for BenchmarkMetricsSink {
    fn record_histogram(&self, name: &str, value: f64, labels: &[(&str, &str)]) {
        if name == "kafpy.handler.latency" {
            self.latency_collector.lock().unwrap().record(Duration::from_secs_f64(value));
        }
    }
    // ...
}
```

This is the same interface production Prometheus adapter uses - benchmark uses a different implementation that records locally instead of exporting to Prometheus.

### RuntimeSnapshot - Memory Sampling

```rust
// At each sampling interval during measurement phase
let mem_snap = RuntimeSnapshot::sample();
benchmark.record_memory(mem_snap.rss_bytes, mem_snap.heap_bytes);
```

Zero-cost when not called - only samples when benchmark explicitly requests it.

### RetryCoordinator - Failure Injection

Benchmark injects failures to test retry behavior:
```rust
// In failure scenario, after handler executes, simulate a failure
let reason = FailureReason::Retryable(RetryableKind::NetworkTimeout);
let (should_retry, should_dlq, delay) = retry_coordinator.record_failure(
    topic, partition, offset, &reason,
);
```

This is the same `RetryCoordinator` used in production - benchmark triggers the same state machine.

### DlqRouter - Correctness Verification

After benchmark run, verify DLQ correctness:
```rust
let check = HardeningCheck::dlq_routing_correctness(
    &dlq_router,
    expected_routing,
    actual_routing,
);
```

### WorkerPool - Throughput Per Handler Mode

Benchmark exercises all 4 `HandlerMode` variants:
- `SingleSync` - one message, spawn_blocking
- `SingleAsync` - one message, into_future
- `BatchSync` - fixed-window batch, spawn_blocking
- `BatchAsync` - fixed-window batch, into_future

Throughput is measured as messages/second per mode.

## New vs Modified

### New Modules

| Module | Lines | Purpose |
|--------|-------|---------|
| `src/benchmark/` | ~600-800 | All benchmark infrastructure |

### Modified Modules

| Module | Change | Rationale |
|--------|--------|-----------|
| `src/lib.rs` | Add `pub(crate) mod benchmark` | Expose benchmark module to internal Rust code |
| `src/pyconsumer.rs` | Add `BenchmarkRunner` pyclass | Python API for benchmark control |
| `src/observability/metrics.rs` | Add `BenchmarkMetricsSink` impl | MetricsSink impl for local measurement collection |

**No production modules modified** - benchmark system uses only public/semi-public interfaces.

## Build Order

### Phase 1: Result Models & Measurement Helpers

1. `src/benchmark/results.rs` - `BenchmarkResult`, `BenchmarkMetrics`, serialization traits
2. `src/benchmark/measurement.rs` - `Timer`, `LatencyCollector`, `ThroughputCollector`
3. `src/benchmark/mod.rs` - module structure

**Rationale:** No dependencies on existing production modules. Pure data structures and math.

### Phase 2: Scenario Definitions

4. `src/benchmark/scenario.rs` - `Scenario` trait, `WorkloadProfile`, `ScenarioConfig`

**Depends on:** Phase 1 (uses `BenchmarkResult` for `ScenarioOutcome`)

### Phase 3: Benchmark Runner Core

5. `src/benchmark/runner.rs` - `BenchmarkRunner` orchestrator

**Depends on:** Phase 1 + 2, plus existing modules: `ConsumerRunner`, `Dispatcher`, `MetricsSink`, `RuntimeSnapshot`

### Phase 4: Hardening Validation

6. `src/benchmark/validation.rs` - `HardeningCheck`, `ValidationResult`

**Depends on:** Phase 1 (uses `BenchmarkResult`)

### Phase 5: Integration

7. `src/lib.rs` - add `pub(crate) mod benchmark`
8. `src/pyconsumer.rs` - add `BenchmarkRunner` pyclass

**Depends on:** All above phases

## Hardening Validation Design

### Validation Checks

```rust
pub enum HardeningCheck {
    DlqRoutingCorrectness,
    RetryBehaviorMatchesPolicy,
    QueueDepthInvariant,
    MemoryGrowthWithinBounds,
    HandlerLatencySla,
    NoMessageLoss,
}
```

### ValidationResult

```rust
pub struct ValidationResult {
    pub checks: Vec<CheckResult>,
    pub passed: bool,
    pub summary: String,
}

pub struct CheckResult {
    pub check: HardeningCheck,
    pub passed: bool,
    pub details: String,
    pub measured_value: Option<f64>,
    pub expected_value: Option<f64>,
}
```

### DLQ Routing Check

```rust
// Verify: all terminal failures routed to DLQ, no retryable failures incorrectly DLQ'd
pub fn check_dlq_routing_correctness(
    dlq_router: &dyn DlqRouter,
    expected: &[(topic, partition, offset, FailureReason)],
    actual: &[(topic, partition, offset)],
) -> CheckResult
```

### Queue Depth Invariant

```rust
// Verify: queue depth never exceeds capacity, inflight never exceeds concurrency limit
pub fn check_queue_depth_invariant(
    queue_manager: &QueueManager,
    capacity_per_topic: &HashMap<String, usize>,
) -> CheckResult
```

## Anti-Patterns to Avoid

### No Direct Instrumentation in Hot Paths

**Bad:** Adding benchmark-specific counters inside `Dispatcher::send_with_policy`
**Why:** Would add atomic increments to every message dispatch in production
**Good:** Record measurements via existing MetricsSink facade, which already has atomic operations for production metrics

### No Forking or Splicing Production Code

**Bad:** Copy-pasting Dispatcher code into benchmark module and adding measurement code
**Why:** Diverges from production over time, adds maintenance burden
**Good:** Use the existing Dispatcher with a benchmark-provided MetricsSink adapter

### No Shared Mutable State Between Benchmark Phases

**Bad:** BenchmarkRunner storing results in a static counter that accumulates across runs
**Why:** Contamination between scenarios
**Good:** Each scenario run produces a fresh `BenchmarkResult`, aggregator combines at the end

### No PyO3 Dependency in Benchmark Core

**Bad:** `src/benchmark/runner.rs` importing `pyo3::prelude`
**Why:** Contaminates pure-Rust benchmark logic with PyO3, complicates testing
**Good:** `BenchmarkRunner` lives in pure Rust, PyO3 binding in `src/pyconsumer.rs` calls it

## Scalability Considerations

| Concern | At 100 messages | At 1M messages | At 10M messages |
|---------|-----------------|-----------------|------------------|
| Memory | All samples in vec | Periodic flush to disk | Streaming serializer, keep summary only |
| Latency percentiles | Exact (all samples) | T-digest approximation | T-digest with bounded memory |
| Queue depth sampling | Every message | Every 100ms | Every 1s, trending only |
| Result file size | <1KB JSON | ~10MB JSON | ~100MB CSV with summary |

## Sources

- Confluence: KafPy v1.9 milestone definition (internal)
- Code review: `src/dispatcher/mod.rs`, `src/python/execution_result.rs`, `src/observability/metrics.rs`, `src/consumer/runner.rs`, `src/coordinator/retry_coordinator.rs`