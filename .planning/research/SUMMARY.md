# Project Research Summary

**Project:** KafPy v2.0 Fan-Out / Fan-In
**Domain:** Kafka consumer framework with parallel multi-sink dispatch (fan-out) and multi-source merge (fan-in)
**Researched:** 2026-04-29
**Confidence:** MEDIUM-HIGH

## Executive Summary

KafPy v2.0 adds two patterns to the existing WorkerPool-based consumer: Fan-Out (one message triggers parallel async dispatch to multiple sinks) and Fan-In (messages from multiple topics merged into a single handler round-robin). Both patterns are solvable with existing infrastructure: `tokio::task::JoinSet` for fan-out parallelism, `futures_util::StreamExt::merge` for fan-in round-robin, and the existing `QueueManager` for routing. No new Rust crates are required.

The recommended approach is Fan-Out first (Phase 1-3), then Fan-In (Phase 4-6). Fan-Out is architecturally independent and uses only existing primitives (JoinSet, Semaphore, invoke_mode_with_timeout). Fan-In depends on the streaming handler async iterator infrastructure from v1.1. The primary risk is unbounded fan-out degree exhausting memory (FOUT-01) -- this must be gated at construction time. The GIL serialization issue (FOUT-05) means fan-out to Python handlers is effectively serialized; this must be documented explicitly rather than treated as a bug.

## Key Findings

### Recommended Stack

**Zero new crates needed.** The existing dependency stack provides everything for both patterns.

**Core technologies:**
- `tokio::task::JoinSet` -- fan-out parallel dispatch. Already imported and used in WorkerPool. Each spawned task gets its own message clone; results collected via `join_next()`.
- `rdkafka::client::subscribe(&[&str])` -- multi-topic fan-in subscription. Already supported; one consumer can subscribe to N topics simultaneously.
- `futures_util::StreamExt::merge` / `select_all` -- fan-in round-robin stream merging. Already in `Cargo.toml` as `futures-util 0.3`.

Integration is entirely in-process:
- Fan-Out: `QueueManager::send_to_all` clones the message per sink and spawns via JoinSet
- Fan-In: `FanInMultiplexer` owns N receivers and round-robins via `tokio::select!` biased

### Expected Features

**Must have (table stakes):**

Fan-Out:
- **Parallel async dispatch via JoinSet** -- one message triggers N handler futures in parallel
- **Configurable fan-out degree** -- bounded max (default 4, hard cap 64), enforced at registration and dispatch
- **Fan-out result aggregation** -- `ExecutionResult` with per-sink success/failure map
- **Timeout propagation per task** -- each fan-out branch gets its own timeout scope via `invoke_mode_with_timeout`
- **Partial success handling** -- primary message ACKed immediately on primary success; sink failures logged/metrics-tracked but non-blocking

Fan-In:
- **Multi-topic subscription** -- rdkafka `subscribe(&[&str])`; one consumer, N topics
- **Round-robin merge** -- `tokio::select!` biased interleaving by arrival order
- **Source identification in context** -- `source_topic: String` field on `ExecutionContext`
- **Independent per-source backpressure** -- `PausePartition`/`ResumePartition` per topic+partition (already exists)

**Should have (competitive):**

Fan-Out:
- **Per-sink error classification** -- `ExecutionResult` carries per-sink error metadata map for monitoring
- **Per-sink metrics** -- `branch_name` label on fan-out metrics; latency histograms and error counters per sink
- **Fire-and-forget sink dispatch** -- non-critical sinks tracked in background JoinSet without blocking primary ack

Fan-In:
- **Per-source consumer lag metrics** -- topic label on existing lag metrics for per-source monitoring
- **Source-aware retry policies** -- per-source retry config map (non-retryable vs retryable source)

