# KafPy Tuning Guide

This guide provides practical recommendations for tuning KafPy consumer parameters to achieve optimal benchmark results and production performance. For methodology details on how measurements are taken, see [BENCHMARK-METHODOLOGY.md](./BENCHMARK-METHODOLOGY.md).

## Key Tuning Parameters

### queue_depth

Controls the maximum number of messages buffered in the consumer channel before backpressure is applied to the Kafka consumer.

| Parameter | Value |
|-----------|-------|
| **Default** | 1000 |
| **Type** | `usize` |

**Tuning guidance:**

| Scenario | Recommended Value | Rationale |
|----------|-----------------|-----------|
| High-throughput benchmarks | 5000-10000 | Absorbs burst production rate; broker should sustain the rate |
| Latency benchmarks | 100-500 | Minimizes queueing delay; messages are processed immediately |
| Memory-constrained environments | 100-500 | Reduces memory footprint of buffered messages |
| Slow handlers | 5000-10000 | Prevents message drops under burst load when handlers are lagging |

**Tradeoffs:**
- **Larger queue**: More memory usage, but fewer drops under burst load and better throughput
- **Smaller queue**: Lower memory usage, but more backpressure and potential drops if production rate exceeds processing rate

**Monitoring**: Watch the `kafpy_handler_queue_size` metric. If this metric frequently reaches `queue_depth`, increase it or scale consumer instances.

```python
config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="benchmark",
    topics=["benchmark"],
    queue_depth=5000,  # Increase for throughput benchmarks
)
```

### concurrent_handlers

Controls the number of Tokio tasks that poll handler queues and invoke Python callbacks concurrently.

| Parameter | Value |
|-----------|-------|
| **Default** | `num_cpus::get()` (number of CPU cores) |
| **Type** | `usize` |

**Tuning guidance:**

| Handler Type | Recommended Value | Rationale |
|-------------|------------------|-----------|
| CPU-bound handlers | `num_cpus` | Maximizes CPU utilization without excessive context switching |
| I/O-bound handlers (network, file) | `num_cpus * 2` | Can overlap I/O wait times; more concurrency helps |
| Latency-sensitive scenarios | 1-4 | Reduces context-switch overhead for predictable latency |
| High-throughput with batching | `num_cpus` | Batching amortizes Python callback overhead |

**Tradeoffs:**
- **More concurrency**: Higher memory usage (each task has stack overhead), more threads, potential context-switch overhead
- **Less concurrency**: Lower memory, fewer context switches, but may leave CPU cores idle for I/O-bound handlers

**Benchmark note**: `concurrent_handlers` primarily affects throughput (msg/s) and not latency percentiles. Latency is dominated by queueing time and handler duration.

```python
config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="benchmark",
    topics=["benchmark"],
    concurrent_handlers=20,  # For I/O-bound handlers
)
```

### batch_size

Controls the number of messages batched before invoking the Python handler callback.

| Parameter | Value |
|-----------|-------|
| **Default** | 1 (single-message processing) |
| **Type** | `usize` |

**Tuning guidance:**

| Scenario | Recommended Value | Rationale |
|----------|-----------------|-----------|
| Maximum throughput | 50, 100, or 500 | Amortizes Python callback overhead across multiple messages |
| Latency-sensitive | 1 | Per-message latency measurement; no batching delay |
| Small messages (< 1KB) | 50-100 | Batch overhead is amortized well with small payloads |
| Large messages (> 100KB) | 10-50 | Memory pressure from large payloads in batch |

**Tradeoffs:**
- **Larger batch**: Higher throughput (msg/s) due to amortized callback overhead, but higher latency per message (messages wait in batch until batch is full or `max_latency_ms` expires)
- **Smaller batch**: Lower throughput per message but lower end-to-end latency for individual messages

**Important**: When `batch_size > 1`, latency is measured per-batch, not per-message. For accurate per-message latency, use `batch_size=1`.

```python
batch_config = kafpy.BatchConfig(
    max_size=100,       # Messages per batch
    max_latency_ms=10,  # Maximum wait before flushing partial batch
)
```

### timeout Settings

Three timeout parameters control different aspects of consumer behavior:

| Timeout | Default | Description |
|---------|---------|-------------|
| `fetch_timeout` | 500ms | How long the Kafka consumer waits for a fetch batch |
| `handler_timeout` | 30s | Maximum duration a handler can run before being cancelled |
| `shutdown_timeout` | 30s | How long graceful shutdown waits before force-exit |

**Tuning fetch_timeout:**

| Scenario | Recommended Value | Rationale |
|----------|-----------------|-----------|
| Latency benchmarks | 100ms | Faster fetch cycles for lower latency |
| Throughput benchmarks | 1000ms | Allows larger fetch batches to amortize overhead |
| High-latency network to broker | 2000ms+ | Accounts for network round-trip time |

**Tuning handler_timeout:**

- Should exceed your 99th-percentile handler duration
- Too short: Valid handler invocations get cancelled, increasing error rate
- Too long: Stuck handlers block shutdown and consume resources

**Tuning shutdown_timeout:**

- Must exceed the time to drain `queue_depth` messages
- Formula: `shutdown_timeout > queue_depth / expected_processing_rate`
- For `queue_depth=1000` and `rate=10000 msg/s`: `shutdown_timeout > 100ms`

**Relationship with RetryPolicy**: The `handler_timeout` should be compatible with `RetryPolicy.base_delay` for retry scenarios. If `base_delay=100ms` and retries are 3x, then handler should complete within ~300ms normally.

