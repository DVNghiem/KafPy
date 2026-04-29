# Project Research Summary

**Project:** KafPy v1.1 — Async & Concurrency Hardening
**Domain:** Rust-core, Python-logic Kafka consumer framework
**Researched:** 2026-04-29
**Confidence:** MEDIUM-HIGH

## Executive Summary

KafPy v1.0 shipped with Tokio async runtime and `spawn_blocking` for Python handlers, but long-running sync handlers block the poll cycle, risking heartbeat misses and rebalances. The v1.1 milestone adds a Rayon work-stealing thread pool (separate from Tokio's blocking pool) to execute blocking sync handlers without stalling the Kafka poll cycle, plus richer async handler patterns (streaming, middleware, timeout, fan-in/out).

**Stack:** Add `rayon = "1.12"` for the thread pool. Tokio stays as-is. No changes to PyO3 bindings, bounded channels, or offset tracking. Integration via oneshot channels from Tokio to Rayon — Tokio never blocks on sync work.

**Architecture:** New `RayonPool` and `ThreadPoolBridge` components. Modified `worker_loop` routes sync handlers to Rayon. `HandlerMode` gets new variant. Python calls still go through `spawn_blocking` for GIL safety.

**Key risk:** Tokio/Rayon deadlock if Rayon threads call Tokio primitives. Prevention: strict isolation — no Tokio APIs called from Rayon closures.

---

## Key Findings

### Recommended Stack

**Add `rayon = "1.12"`** for work-stealing thread pool. Tokio remains the async runtime (no change). Python GIL calls stay on Tokio's blocking pool via `spawn_blocking`. Rayon handles CPU-intensive Rust preprocessing only — no Python calls on Rayon threads directly.

Configuration: `ConsumerConfigBuilder::rayon_pool_size(u32)` for user tuning. Default: `num_cpus::get()` with Tokio needing at least 1-2 threads.

### Expected Features

**Must have (table stakes):**
- Async handler timeout — wraps existing `invoke_mode_with_timeout` with Python API, DLQ metadata, metrics
- Work-stealing thread pool for sync handlers — prevents poll blocking (the core v1.1 goal)

**Should have (differentiators):**
- Handler middleware chain (logging, metrics, retries) — leverages existing `TracingSink`/`MetricsSink` infra
- Streaming handler patterns (`@stream_handler`) — persistent async iterable, new lifecycle management

**Defer (v2+):**
- Fan-out (parallel multi-handler) — depends on timeout, complex partial failure semantics
- Fan-in (multi-source merge) — depends on streaming handler async iterator infra

### Architecture Approach

Tokio dispatches work to Rayon via `pool.spawn()`. Python handler runs on Rayon thread. Completion notified via `oneshot::try_recv()`. Tokio continues polling Kafka and processing async handlers uninterrupted.

New components: `RayonPool`, `ThreadPoolBridge`. Modified: `worker_loop`, `PythonHandler`/`HandlerMode`, `WorkerPool`, `pyconsumer.rs`.

Build order: `rayon_pool.rs` → `thread_pool_bridge.rs` → handler.rs modifications → worker.rs modifications → pool.rs → pyconsumer.rs.

### Critical Pitfalls

1. **Tokio/Rayon deadlock** — Rayon threads calling Tokio primitives causes deadlock. Prevention: never call Tokio APIs from Rayon closures. Use oneshot channels for communication.
2. **PyO3 GIL on Rayon threads** — Python called from Rayon without proper thread state crashes. Prevention: all Python calls go through `spawn_blocking`, never directly from `rayon::spawn`.
3. **Memory pressure from oversized pool** — Rayon pool sized to CPU count multiplies concurrent Python invocations under load. Prevention: explicit pool size configuration, semaphore on concurrency.
4. **Nested Tokio runtime** — Creating Tokio runtime inside Rayon causes panic. Prevention: use `Handle::current()` instead of creating new runtime.
5. **Sync handler poll cycle blocking** — The core problem: long-running sync handlers block Tokio poll. Prevention: Rayon pool for sync handlers, Tokio stays free.

---

## Implications for Roadmap

Based on research, suggested phase structure:

### Phase 7: Work-Stealing Thread Pool
**Rationale:** Core v1.1 goal — prevent sync handlers from blocking poll. All other features depend on having non-blocking execution.
**Delivers:** `RayonPool`, `ThreadPoolBridge`, modified `worker_loop`, `ConsumerConfigBuilder::rayon_pool_size()`
**Addresses:** Poll blocking, async/sync execution, memory pressure from unbounded blocking pool
**Avoids:** Tokio/Rayon deadlock, GIL thread state errors

### Phase 8: Async Handler Timeout
**Rationale:** Table stakes — existing `invoke_mode_with_timeout` already implemented. Python API + DLQ metadata + metrics unblock production safety.
**Delivers:** `@handler(topic, timeout=X)` Python API, timeout metadata in DLQ envelope, timeout metrics
**Uses:** Phase 7 thread pool infrastructure
**Implements:** Handler timeout per FEATURES.md

### Phase 9: Handler Middleware Chain
**Rationale:** High-value differentiator — leverages existing `TracingSink`/`MetricsSink`. Common request, significant boilerplate reduction.
**Delivers:** `HandlerMiddleware` trait, built-in logging/metrics middleware, Python API `@handler(middleware=[...])`
**Uses:** Phase 7's pool, Phase 8's timeout pattern
**Implements:** Middleware chain per FEATURES.md

### Phase 10: Streaming Handler Patterns
**Rationale:** New lifecycle — requires async iterator infrastructure. High complexity but enables WebSocket/SSE/live dashboard use cases.
**Delivers:** `HandlerMode::StreamingAsync`, `AsyncMessageStream`, `@stream_handler(topic)` Python API, lifecycle management (start/run/stop)
**Uses:** PythonAsyncFuture (already exists), async iterator patterns
**Implements:** Streaming handler per FEATURES.md

### Phase Ordering Rationale

- Phase 7 first: Thread pool is the foundation for non-blocking execution — without it, sync handlers block poll.
- Phase 8 next: Timeout is table stakes with existing implementation — quick win, production safety.
- Phase 9 (middleware) leverages existing observability infra — medium complexity, high value.
- Phase 10 (streaming) is new async iterator lifecycle — complex, depends on nothing but enables fan-in.
- Fan-out and fan-in deferred to v1.2 (depend on timeout + streaming infra respectively).

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 10 (Streaming Handler):** Async iterator lifecycle management is complex — may need API research during planning
- **Phase 9 (Middleware):** User-defined middleware safety (no blocking, no GIL issues) needs verification

Phases with standard patterns (skip research-phase):
- **Phase 7 (Thread Pool):** Rayon + Tokio integration is well-documented — standard pattern, implementation straightforward
- **Phase 8 (Timeout):** Existing code foundation — mostly wiring work

---

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | Rayon 1.12.0 verified via crates.io; integration approach confirmed against existing codebase |
| Features | MEDIUM | Based on codebase inspection + standard async patterns; streaming/fan-in depend on new infra |
| Architecture | MEDIUM | Based on existing worker_pool code review; new components are implementation-dependent |
| Pitfalls | MEDIUM-HIGH | Tokio/Rayon deadlock and GIL issues are documented; exact timing/handling needs validation |

**Overall confidence:** MEDIUM-HIGH

### Gaps to Address

- **Rayon pool granularity:** Global pool vs per-handler pool safety when handlers have different CPU/Rust ratios — needs benchmarking
- **Streaming handler lifecycle:** Start/subscribe/stop/drain state machine — complex, needs careful design during Phase 10
- **Handler timeout interaction with Rayon:** When timeout fires on a Rayon task, does it properly cancel/abort? Needs implementation verification

---

## Sources

### Primary (HIGH confidence)
- Rayon crates.io — version 1.12.0 confirmed
- Tokio spawn_blocking docs — blocking semantics documented
- KafPy existing worker_pool architecture — pool.rs, worker.rs inspected

### Secondary (MEDIUM confidence)
- PyO3 GitHub issue #58 — GIL + async runtime interaction
- Tokio/Rayon community patterns — deadlock risk documented

### Tertiary (LOW confidence)
- Rayon pool sizing — principle sound, exact numbers need benchmarking
- Streaming handler lifecycle — standard async iterator pattern, implementation-specific

---
*Research completed: 2026-04-29*
*Ready for roadmap: yes*