**Defer (v2+):**
- **Cancellable fan-out** -- `AbortHandle` per fan-out task; structured cancellation; cancellation safety requires idempotency guarantees
- **Sequential ordered fan-out** -- policy for sink B depending on sink A result; rare use case
- **Priority-based fan-in ordering** -- weighted round-robin or priority queue; high complexity
- **Cross-source aggregation windows** -- buffering + window trigger logic; separate feature
- **Distributed transactions across fan-out sinks** -- explicitly incompatible with at-least-once delivery; use idempotent handlers instead

### Architecture Approach

Fan-Out extends dispatch routing at the message level: `RoutingChain` decides a message belongs to a fan-out group, `FanOutRegistry` maps group -> [sink topics], and `QueueManager::send_to_all` dispatches clones to all sinks in parallel via JoinSet. Fan-In introduces a `FanInMultiplexer` worker that owns N topic receivers and round-robins them into a single message stream consumed by one handler.

**Major components:**

1. **`FanOutRegistry`** (new: `src/fanout/mod.rs`) -- 1-to-many topic mapping; enforces max fan-out degree at registration
2. **`FanOutSet`** (new: `src/fanout/set.rs`) -- JoinSet manager for parallel handler dispatches; aggregates per-sink outcomes
3. **`FanOutResult`** (new: `src/fanout/result.rs`) -- per-sink outcome aggregation with error metadata
4. **`FanInMultiplexer`** (new: `src/fanin/multiplexer.rs`) -- owns N receivers; `tokio::select!` biased round-robin merge
5. **`FanInSource`** (new: `src/fanin/source.rs`) -- topic receiver wrapper with source metadata
6. **`QueueManager::send_to_all`** (modify) -- parallel try_send to all registered sinks
7. **`QueueManager::register_fanin`** (modify) -- registers multiple topic receivers under one handler key

Both patterns reuse: `WorkerPool`, `OffsetCoordinator` (independent per-sink/per-source ack), `PythonHandler` (per-sink invoke or merged invoke), `HandlerMetadata`.

### Critical Pitfalls

1. **FOUT-01: Unbounded fan-out causes resource exhaustion** -- A misconfigured rule produces 10,000 sink targets, exhausting memory. Prevention: `ConsumerConfigBuilder::max_fan_out(u32)` validated at construction (1-64). Enforce cap in WorkerPool dispatch before spawning. Reject handler registration if exceeded.

2. **FOUT-03: Primary message ACKed before fan-out branches complete** -- WorkerPool acks offset immediately after primary handler returns, but JoinSet branches are still running. At-least-once guarantee violated. Prevention: Add `fan_out_tracker: Arc<FanOutTracker>` to WorkerPool. Decrement pending branch counter on each completion. Only call `queue_manager.ack()` when counter reaches zero.

3. **FOUT-05: GIL serialization creates false parallelism** -- Fan-out spawns N Python handler tasks via JoinSet expecting parallelism, but PyO3 GIL means only one executes Python code at a time. Throughput does not scale with fan-out degree beyond ~2-3x. Prevention: Document this explicitly in API docs. Recommend I/O-bound handlers for fan-out; CPU-bound handlers should use multiprocessing.

4. **FOUT-08: JoinSet borrow checker friction with Python GIL** -- `JoinSet` requires futures to be `Send`. PyO3 GIL tokens (`Python<'py>`) are not `Send`. Storing a GIL token in the future causes "Future is not Send" compilation errors. Prevention: Acquire GIL inside the spawned task body, not before. Use `unsafe { Python::assume_gil() }` only in non-Send contexts.

5. **FIN-02: Round-robin fan-in loses message ordering within key scope** -- Round-robin interleaves messages from independent sources without respecting per-key ordering. Kafka guarantees within-partition ordering only. Prevention: Document this explicitly. If ordering required, use a sharded fan-in approach: hash by key to key-specific queues.

## Implications for Roadmap

Based on research, suggested phase structure: **Fan-Out first (3 phases), then Fan-In (3 phases)**.

