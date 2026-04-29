# KafPy Roadmap

**Project:** Rust-Core, Python-Logic Kafka Consumer Framework

---

## Milestones

- ✅ **v1.0 MVP** — Phases 1-6 (shipped 2026-04-29)
- 🔄 **v1.1** — Phases 7-10 (in progress)
- ⬜ **v2.0** — Fan-out, Fan-in (future)

---

## Phases

- [ ] **Phase 7: Thread Pool** — Sync handlers on Rayon work-stealing pool
- [ ] **Phase 8: Async Timeout** — @handler timeout with DLQ metadata and metrics
- [ ] **Phase 9: Handler Middleware** — Middleware chain with logging and metrics
- [x] **Phase 10: Streaming Handler** — Persistent async iterable handlers (completed 2026-04-29)

---

## Phase 7: Thread Pool

**Goal:** Sync handlers execute on Rayon work-stealing pool instead of blocking Tokio's poll cycle, preventing heartbeat misses and rebalances.

**Depends on:** Phase 6 (Hardening)

**Requirements:** SYNC-01, SYNC-02, SYNC-03

**Success Criteria** (what must be TRUE):
1. Sync handlers execute on Rayon pool — Tokio poll cycle never blocks on sync work
2. Poll cycle not blocked for more than 100ms even with long-running sync handlers
3. ConsumerConfigBuilder::rayon_pool_size(u32) accepts valid pool size (1-256)
4. Default pool size defaults to num_cpus::get() with at least 2 threads for Tokio
5. Graceful shutdown drains Rayon pool within 30s timeout before exit

**Plans:** TBD

---

## Phase 8: Async Timeout

**Goal:** Async handlers can be aborted after a configured timeout, with timeout metadata in DLQ and timeout metrics in Prometheus.

**Depends on:** Phase 7 (Thread Pool)

**Requirements:** TMOUT-01, TMOUT-02, TMOUT-03

**Success Criteria** (what must be TRUE):
1. @handler(topic, timeout=X) Python API sets handler-specific timeout
2. Timeout fires after X seconds, handler is aborted and returns Timeout error
3. DLQ envelope includes timeout_duration and last_processed_offset metadata
4. Prometheus metric kafpy_handler_timeout_total (counter, labels: handler_name) increments on timeout

**Plans:** TBD

---

## Phase 9: Handler Middleware

**Goal:** Handler middleware chain enables before/after/on_error hooks for cross-cutting concerns (logging, metrics) without duplicating handler code.

**Depends on:** Phase 8 (Async Timeout)

**Requirements:** MIDW-01, MIDW-02, MIDW-03, MIDW-04

**Success Criteria** (what must be TRUE):
1. HandlerMiddleware trait exists with before(message) -> Message, after(message, result), on_error(message, error) hooks
2. Built-in Logging middleware emits span events on handler start/complete/error with trace context
3. Built-in Metrics middleware records latency histogram and throughput counter per handler
4. Python API @handler(middleware=[Logging(), Metrics()]) accepts middleware list per handler
5. Middleware executes in order: logging before -> handler -> metrics after (on success) or on_error (on failure)

**Plans:** 3 plans

Plans:
- [x] 09-01-PLAN.md -- MIDW-01: HandlerMiddleware trait + MiddlewareChain
- [x] 09-02-PLAN.md -- MIDW-02/03: Logging + Metrics middleware
- [x] 09-03-PLAN.md -- MIDW-04: Python API middleware

---

## Phase 10: Streaming Handler

**Goal:** Persistent async iterable handlers maintain long-lived connections (WebSocket, SSE, live dashboard feeds) with proper lifecycle and backpressure.

**Depends on:** Phase 9 (Handler Middleware)

**Requirements:** STRM-01, STRM-02, STRM-03, STRM-04

**Success Criteria** (what must be TRUE):
1. HandlerMode::StreamingAsync variant exists for persistent async iterable handlers
2. @stream_handler(topic) Python API registers long-lived handler that loops on async iterator
3. Lifecycle management: start/subscribe (connect + subscribe), run/loop (process messages until stop), stop/drain (graceful finish), error recovery (retry with backoff)
4. Per-stream backpressure: slow consumer pauses Kafka consumption, fast producer does not overflow memory

**Plans:** 4/4 plans complete

Plans:
- [x] 10-01-PLAN.md -- STRM-01: HandlerMode::StreamingAsync + invoke_streaming
- [x] 10-02-PLAN.md -- STRM-02: @stream_handler Python API
- [x] 10-03-PLAN.md -- STRM-03: streaming_worker_loop lifecycle
- [x] 10-04-PLAN.md -- STRM-04: backpressure propagation

---

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1 | v1.0 | 1/1 | Complete | brownfield |
| 2 | v1.0 | 7/7 | Complete | 2026-04-29 |
| 3 | v1.0 | 1/1 | Complete | 2026-04-29 |
| 4 | v1.0 | 2/2 | Complete | 2026-04-29 |
| 5 | v1.0 | 2/2 | Complete | 2026-04-29 |
| 6 | v1.0 | 2/2 | Complete | 2026-04-29 |
| 7 | v1.1 | 0/5 | Not started | - |
| 8 | v1.1 | 0/4 | Not started | - |
| 9 | v1.1 | 3/3 | Planned | - |
| 10 | v1.1 | 4/4 | Complete    | 2026-04-29 |

**v1.0 MVP shipped.** Full milestone history at `.planning/milestones/`.

---
*Last updated: 2026-04-29 after phase 10 planning*
