# KafPy Benchmark Methodology

This document describes the benchmark methodology used by KafPy's `run_scenario()` API to measure consumer performance, including percentile computation, warmup exclusion, confidence intervals, and reproducibility requirements.

## Percentile Computation

### How P50 / P95 / P99 Are Computed

KafPy uses the **t-digest** algorithm (via the `dedup_tdigest` crate) to compute high-percentile latencies from latency samples collected on the hot path.

The `HistogramRecorder` in `src/benchmark/measurement.rs` wraps a t-digest digest and exposes:

```python
histogram.percentile(50.0)   # returns p50 value in milliseconds
histogram.percentile(95.0)  # returns p95 value in milliseconds
histogram.percentile(99.0)  # returns p99 value in milliseconds
```

Percentile buckets are configurable via `PercentileBuckets` (default: `[50.0, 95.0, 99.0, 99.9]`) and stored in `BenchmarkResult` as `latency_p50_ms`, `latency_p95_ms`, `latency_p99_ms`.

### Why t-digest?

- **Naive sorted array** requires O(n log n) per flush and O(n) memory
- **t-digest** uses O(k) memory where k is the compression factor (default 100 centroids)
- Accuracy at the tails: within ~1% of the true percentile at p99 for 10M samples with default compression
- The compression factor controls the memory-accuracy tradeoff: higher compression = more accurate at extreme percentiles but more memory

### What the Percentiles Mean

| Percentile | Meaning |
|------------|---------|
| **P50 (median)** | 50% of messages are processed faster than this; the midpoint of the latency distribution |
| **P95** | 95% of messages are processed faster than this; commonly used for SLA targets |
| **P99** | 99% of messages are processed faster than this; captures tail latency issues |
| **P99.9** | 99.9% of messages; available for extreme tail analysis when configured |

## Warmup Exclusion

### Purpose

Cold-start effects bias latency measurements. The first N messages processed by a consumer encounter:

- JIT compilation warmup (Python/Rust)
- TCP connection establishment and TCP slow-start
- Kafka consumer group rebalancing
- Page cache population

To exclude these effects from measured metrics, KafPy drops the first N samples before recording latency and throughput statistics.

### How Warmup Works

`BackgroundAggregator` in `src/benchmark/measurement.rs` tracks a `messages_seen` counter and skips recording until `messages_seen >= warmup_messages`:

```rust
fn process_sample(&self, sample: Sample) {
    let msg_count = self.messages_seen.fetch_add(1, Ordering::Relaxed);
    if msg_count < self.warmup_messages {
        return;  // skip during warmup
    }
    // record sample ...
}
```

### Configuring Warmup

Default warmup is **1000 messages** for all scenarios (`ThroughputScenario`, `LatencyScenario`, `FailureScenario`). Configure via `ScenarioConfig.warmup_messages`:

```python
result = kafpy.benchmark.run_scenario(
    "throughput",
    num_messages=50_000,
    warmup_messages=5000,  # Exclude first 5000 from measurement
)
```

Guidelines:
- **Long-running scenarios**: Increase to 5000-10000 to ensure JIT is fully optimized
- **Short scenarios**: Ensure warmup_messages is less than 10% of total messages
- **Steady-state workloads**: Default 1000 is sufficient

## Confidence Intervals

### t-digest Accuracy Bounds

t-digest provides **relative accuracy guarantees**: estimates are within d% of the true percentile where d depends on the compression factor and the number of samples. At the default compression factor of 100:

- p50 estimates: typically within 0.1% of true value
- p95 estimates: typically within 0.5% of true value
- p99 estimates: typically within 1% of true value for 10M samples

Accuracy degrades slightly for extreme percentiles (p99.9) but remains bounded.

### Run-to-Run Variation

Latency percentiles vary between runs due to:
- OS scheduler jitter
- Network latency variation
- Kafka broker load
- GC pressure in Python/Rust runtime

**Recommendation**: Run each scenario at least 3 times and report the median result. For CI regression detection, compare against a baseline using comparison reports.

For more accurate confidence intervals, run 5-10 iterations and compute:
- Mean and standard deviation across runs
- Min and max observed values
- Report the interquartile range for middle 50% of runs

## Reproducibility Requirements

### Kafka Broker Requirements

- **Broker version**: Kafka 3.x or later recommended
- **Dedicated or controlled noise**: The broker under test must not be shared with production workloads
- **Broker metrics**: CPU, disk, and network usage on the broker can significantly affect results
- **Same-LAN configuration**: For accurate latency benchmarks, place the broker on the same LAN as the benchmark client (< 1ms network latency)

### Network Latency Contribution

Network latency adds directly to round-trip time:

```
measured_latency = processing_latency + network_latency
```

- **localhost benchmarks**: ~0.1-0.5ms per hop
- **remote broker**: measure network latency separately with `ping` and subtract from reported p50
- **Rule of thumb**: If broker network latency exceeds 5ms, latency benchmarks will be dominated by network, not KafPy internals