### Phase 1: Fan-Out Core (Registry + JoinSet Dispatch)
**Rationale:** Fan-out is the simpler pattern with well-understood primitives (JoinSet, existing QueueManager). It establishes the fan-out infrastructure that later phases (DLQ, Python handlers) depend on. FOUT-01 (bounded fan-out) must be enforced here before any dispatch.
**Delivers:** `FanOutRegistry`, `FanOutSet`, `QueueManager::send_to_all`, bounded fan-out degree enforcement, `FanOutResult` aggregation
**Implements:** Fan-Out parallel dispatch mechanism
**Avoids:** FOUT-01 (unbounded resource exhaustion), FOUT-08 (JoinSet GIL Send issue -- GIL acquired inside spawned task)
**Research flags:** None -- JoinSet and QueueManager patterns are well-documented

### Phase 2: Fan-Out Offset Commit + Partial Success
**Rationale:** FOUT-03 (primary ack before branches complete) is the most critical correctness issue for at-least-once semantics. It must be implemented alongside fan-out dispatch, not added later.
**Delivers:** `FanOutTracker` integration with WorkerPool; offset commit gated on all branch completions; partial success policy (primary ACKs immediately, sink failures non-blocking)
**Implements:** Offset commit sequencing for fan-out
**Avoids:** FOUT-03 (premature ack), FOUT-04 (duplicate processing on restart via `fan_out_id` deduplication)
**Research flags:** Existing offset commit mechanism in WorkerPool needs verification

### Phase 3: Fan-Out Python API + Multi-Topic Subscribe
**Rationale:** Python API is needed for user-facing ergonomics. Multi-topic subscribe is the fan-in side's consumer primitive, but it is also needed for fan-out source configuration. It uses existing rdkafka subscribe which already supports multiple topics.
**Delivers:** `ConsumerConfigBuilder::max_fan_out`, `PyConsumer::register_fanout`, multi-topic subscribe via `topics_patterns`, Python `@handler(sinks=[...])` decorator
**Implements:** Python-facing API surface
**Avoids:** FOUT-05 (document GIL serialization limits in API docs), FOUT-07 (metrics tagging with fan_out_id and branch_name)
**Research flags:** Existing Python handler decorator API shape needs verification for new parameter naming

### Phase 4: Fan-In Multiplexer + Round-Robin Select
**Rationale:** Fan-In depends on streaming handler async iterator infrastructure (v1.1 AsyncMessageStream). Fan-In multiplexer is the core fan-in primitive -- it owns multiple receivers and round-robins them. This builds on Phase 3's multi-topic subscribe.
**Delivers:** `FanInMultiplexer`, `FanInSource`, `round_robin_select` helper, `fanin_worker_loop`
**Implements:** Fan-In stream merge mechanism
**Avoids:** FIN-01 (partition conflict -- one consumer per topic), FIN-03 (source exhaustion blocking -- proper stream termination)
**Research flags:** rdkafka multi-topic partition assignment behavior needs verification

### Phase 5: Fan-In QueueManager Integration + Backpressure
**Rationale:** `register_fanin` on QueueManager wires the multiplexer into the existing worker pool. Per-source backpressure (PausePartition per topic+partition) must be correctly routed -- slow source must not block fast source.
**Delivers:** `QueueManager::register_fanin`, per-source backpressure routing, independent offset tracking per topic via OffsetCoordinator
**Implements:** Fan-In integrated into existing WorkerPool
**Avoids:** FIN-01 (multi-topic partition conflict -- per-source offset tracking), FIN-02 (ordering loss -- documented explicitly)
**Research flags:** Existing PausePartition signaling mechanism needs verification for multi-topic routing

