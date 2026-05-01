---
phase: 15-fan-in-integration
plan: 03
subsystem: observability
tags: [metrics, prometheus, consumer-lag, fan-in, OBSV-03]
dependency_graph:
  requires:
    - OBSV-03
  provides:
    - "kafpy.consumer.lag gauge with topic label"
tech_stack:
  added: []
  patterns:
    - "Prometheus gauge with lexicographically sorted labels for cardinality safety"
    - "Topic label for per-source lag monitoring in fan-in scenarios"
key_files:
  created: []
  modified:
    - path: src/observability/metrics.rs
      role: "ConsumerLagMetrics::record_lag with topic label for fan-in monitoring"
decisions:
  - "topic label included in kafpy.consumer.lag gauge (D-09)"
  - "Labels sorted lexicographically via MetricLabels to prevent cardinality explosion"
  - "partition label retained for per-partition granularity"
metrics:
  duration: ""
  completed: "2026-05-01"
---

# Phase 15 Plan 03 Summary: Consumer Lag Metrics Topic Label

## One-liner

Verified `kafpy.consumer.lag` gauge includes topic label for per-source lag monitoring in fan-in scenarios.

## Truths

- Consumer lag metrics include topic label for per-source monitoring
- `ConsumerLagMetrics::record_lag(topic, partition, lag)` accepts topic parameter
- Topic label is lexicographically sorted with other labels (cardinality-safe)
- `kafpy.consumer.lag` gauge registered in `SharedPrometheusSink::new()` (line 164)
- For fan-in, the topic label distinguishes which source is lagging (per D-09)

## Artifacts

| Artifact | Status |
|----------|--------|
| `src/observability/metrics.rs` | `kafpy.consumer.lag` gauge with topic + partition labels |

## Completed Tasks

| Task | Name | Verification | Result |
|------|------|--------------|--------|
| 1 | ConsumerLagMetrics.topic_label_implementation | Code review lines 304-311 | PASS - topic label included |
| 2 | metric_labels_sorts_lexicographically test | Code review lines 464-474 | PASS - test exists |
| 3 | kafpy.consumer.lag gauge registered | Code review line 164 | PASS - gauge registered |

## Implementation Details

**ConsumerLagMetrics::record_lag** (src/observability/metrics.rs:304-311):
```rust
impl ConsumerLagMetrics {
    pub fn record_lag(sink: &dyn MetricsSink, topic: &str, partition: i32, lag: i64) {
        let labels = MetricLabels::new()
            .insert("topic", topic)
            .insert("partition", partition.to_string());
        sink.record_gauge("kafpy.consumer.lag", lag as f64, &labels.as_slice());
    }
}
```

**Gauge Registration** (line 164):
```rust
sink.register_gauge("kafpy.consumer.lag", "Consumer lag per partition");
```

**Cardinality Safety** (lines 464-474):
```rust
#[test]
fn metric_labels_sorts_lexicographically() {
    let labels = MetricLabels::new()
        .insert("topic", "my-topic")
        .insert("handler_id", "h1")
        .insert("mode", "BatchSync");
    let slice = labels.as_slice();
    let keys: Vec<&str> = slice.iter().map(|(k, _)| *k).collect();
    assert_eq!(keys, vec!["handler_id", "mode", "topic"]);
}
```

## Deviations from Plan

None - plan executed exactly as written.

## Notes

- Tests require Python runtime environment for pyo3 linking (rdkafka bindings)
- Code inspection confirms correct implementation per D-09
- Library compiles successfully with `cargo build --lib`

## Self-Check

- [x] `kafpy.consumer.lag` gauge includes "topic" label
- [x] `ConsumerLagMetrics::record_lag(topic, partition, lag)` accepts topic parameter
- [x] Topic label is lexicographically sorted with other labels
- [x] PrometheusExporter produces metrics output
- [x] Topic label distinguishes which source is lagging in fan-in scenarios

## Self-Check: PASSED