To account for network latency:
1. Measure idle round-trip time with `kafping` or a simple ping test
2. Subtract this baseline from reported latency percentiles
3. Report network-corrected values alongside raw values

### Payload Size Impact

Payload size affects benchmarks differently:

| Metric | Small Payloads (< 1KB) | Large Payloads (> 100KB) |
|--------|------------------------|--------------------------|
| **Throughput** | KafPy overhead per message is constant; smaller payloads reveal overhead | Bandwidth-bound; throughput limited by network |
| **Latency P50** | Fixed overhead dominates; minimal impact from size | Serialization cost proportional to size |
| **Latency P99** | GC pressure from many small allocations | Large object handling can increase tail latency |

**Recommendation**: Test with payload sizes matching your production workload (256B to 1MB range). Always report `payload_bytes` alongside results.

### Reproducibility Checklist

- [ ] Use the same Kafka broker (same version, same configuration) across runs
- [ ] Use the same network path (localhost for lowest variance)
- [ ] Lock or disable power management on the benchmark machine
- [ ] Use the same `num_messages` and `warmup_messages` settings
- [ ] Run multiple iterations and use the median result
- [ ] Record broker metrics (CPU, disk IO) alongside benchmark results
- [ ] Report `payload_bytes`, scenario type, and KafPy version with results

## Benchmark Types

### Throughput Scenario

Configured via `ThroughputScenario` in `src/benchmark/scenarios.rs`:

- **Goal**: Measure maximum message throughput (msg/s)
- **Rate control**: `rate=None` means unlimited (producer goes as fast as possible)
- **Primary metric**: `throughput_msg_s`
- **Warmup**: Default 1000 messages excluded

```python
result = kafpy.benchmark.run_scenario("throughput", num_messages=1_000_000)
```

### Latency Scenario

Configured via `LatencyScenario`:

- **Goal**: Measure round-trip latency under steady-state load
- **Rate control**: Required steady `rate` generates background load
- **Primary metrics**: `latency_p50_ms`, `latency_p95_ms`, `latency_p99_ms`
- **Warmup**: Default 1000 messages excluded

```python
result = kafpy.benchmark.run_scenario(
    "latency",
    num_messages=100_000,
    rate=10_000,  # 10,000 messages/second steady load
)
```

### Failure Scenario

Configured via `FailureScenario`:

- **Goal**: Measure DLQ routing correctness and retry budget enforcement
- **Failure injection**: `failure_rate` (0.0 to 1.0) determines % of messages that fail
- **Primary validation**: `error_rate` should match `failure_rate` if DLQ routing is correct
- **Warmup**: Default 1000 messages excluded from failure rate measurement

```python
result = kafpy.benchmark.run_scenario(
    "failure",
    num_messages=100_000,
    failure_rate=0.05,  # 5% of messages will fail
)
```

## Result Model

Benchmark results are captured in `BenchmarkResult` (`src/benchmark/results.rs`):

| Field | Description |
|-------|-------------|
| `scenario_config` | Echoes the scenario configuration for reproducibility |
| `total_messages` | Total messages processed (excluding warmup) |
| `duration_ms` | Total elapsed time in milliseconds |
| `throughput_msg_s` | Messages per second |
| `latency_p50_ms` | 50th percentile latency in milliseconds |
| `latency_p95_ms` | 95th percentile latency in milliseconds |
| `latency_p99_ms` | 99th percentile latency in milliseconds |
| `error_rate` | Fraction of messages that failed (0.0 to 1.0) |
| `memory_delta_bytes` | Heap memory delta during the run |
| `percentile_buckets` | Configured percentile values |
| `timestamp_ms` | Unix timestamp at end of run |

## Methodology FAQ

**Q: Why does KafPy use t-digest instead of a sorted array?**

A: For 1M+ messages, a sorted array requires significant memory and O(n log n) recomputation. t-digest uses fixed O(k) memory with O(1) per-sample insertion and provides high accuracy at the percentiles that matter most for SLA reporting.

**Q: Can I use a different percentile set?**

A: Yes. Configure `PercentileBuckets` with your desired percentiles. Note that extreme percentiles (p99.9+) require more samples for accurate estimation.

**Q: How do I compare results between runs?**

A: Use `AggregatedResult` to compute mean, stddev, min, and max across multiple runs. Run each scenario at least 3 times (5-10 recommended for CI). Compare against a baseline using the median to reduce outlier impact.

**Q: Why does the benchmark recommend localhost Kafka?**

A: Network latency is a significant variable that is hard to control on shared or remote infrastructure. Localhost Kafka minimizes network jitter and provides the most reproducible latency measurements.

**Q: How does payload size affect throughput benchmarks?**

A: Larger payloads increase serialization/deserialization cost proportionally. Throughput (msg/s) decreases as payload size increases because the per-message overhead in KafPy remains constant while payload processing scales with size. For bandwidth-limited scenarios, throughput in MB/s may remain constant while msg/s decreases.

For more information on tuning consumer parameters, see [TUNING.md](./TUNING.md).
