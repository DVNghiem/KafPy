# Benchmark

KafPy includes a benchmark module for performance testing.

## Running Benchmarks

```python
import kafpy

# Run a throughput benchmark
result = kafpy.run_scenario_py(
    "throughput",
    '{"message_size": 1024, "num_messages": 100000}'
)

print(f"Throughput: {result['throughput_msg_s']} msg/s")
print(f"P99 Latency: {result['latency_p99_ms']} ms")
```

## GIL Optimization Baseline Check

For local Rust-side callback baseline comparisons (single-sync vs batch-sync handler path), run:

```bash
cargo test perf_smoke_sync_vs_batch -- --ignored --nocapture
```

This ignored test prints comparative timing and is intended as a smoke-level performance check during tuning.

## Available Scenarios

| Scenario | Description |
|----------|-------------|
| `"throughput"` | Measure message throughput |
| `"latency"` | Measure message latency |
| `"failure"` | Test failure handling |

## Benchmark Result Fields

| Field | Type | Description |
|-------|------|-------------|
| `scenario_name` | `str` | Name of the scenario |
| `total_messages` | `int` | Total messages processed |
| `duration_ms` | `int` | Total duration in milliseconds |
| `throughput_msg_s` | `float` | Messages per second |
| `latency_p50_ms` | `float` | P50 latency in milliseconds |
| `latency_p95_ms` | `float` | P95 latency in milliseconds |
| `latency_p99_ms` | `float` | P99 latency in milliseconds |
| `error_rate` | `float` | Error rate as percentage |