---
phase: 09-handler-middleware
plan: "02"
subsystem: middleware
tags: [middleware, logging, metrics, tracing, prometheus]
requirements_completed: [MIDW-02, MIDW-03]
provides:
  - Logging middleware (before/after/on_error tracing hooks)
  - Metrics middleware (latency histogram + throughput counter)
depends_on: [09-01]
tech_stack:
  added: []
  patterns: [middleware-chain, decorator-pattern]
key_files:
  created:
    - src/middleware/logging.rs
    - src/middleware/metrics.rs
  modified:
    - src/middleware/mod.rs
key_decisions:
  - "Logging uses tracing::info_span! with .entered() for zero-cost span lifecycle"
  - "Metrics holds SharedPrometheusSink (Clone cheap, already Arc-wrapped)"
  - "Throughput counter only incremented on success; latency recorded on both success and error"
  - "Metrics labels limited to handler_id and topic to avoid cardinality explosion"
duration: "~1 min"
completed: "2026-04-29T13:05:00Z"
---

# Phase 09 Plan 02: Built-in Logging and Metrics Middleware Summary

## One-liner

Logging middleware with tracing span events and Metrics middleware with Prometheus latency/throughput recording.

## What Was Built

Implemented MIDW-02 and MIDW-03: the built-in Logging and Metrics middleware for the handler chain.

### Components

| Component | File | Purpose |
|-----------|------|---------|
| `Logging` | `src/middleware/logging.rs` | Emits tracing span events on `before`/`after`/`on_error` with handler context fields |
| `Metrics` | `src/middleware/metrics.rs` | Records `kafpy.handler.latency` histogram and `kafpy.message.throughput` counter per invocation |
| `mod.rs` | `src/middleware/mod.rs` | Re-exports `Logging` and `Metrics` structs |

### Logging Middleware (MIDW-02)

- `before()`: Creates `tracing::info_span!("kafpy.middleware.logging")` with handler_id, topic, partition, offset fields, then emits `tracing::info!` event
- `after()`: Emits `tracing::info!` with elapsed_ms and result error_type_label
- `on_error()`: Emits `tracing::error!` with error_type label
- Zero-cost: `tracing::info_span!` with `.entered()` only allocates when a subscriber is configured

### Metrics Middleway (MIDW-03)

- Holds `SharedPrometheusSink` (Clone cheap, already Arc-wrapped)
- `after()` records `kafpy.handler.latency` histogram via `HandlerMetrics::record_latency`
- `kafpy.message.throughput` counter incremented via `ThroughputMetrics::record_throughput` only on success
- Labels: `handler_id` and `topic` only (per cardinality safety guideline)

## Verification

```
$ cargo check --lib
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.88s
```

## Success Criteria

| Criterion | Status |
|-----------|--------|
| Logging middleware emits tracing events on before/after/on_error with handler_id, topic, partition, offset fields | PASS |
| Metrics middleware records kafpy.handler.latency histogram and kafpy.message.throughput counter per handler | PASS |
| Both middleware reuse existing observability infrastructure (no new metric registrations) | PASS |

## Deviations from Plan

None - plan executed exactly as written.

## Commits

| Hash | Message |
|------|---------|
| `a9dbcbe` | feat(phase-09): implement Logging and Metrics built-in middleware |

## Pre-existing Issues (Deferred)

| Issue | File | Note |
|-------|------|------|
| `clippy::too-many-arguments` (9/7) | `src/dlq/metadata.rs:36` | Pre-existing, not introduced by this plan |
| `clippy::should-implement-trait` for `add()` method | `src/middleware/chain.rs:26` | Pre-existing from plan 01 |

## Next

Ready for plan 09-03 (wire MiddlewareChain into PythonHandler invocation path) or continue with next phase (10-streaming-handler).
