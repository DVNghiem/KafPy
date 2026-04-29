# Architecture Integration

**Project:** KafPy v2.0 Fan-Out/Fan-In
**Researched:** 2026-04-29
**Confidence:** MEDIUM-HIGH (based on code analysis, not yet implemented)

## Executive Summary

Fan-Out and Fan-In integrate with the existing WorkerPool architecture by extending dispatch routing and worker spawning patterns, not by replacing them. Fan-Out uses the existing `send_to_handler_by_id` for parallel multi-sink dispatch via JoinSet. Fan-In introduces a multiplexer worker that merges multiple topic receivers into a single round-robin stream. The QueueManager is reused as-is for both patterns.

---

## Fan-Out Integration

**Definition:** One message triggers multiple async handlers/sinks in parallel via JoinSet.

### New Components

| Component | Purpose | Location |
|-----------|---------|----------|
| `FanOutRegistry` | Tracks which topics receive the same message -- 1-to-many mapping | New: `src/fanout/mod.rs` |
| `FanOutSet` | Manages JoinSet of parallel handler dispatches; collects all results | New: `src/fanout/set.rs` |
| `FanOutResult` | Aggregates per-sink outcomes (success/failure per sink) | New: `src/fanout/result.rs` |
| `send_to_all` method on `QueueManager` | Dispatches to all registered sinks for a given fan-out group | Modify: `src/dispatcher/queue_manager.rs` |

### Modifications to Existing

| File | Change |
|------|--------|
| `src/dispatcher/queue_manager.rs` | Add `send_to_all(topic, message) -> Vec<Result<DispatchOutcome, DispatchError>>` -- iterates registered handlers for a fan-out group, calls `try_send` on each |
| `src/worker_pool/pool.rs` | `WorkerPool::new` spawns fan-out workers alongside regular workers; accepts `FanOutRegistry` |
| `src/worker_pool/worker.rs` | `worker_loop` gets optional `fanout_sinks: Vec<String>` -- when set, spawns a JoinSet to dispatch to all sinks in parallel |
| `src/python/handler.rs` | `HandlerMode::SingleAsync` already exists -- fan-out uses this for async parallel dispatch |
| `src/routing/context.rs` | `HandlerId` already supports topic-pattern routing; add `fanout_group: Option<String>` for 1-to-many dispatch |
| `src/consumer/config.rs` | Add `topic_patterns: Vec<String>` for multi-topic subscription (fan-out sources) |
| `src/pyconsumer.rs` | Python consumer builder exposes `register_fanout(group, sinks)` to register 1-to-many routing |

### Data Flow Changes

```
Message arrives at ConsumerDispatcher
        │
        ▼
  RoutingChain decides: "this message has fan-out group 'analytics'"
        │
        ▼
  FanOutRegistry maps group -> [sink_topic_1, sink_topic_2, sink_topic_3]
        │
        ▼
  QueueManager::send_to_all(message) called
        │
        ├──────────────────┬──────────────────┬──────────────────
        ▼                  ▼                  ▼
   try_send to          try_send to          try_send to
   sink_topic_1         sink_topic_2         sink_topic_3
   (bounded queue)      (bounded queue)      (bounded queue)
        │                  │                  │
        ▼                  ▼                  ▼
   WorkerPool           WorkerPool           WorkerPool
   (async parallel)     (async parallel)     (async parallel)
        │
        ▼
   All results collected via JoinSet
   Primary outcome (first sink success) drives offset ack
   Per-sink failures recorded for observability
```

**Key design decisions from PROJECT.md:**
- Fan-Out partial success (sinks fail independently) -- primary message ACKed immediately regardless of sink failures. Sink failures are logged/metrics-tracked but do not block ack.
- At-least-once delivery -- each sink has its own queue with independent offset tracking.
- Unbounded fan-out is explicitly OUT OF SCOPE -- `FanOutRegistry::register` requires a maximum sink count; registration fails if exceeded.

### Suggested Build Order

**Phase 1 (FANOUT-01):** `FanOutRegistry` and `send_to_all` on QueueManager
- Implement `FanOutRegistry` struct with 1-to-many topic mapping
- Add `send_to_all` to QueueManager (parallel try_send to all registered sinks)
- Unit tests for registry and parallel dispatch

