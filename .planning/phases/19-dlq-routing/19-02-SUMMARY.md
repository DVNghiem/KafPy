---
gsd_state_version: 1.0
phase: 19-dlq-routing
plan: "02"
subsystem: dlq
tags: [dlq, retry, infrastructure]
dependency_graph:
  requires:
    - 19-01
  provides:
    - SharedDlqProducer with fire-and-forget produce
    - DLQ routing in worker_loop
  affects:
    - src/worker_pool/mod.rs
    - src/pyconsumer.rs
tech_stack:
  added:
    - tokio::sync::mpsc for bounded channel
    - rdkafka producer bindings
  patterns:
    - Fire-and-forget via bounded channel + try_send
    - Background task draining mpsc channel
    - DLQ metadata → Kafka headers conversion
key_files:
  created:
    - src/dlq/produce.rs
  modified:
    - src/worker_pool/mod.rs
    - src/pyconsumer.rs
    - src/dlq/mod.rs
decisions:
  - "Bounded channel capacity 100 for DLQ messages — drop on full, don't block worker"
  - "Internal do_produce() vs public produce_async() to avoid duplicate definition"
  - "DLQ metadata uses chrono DateTime<Utc> timestamps in RFC3339 header format"
metrics:
  duration: "~5 min"
  completed: 2026-04-17
  tasks_completed: 3
---

# Phase 19 Plan 02: DLQ Routing Integration Summary

## One-liner

Fire-and-forget DLQ routing wired into worker_loop: `SharedDlqProducer` decouples produce via bounded channel, `DlqMetadata` serialized as Kafka headers, `DefaultDlqRouter` computes topic/partition.

## What Was Built

### SharedDlqProducer (`src/dlq/produce.rs`)
- Wraps `FutureProducer` with bounded mpsc channel (capacity 100)
- `produce_async()` uses `try_send` for fire-and-forget (non-blocking)
- Background task spawns via `tokio::spawn` to drain channel and produce to Kafka
- `metadata_to_headers()` converts `DlqMetadata` to 7 Kafka headers: `dlq.original_topic`, `dlq.partition`, `dlq.offset`, `dlq.reason`, `dlq.attempts`, `dlq.first_failure`, `dlq.last_failure`
- Channel full → warning logged, message dropped (main pipeline unaffected)

### worker_loop DLQ Integration (`src/worker_pool/mod.rs`)
- `worker_loop` accepts `dlq_producer: Arc<SharedDlqProducer>` and `dlq_router: Arc<dyn DlqRouter>`
- `Error` arm: when `should_dlq=true`, constructs `DlqMetadata`, calls `dlq_router.route()`, calls `produce_async()` fire-and-forget
- `Rejected` arm: same DLQ routing logic
- `WorkerPool::new` stores and passes `dlq_producer` and `dlq_router` to each worker
- Test helpers: `test_config()`, `dummy_dlq_producer()`, `dummy_dlq_router()`

### Consumer Wiring (`src/pyconsumer.rs`)
- Creates `SharedDlqProducer::new(&rust_config)` on `Consumer::start()`
- Creates `DefaultDlqRouter::new(rust_config.dlq_topic_prefix.clone())`
- Passes both to `WorkerPool::new`

## Verification

| Check | Result |
|-------|--------|
| `cargo build --lib` | PASS (22 warnings, all pre-existing) |
| `SharedDlqProducer::new()` | Creates FutureProducer + bounded channel + background task |
| `produce_async()` fire-and-forget | Uses `try_send` on bounded channel |
| DLQ headers | 7 headers from DlqMetadata fields |
| `worker_loop` params | `dlq_producer` + `dlq_router` added |
| Error/Rejected arms | DLQ routing with metadata construction |
| Tests updated | `dummy_dlq_producer()` + `dummy_dlq_router()` helpers |

## Deviations from Plan

1. **[Rule 1 - Bug] Duplicate `produce_async` definition**: rdkafka `FutureRecord::key()` takes `Option<&[u8]>` not `Option<&Option<Vec<u8>>>`. Fixed by conditionally building the record with key inserted only when present.

2. **[Rule 3 - Blocking] Arc<FutureProducer>**: `SharedDlqProducer::new` returned `FutureProducer` but struct field was `Arc<FutureProducer>`. Fixed by wrapping result in `Arc::new()`.

## Known Stubs

None.

## Commits

| Hash | Message |
|------|---------|
| bfa2f3b | feat(phase-19): add SharedDlqProducer with fire-and-forget DLQ produce |
| bbe115a | feat(phase-19): wire DLQ routing into worker_loop |

## Self-Check

- [x] `src/dlq/produce.rs` exists with `SharedDlqProducer`
- [x] `SharedDlqProducer::produce_async()` uses `try_send` (fire-and-forget)
- [x] `worker_loop` accepts `dlq_producer` and `dlq_router`
- [x] Error arm routes to DLQ when `should_dlq=true`
- [x] Rejected arm routes to DLQ when `should_dlq=true`
- [x] `WorkerPool::new` passes dlq params to workers
- [x] `pyconsumer.rs` creates DLQ producer/router and wires to WorkerPool
- [x] Tests updated with helper functions
- [x] `cargo build --lib` passes
- [x] Commit hashes bfa2f3b, bbe115a verified

**Self-Check: PASSED**