```python
config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="benchmark",
    topics=["benchmark"],
    fetch_timeout_ms=100,    # For latency benchmarks
    handler_timeout_ms=5000,  # For slow handlers
    shutdown_timeout_ms=60_000,
)
```

## Scenario-Specific Recommendations

### Throughput Benchmark Tuning

Recommended settings for measuring maximum sustained throughput:

```python
result = kafpy.benchmark.run_scenario(
    "throughput",
    num_messages=1_000_000,
    payload_bytes=256,
    warmup_messages=10_000,
    # Tuned parameters:
    queue_depth=10_000,
    concurrent_handlers=num_cpus * 2,
    batch_size=100,
)
```

Key changes vs. defaults:
- `queue_depth=10000`: Absorbs high production burst rate
- `concurrent_handlers=2*num_cpus`: Overlaps I/O for higher throughput
- `batch_size=100`: Amortizes Python callback overhead
- `warmup_messages=10000`: Ensures JIT is fully warmed before measurement

### Latency Benchmark Tuning

Recommended settings for accurate latency measurement:

```python
result = kafpy.benchmark.run_scenario(
    "latency",
    num_messages=100_000,
    rate=10_000,  # Steady background load
    payload_bytes=256,
    warmup_messages=1000,
    # Tuned parameters:
    queue_depth=100,
    concurrent_handlers=1,
    batch_size=1,
)
```

Key changes vs. defaults:
- `queue_depth=100`: Minimizes queueing delay
- `concurrent_handlers=1`: Eliminates context-switch jitter for predictable latency
- `batch_size=1`: Per-message latency; no batching delay

### Failure/Recovery Benchmark Tuning

Recommended settings for measuring DLQ routing and retry behavior:

```python
result = kafpy.benchmark.run_scenario(
    "failure",
    num_messages=100_000,
    failure_rate=0.05,  # 5% of messages injected with failures
    payload_bytes=256,
    warmup_messages=1000,
    # Tuned parameters:
    queue_depth=5000,
    retry_policy=kafpy.RetryPolicy(
        max_attempts=3,
        base_delay=Duration.from_millis(100),
        max_delay=Duration::from_secs(30),
    ),
)
```

Validation checklist:
- Verify `error_rate` in result matches `failure_rate` (DLQ routing working)
- Check DLQ topic for correctly routed messages after benchmark
- Confirm retry budget is exhausted for messages that eventually fail

### Batch vs. Sync Comparison Tuning

For comparing batch vs. synchronous handler modes:

```python
result = kafpy.benchmark.run_scenario(
    "batch_vs_sync",
    num_messages=100_000,
    rate=10_000,
    payload_bytes=512,
    batch_size=100,  # Compare with batch_size=1 for sync baseline
)
```

Use identical `queue_depth` and `concurrent_handlers` for both runs to isolate batch vs. sync effect.

## Broker-Side Tuning

For accurate benchmarks, broker configuration matters:

| Broker Setting | Recommended Value | Rationale |
|---------------|------------------|-----------|
| `num.network.threads` | Match or exceed client concurrency | Prevents broker-side bottleneck |
| `queued.max.requests` | Exceed `queue_depth` (default 500) | Prevents request rejection under load |
| `socket.send.buffer.bytes` | 1MB for high-throughput | Reduces send-buffer saturation |
| `socket.recv.buffer.bytes` | 1MB for high-throughput | Reduces recv-buffer saturation |
| `log.segment.bytes` | 1GB | Larger segments reduce metadata overhead |
| `num.io.threads` | Match CPU cores | Parallel I/O processing |

**Note**: Broker-side tuning is outside KafPy's scope but significantly affects benchmark results. Always report broker configuration alongside benchmark results.

## Cross-Reference to Methodology

For understanding how measurements are taken and interpreted:

| Topic | See |
|-------|-----|
| How warmup exclusion works | [BENCHMARK-METHODOLOGY.md#warmup-exclusion](./BENCHMARK-METHODOLOGY.md#warmup-exclusion) |
| How t-digest affects percentile accuracy | [BENCHMARK-METHODOLOGY.md#percentile-computation](./BENCHMARK-METHODOLOGY.md#percentile-computation) |
| How to interpret confidence intervals | [BENCHMARK-METHODOLOGY.md#confidence-intervals](./BENCHMARK-METHODOLOGY.md#confidence-intervals) |
| Reproducibility requirements | [BENCHMARK-METHODOLOGY.md#reproducibility-requirements](./BENCHMARK-METHODOLOGY.md#reproducibility-requirements) |
| Network latency contribution | [BENCHMARK-METHODOLOGY.md#network-latency-contribution](./BENCHMARK-METHODOLOGY.md#network-latency-contribution) |
| Payload size impact | [BENCHMARK-METHODOLOGY.md#payload-size-impact](./BENCHMARK-METHODOLOGY.md#payload-size-impact) |

## Quick Reference

| Parameter | Default | Latency Tuning | Throughput Tuning |
|-----------|---------|----------------|-------------------|
| `queue_depth` | 1000 | 100-500 | 5000-10000 |
| `concurrent_handlers` | num_cpus | 1-4 | num_cpus * 2 |
| `batch_size` | 1 | 1 | 50-500 |
| `fetch_timeout` | 500ms | 100ms | 1000ms |
| `handler_timeout` | 30s | 5s | 30s |
| `warmup_messages` | 1000 | 1000 | 5000-10000 |
