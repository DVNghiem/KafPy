# Requirements: KafPy

**Defined:** 2026-04-29
**Core Value:** Python developers can write Kafka message handlers easily while Rust controls the hard runtime problems (concurrency, backpressure, retries, DLQ, offset tracking, graceful shutdown).

## v1.1 Requirements

Requirements for milestone v1.1. Each maps to roadmap phases.

### Thread Pool (SYNC)

- [ ] **SYNC-01**: Sync handlers run on Rayon work-stealing pool, not blocking Tokio poll cycle
- [ ] **SYNC-02**: `ConsumerConfigBuilder::rayon_pool_size(u32)` for configurable pool size
- [ ] **SYNC-03**: Graceful shutdown coordinates with Rayon drain (30s timeout)

### Async Timeout (TMOUT)

- [ ] **TMOUT-01**: `@handler(topic, timeout=X)` Python API — wire existing `invoke_mode_with_timeout`
- [ ] **TMOUT-02**: Timeout metadata propagated to DLQ envelope (timeout_duration, last_processed_offset)
- [ ] **TMOUT-03**: Timeout metric (count per handler) emitted to Prometheus

### Handler Middleware (MIDW)

- [ ] **MIDW-01**: `HandlerMiddleware` trait in Rust with before/after/on_error hooks
- [ ] **MIDW-02**: Built-in logging middleware (span events on handler start/complete/error)
- [ ] **MIDW-03**: Built-in metrics middleware (latency histogram, throughput counter)
- [ ] **MIDW-04**: Python API `@handler(middleware=[Logging(), Metrics()])` per handler

### Streaming Handler (STRM)

- [ ] **STRM-01**: `HandlerMode::StreamingAsync` for persistent async iterable handlers
- [ ] **STRM-02**: `@stream_handler(topic)` Python API — long-lived handler
- [ ] **STRM-03**: Lifecycle management (start/subscribe, run/loop, stop/drain, error recovery)
- [ ] **STRM-04**: Per-stream backpressure propagation

## v2 Requirements

Deferred to future release. Tracked but not in current roadmap.

### Fan-Out

- **FANOUT-01**: One message triggers multiple async handlers/sinks in parallel via `JoinSet`
- **FANOUT-02**: Python API `@handler(topic, fan_out=["audit", "notify"])` with policy options
- **FANOUT-03**: Partial failure handling (for PartialSuccess policy)

### Fan-In

- **FANIN-01**: Multiple async sources merged into single handler (round-robin, priority)
- **FANIN-02**: Python API `@handler(topics=["events", "alerts"], fan_in=True)`
- **FANIN-03**: Source identification in ExecutionContext (`source_topic` field)

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| Blocking middleware in async context | GIL hold during middleware blocks Tokio event loop |
| Fan-out with distributed transactions | Two-phase commit across handlers is not at-least-once semantics |
| Streaming handler with exactly-once | Checkpointing state complexity explodes |
| Middleware that mutates messages | Hidden mutations break debugging and predictability |
| Unbounded fan-out | No limit causes resource exhaustion |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| SYNC-01 | Phase 7 | Pending |
| SYNC-02 | Phase 7 | Pending |
| SYNC-03 | Phase 7 | Pending |
| TMOUT-01 | Phase 8 | Pending |
| TMOUT-02 | Phase 8 | Pending |
| TMOUT-03 | Phase 8 | Pending |
| MIDW-01 | Phase 9 | Pending |
| MIDW-02 | Phase 9 | Pending |
| MIDW-03 | Phase 9 | Pending |
| MIDW-04 | Phase 9 | Pending |
| STRM-01 | Phase 10 | Pending |
| STRM-02 | Phase 10 | Pending |
| STRM-03 | Phase 10 | Pending |
| STRM-04 | Phase 10 | Pending |

**Coverage:**
- v1.1 requirements: 14 total
- Mapped to phases: 14
- Unmapped: 0 ✓

---
*Requirements defined: 2026-04-29*
*Last updated: 2026-04-29 after initial definition*