**Phase 2 (FANOUT-02):** `FanOutSet` with JoinSet-based parallel dispatch
- Implement `FanOutSet::dispatch_and_collect` using tokio::join! or JoinSet
- Wire into WorkerPool -- fan-out workers spawn JoinSet of handler futures
- Ensure primary ack is driven by first completion (not all)

**Phase 3 (FANOUT-03):** Python API and multi-topic subscription
- `ConsumerConfigBuilder::topics_patterns([])` for multi-topic subscribe
- `PyConsumer::register_fanout(group, sinks)` expose to Python
- Integration test: one message -> 3 sinks, verify all receive

---

## Fan-In Integration

**Definition:** Multiple async sources merged into single handler (round-robin).

### New Components

| Component | Purpose | Location |
|-----------|---------|----------|
| `FanInMultiplexer` | Merges multiple `mpsc::Receiver<OwnedMessage>` into single `Receiver<OwnedMessage>` via round-robin select | New: `src/fanin/multiplexer.rs` |
| `FanInSource` | Wraps a topic receiver with source metadata (topic, partition) for round-robin ordering | New: `src/fanin/source.rs` |
| `round_robin_select` helper | Async loop that uses `tokio::select!` to interleave from multiple receivers | New: `src/fanin/scheduler.rs` |
| `register_fanin` on QueueManager | Registers multiple topic receivers under a single fan-in handler key | Modify: `src/dispatcher/queue_manager.rs` |

### Modifications to Existing

| File | Change |
|------|--------|
| `src/dispatcher/queue_manager.rs` | Add `register_fanin(handler_key, sources: Vec<(topic, capacity)>) -> mpsc::Receiver<OwnedMessage>` -- creates internal `FanInMultiplexer` that owns all source receivers |
| `src/worker_pool/pool.rs` | WorkerPool spawns multiplexer workers; accepts `FanInGroup` definitions |
| `src/worker_pool/worker.rs` | New `multiplexer_worker_loop` -- drives the FanInMultiplexer, receives messages round-robin, invokes handler |
| `src/consumer/config.rs` | `ConsumerConfigBuilder::topics([])` already supports multiple topics -- fan-in uses this for multi-source subscription |
| `src/pyconsumer.rs` | `PyConsumer::register_fanin(handler_key, topics)` expose to Python |

### Data Flow Changes

```
Kafka: topic_A (partition 0) --┐
Kafka: topic_B (partition 1) --┼---> ConsumerDispatcher::register_fanin
Kafka: topic_C (partition 2) --┘         │
                                      ▼
                            FanInMultiplexer owns 3 receivers
                            (one per topic subscription)
                                      │
                            round_robin_select loop
                            (tokio::select! with biased)
                                      │
                            Single message stream
                            -> WorkerPool worker
                            -> PythonHandler::invoke
                                      │
                            OffsetCoordinator::ack per topic
```

**Key design decisions from PROJECT.md:**
- Fan-In round-robin (no ordering guarantee) -- simplicity, no head-of-line blocking.
- Messages from different topics are interleaved in arrival order.
- Each topic's offset is tracked independently via OffsetCoordinator.

### Suggested Build Order

**Phase 4 (FANIN-01):** `FanInMultiplexer` and round-robin select
- Implement `FanInMultiplexer::new(sources)` -- takes ownership of multiple receivers
- Implement `fanin_worker_loop` -- tokio::select! biased round-robin, yields `OwnedMessage` with source metadata
- Unit tests for round-robin interleaving

**Phase 5 (FANIN-02):** QueueManager integration
- Add `register_fanin(handler_key, topic_capacity_pairs)` to QueueManager
- FanInMultiplexer stored internally in HandlerEntry for the fan-in case
- Modify `ack` to route to correct source topic's offset tracker

**Phase 6 (FANIN-03):** Python API
- `PyConsumer::register_fanin(handler_key, topics)` registers multi-source consumer
- `PythonHandler` receives messages from all sources via multiplexer
- Integration test: 3 topics -> 1 handler, verify round-robin interleaving

---

## Interaction Points

### Fan-Out + Fan-In Combined

```
Topic_A -> Fan-Out (to sinks B, C) -> WorkerPool (parallel async)

Topic_B --┐
Topic_C --+---> Fan-In (merge) -> WorkerPool (round-robin) -> same PythonHandler
Topic_D --┘
```

