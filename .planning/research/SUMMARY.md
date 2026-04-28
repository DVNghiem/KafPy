# KafPy Research Summary

**Date**: 2026-04-28
**Project**: Rust-Core, Python-Logic Kafka Consumer Framework

---

## Key Findings

### Stack

| Component | Choice | Version | Rationale |
|-----------|--------|---------|-----------|
| Kafka Client | rdkafka | 0.39.0 | Battle-tested, all Kafka features, librdkafka-based |
| Async Runtime | Tokio | 1.52.1 | Multi-thread work-stealing, standard for Kafka/rust ecosystem |
| Python Bindings | PyO3 | 0.28.3 | GIL-safe async via pyo3-async-runtimes 0.28.0 |
| Observability | tracing | 0.1.44 | Span semantics, tracing-opentelemetry 0.29.0 integration |
| Worker Pattern | Bounded mpsc + fixed pool | — | Pre-spawn workers, not spawn-per-message |

**Critical GIL insight**: Use `spawn_blocking` for Python calls, NOT `tokio::spawn` inside which GIL acquire blocks Tokio event loop.

### Table Stakes (Must-Have)

- Consumer groups, topic subscription, manual/auto offset commit
- Rebalance listener, graceful start/stop, pause/resume partitions
- Message deserialization (JSON, msgpack), headers/timestamp/partition access
- Context manager support, health checks, SSL/SASL
- @handler decorator API, async/await support

### Differentiators

1. **DLQ handling** — Automatic poison pill routing with retry + backoff
2. **Per-handler bounded queues** — Memory-bounded backpressure
3. **Partition-aware offset tracking** — Highest contiguous offset commit
4. **Graceful shutdown with drain** — SIGTERM handling, queue flushing

### Watch Out For

| Pitfall | Phase | Prevention |
|---------|-------|------------|
| GIL blocking Tokio event loop | Architecture | `spawn_blocking` for Python calls |
| Unbounded queue memory growth | Configuration | `queued.max.messages.kbytes` + bounded channels |
| Backpressure reset on rebalance | Rebalance handler | Re-pause partitions after assign |
| Off-by-one on seek after cache expiry | Unit test | `seek(offset + 1)` pattern, integration test |
| Commit reordering across partitions | Architecture | Serialize via single dedicated task |
| DLQ graveyard (no redrive) | Architecture | Redrive mechanism, DLQ depth monitoring |
| SIGTERM message loss | Shutdown | Graceful drain + commit before exit |

---

## Architecture Summary

**7 layers**: Kafka Ingestion → Dispatch/Routing → Queue/Backpressure → Python Execution → Offset Delivery → Failure Handling → Lifecycle Operations

**Critical properties**:
- Rust owns all runtime control (concurrency, memory, Kafka protocol)
- Python owns business logic (handlers via @handler decorator)
- GIL acquired only during Python callback execution
- Bounded channels everywhere — backpressure is native to design

**Build order**:
1. Core engine (PyO3-free Rust)
2. Python boundary (PyO3 + handler invocation)
3. Failure handling (retry + DLQ)
4. Observability (tracing + metrics)
5. Python API (@handler decorator + config)

---

## Research Files

| File | Description |
|------|-------------|
| `STACK.md` | Library versions, async patterns, PyO3 GIL strategy |
| `FEATURES.md` | Table stakes vs differentiators, anti-features |
| `ARCHITECTURE.md` | 7-layer design, GIL management, offset tracking, build order |
| `PITFALLS.md` | Domain pitfalls with warning signs and prevention strategies |

---

*Research synthesized: 2026-04-28*