### Phase 6: Fan-In Python API + Observability
**Rationale:** Final user-facing API and observability. Metrics per source (topic label) and source health monitoring complete the fan-in feature. Fan-out observability (per-sink metrics, trace context branching) also completes here.
**Delivers:** `PyConsumer::register_fanin`, per-source consumer lag metrics, source identification in ExecutionContext, fan-out trace context branching (FOUT-02 if tracing exists), fan-out DLQ schema extension (FOUT-06)
**Implements:** Full v2.0 API and observability surface
**Research flags:** Existing DLQ schema structure (FOUT-06), existing trace infrastructure (FOUT-02)

### Phase Ordering Rationale

1. **Fan-Out before Fan-In:** Fan-out is independent of streaming handler infrastructure. Fan-in depends on AsyncMessageStream from v1.1 streaming handler.
2. **Core dispatch before API:** Phase 1 establishes the mechanism before Phase 3 exposes it to Python. This allows the Rust-level correctness issues (FOUT-01, FOUT-03) to be solved without API churn.
3. **Offset commit sequencing in Phase 2:** FOUT-03 is a correctness issue (at-least-once violation) that must be fixed alongside dispatch, not retrofitted.
4. **Multiplexer before wiring:** Phase 4 builds the multiplexer as a standalone component with unit tests. Phase 5 integrates it into QueueManager. This separation enables testing without full system integration.
5. **Observability last:** FOUT-02 (tracing) and FOUT-07 (metrics) depend on the underlying mechanisms existing first. Phase 6 adds the observability layer on top of complete fan-out and fan-in mechanisms.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | Zero new crates; JoinSet, StreamExt::merge, rdkafka multi-topic subscribe all verified |
| Features | MEDIUM-HIGH | Fan-out table stakes well-understood; some differentiators (cancellation, ordered fan-out) deferred |
| Architecture | MEDIUM-HIGH | Integration patterns confirmed from code; offset commit sequencing needs verification |
| Pitfalls | MEDIUM | Many pitfalls identified with prevention strategies; some gaps (DLQ schema, trace infra) need code verification |

**Overall confidence:** MEDIUM-HIGH

### Gaps to Address

1. **Existing DLQ schema structure** -- FOUT-04 and FOUT-06 depend on understanding the current DLQ metadata layout. Needs reading of `src/dlq/` files during Phase 6 planning.
2. **Existing metrics crate and labeling conventions** -- FOUT-07 requires knowing current Prometheus metric naming. Needs verification of `MetricsSink` implementation.
3. **Existing trace infrastructure** -- FOUT-02 requires knowing which tracing crate (tracing, opentelemetry) and current header injection pattern. Needs verification if tracing already exists.
4. **JoinSet Send requirement for Python futures** -- FOUT-08 requires verifying whether current Python handler future type is actually `Send` and what the migration path is if not.
5. **rdkafka multi-topic consumer behavior** -- FIN-01 requires verifying whether a single consumer can subscribe to multiple topics and maintain correct partition offset tracking across all of them.
6. **Existing ack mechanism in WorkerPool** -- FOUT-03 requires verifying how the current offset commit fence works to know where to inject the fan-out tracker.

## Sources

### Primary (HIGH confidence)
- Tokio `JoinSet` documentation -- Send requirements for stored futures
- PyO3 GIL guide -- GIL acquisition patterns in async contexts
- W3C traceparent specification -- child span creation per branch
- futures-util `StreamExt` -- merge and select_all combinators for fan-in

### Secondary (MEDIUM confidence)
- KafPy existing architecture -- `src/worker_pool/pool.rs`, `src/worker_pool/worker.rs`, `src/dispatcher/queue_manager.rs`
- rdkafka `subscribe(&[&str])` -- multi-topic subscription support
- Existing `invoke_mode_with_timeout`, `Arc<Semaphore>`, `PausePartition`/`ResumePartition` patterns

### Tertiary (LOW confidence)
- Existing DLQ schema -- needs code verification
- Existing trace infrastructure -- unknown if tracing is currently used in project
- rdkafka multi-topic partition assignment behavior under rebalance -- needs testing verification

---
*Research completed: 2026-04-29*
*Ready for roadmap: yes*