Both patterns can coexist. Fan-Out is a dispatch-time decision (one message -> many sinks). Fan-In is a receive-time decision (many topics -> one handler). They do not conflict.

### Shared Components

| Component | Used By Fan-Out | Used By Fan-In |
|-----------|-----------------|----------------|
| `QueueManager` | `send_to_all` | `register_fanin` |
| `HandlerMetadata` | Per-sink tracking | Per-source tracking |
| `OffsetCoordinator` | Independent per-sink ack | Independent per-source ack |
| `WorkerPool` | Fan-out workers (JoinSet) | Multiplexer workers |
| `PythonHandler` | Per-sink invoke | Single merged invoke |

---

## Architecture Diagram

```
                        Kafka Cluster
                              │
                 rdkafka::consumer::StreamConsumer
                              │
                 +------------+------------+
                 │                         │
                 ▼                         ▼
        ConsumerDispatcher          FanOutRegistry
        (dispatcher/mod.rs)          (1-to-many map)
                 │                         │
                 ▼                         ▼
        WorkerPool::run            send_to_all()
        (JoinSet workers)                 │
                 │          +-------------+-------------+
                 │          ▼             ▼             ▼
                 │     try_send to    try_send to   try_send to
                 │     sink_topic_1   sink_topic_2  sink_topic_3
                 │          │             │             │
                 │          ▼             ▼             ▼
                 │     WorkerPool     WorkerPool    WorkerPool
                 │     (async)        (async)       (async)
                 │          │             │             │
                 │          +-------------+-------------+
                 │                       │
                 ▼                       ▼
        FanInMultiplexer           JoinSet (parallel)
        (round_robin_select)              │
                 │                       ▼
                 ▼                 PythonHandler
        multiplexer_worker_loop    (per-sink async)
                 │
                 ▼
        PythonHandler (merged stream)
                 │
                 ▼
        OffsetCoordinator::ack (per topic)
```

---

## Risks and Mitigations

| Risk | Severity | Mitigation |
|------|----------|------------|
| Fan-out sink timeout blocks primary ack | MEDIUM | Primary ack driven by first completion; slow sinks tracked separately |
| Fan-in source skew causes offset gaps | LOW | Per-source offset tracking; no cross-source ordering assumed |
| JoinSet memory pressure with many sinks | MEDIUM | `FanOutRegistry::register` enforces max_sinks limit; bounded JoinSet |
| QueueManager lock contention with many fan-in sources | LOW | parking_lot::Mutex already fine-grained; multiplexer owns receivers after registration |
| Cancellation propagation for fan-out JoinSet | MEDIUM | CancellationToken shared across all sink tasks; abort on shutdown |

---

## Confidence Assessment

| Area | Confidence | Reason |
|------|------------|--------|
| Fan-Out architecture | HIGH | `send_to_handler_by_id` already exists in QueueManager; JoinSet pattern well-understood |
| Fan-In architecture | MEDIUM | Round-robin multiplexer is standard pattern; integration with offset tracking needs verification |
| Build ordering | HIGH | Fan-Out before Fan-In aligns with project requirements |
| Python API surface | MEDIUM | HandlerId and registry patterns understood; exact PyO3 API needs phase planning |

---

## Open Questions

1. **Fan-Out sink failure observability:** How does the primary ack outcome reflect per-sink failures? Need to define metrics/event emission for sink-level failures.
2. **Fan-In rebalance safety:** When Kafka rebalances across multiple fan-in sources, does the multiplexer need to re-subscribe? Likely yes -- need to propagate rebalance callbacks to `FanInMultiplexer`.
3. **Maximum fan-out fan-in combination:** Can a message be both fan-out dispatched AND fan-in consumed? Unlikely needed, but worth explicit exclusion in scope.

---

## Sources

- KafPy existing architecture: `src/worker_pool/pool.rs`, `src/worker_pool/worker.rs`, `src/dispatcher/queue_manager.rs`, `src/python/handler.rs`
- Tokio JoinSet: https://docs.rs/tokio/latest/tokio/task/struct.JoinSet.html
- tokio::select! for round-robin: https://docs.rs/tokio/latest/tokio/macro.select.html