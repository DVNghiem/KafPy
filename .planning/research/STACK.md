# Stack Additions: Fan-Out / Fan-In

**Project:** KafPy v2.0 Fan-Out/Fan-In
**Researched:** 2026-04-29
**Confidence:** HIGH

---

## New Dependencies

### No new crates required

**Rationale:** The existing dependency stack already provides everything needed:

| Crate | Version | Status | Rationale |
|-------|---------|--------|-----------|
| `tokio` | `1.40` | Existing | `JoinSet` is the standard tool for Fan-Out parallel spawning |
| `rdkafka` | `0.38` | Existing | Multi-topic subscribe already supported via `subscribe(&[&str])` |
| `futures-util` | `0.3` | Existing | `StreamExt::merge()` for Fan-In round-robin merging |

The Fan-Out use case (one message -> multiple sinks in parallel) is solved entirely with `tokio::task::JoinSet`, which is already imported and used in `WorkerPool`. The Fan-In use case (multiple topics -> single handler round-robin) is solved by subscribing to multiple topics in one consumer and merging streams via `futures_util::StreamExt::merge`.

---

## Integration Points

### Fan-Out: `JoinSet`-Based Parallel Dispatch

**Existing infrastructure:** `WorkerPool` already owns a `JoinSet<()>` and spawns worker tasks via `join_set.spawn(...)`. The Fan-Out fan-out feature reuses this same primitive at a finer grain -- within a single message's handler chain, not just across workers.

**New integration point:** A `FanOutDispatcher` struct that, given one `OwnedMessage` and a list of target handler IDs:

1. For each target handler, acquires a semaphore permit (concurrency limit)
2. Spawns a task onto the existing Tokio runtime via `tokio::spawn`
3. Tracks all spawned tasks in a local `JoinSet`
4. Waits for all to complete (or until cancelled)
5. Aggregates results -- primary handler ACKs immediately; secondary sink failures are logged but do not block ACK

```rust
// Pseudocode for fan-out dispatch
pub async fn fan_out_dispatch(
    msg: OwnedMessage,
    handler_ids: &[String],
    queue_manager: Arc<QueueManager>,
) -> Result<DispatchOutcome, DispatchError> {
    let mut join_set = JoinSet::new();

    for handler_id in handler_ids {
        let handler_msg = msg.clone(); // each sink gets its own copy
        join_set.spawn(async move {
            // Route to handler via queue_manager.send_to_handler_by_id
            queue_manager.send_to_handler_by_id(handler_id, handler_msg).await
        });
    }

    // Collect results as they complete
    let mut errors = Vec::new();
    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok(())) => {} // sink succeeded
            Ok(Err(e)) => errors.push(e), // sink failed, continue
            Err(e) => errors.push(anyhow::anyhow!("task panicked: {}", e)),
        }
    }

    // Primary outcome from first handler; secondary failures logged
    Ok(DispatchOutcome::PartialSuccess { errors })
}
```

**Connection to existing code:**
- `QueueManager::send_to_handler_by_id` already exists and is used by `ConsumerDispatcher::route_with_chain`
- `OwnedMessage` is `Clone` (via `#[derive(Clone)]`), enabling zero-copy sharing across spawned tasks
- Semaphore-based concurrency limiting already exists per handler via `Arc<Semaphore>`

**No new module required:** Fan-Out dispatch logic can live as a method on `QueueManager` or as a free function in `dispatcher/fanout.rs`.

---

### Fan-In: Multi-Topic Subscribe + Stream Merge

**Existing infrastructure:** `ConsumerRunner` already calls `consumer.subscribe(&config.topics.iter().map(|s| s.as_str()).collect::<Vec<_>>())`. The rdkafka `subscribe` call accepts a `Vec<&str>`, meaning one consumer can subscribe to many topics simultaneously. The `ConsumerStream` wraps the receiver in a `ReceiverStream` and implements `Stream`.

**New integration point:** A `FanInManager` that:

1. Takes a list of topic subscriptions
2. Creates one `ConsumerRunner` per topic (or uses partition-aware routing to route messages to a unified queue)
3. Merges multiple `ConsumerStream`s into one via `futures_util::StreamExt::merge`
4. Emits messages round-robin from all topics

```rust
// Pseudocode for fan-in
use futures_util::StreamExt;

pub async fn fan_in_loop(
    topics: Vec<String>,
    config: &ConsumerConfig,
    handler: mpsc::Sender<OwnedMessage>,
) -> Result<(), ConsumerError> {
    let mut streams: Vec<ConsumerStream> = Vec::new();

    for topic in &topics {
        let runner = ConsumerRunner::new(config.clone(), None, /* ... */)?;
        streams.push(runner.stream());
    }

    // Merge N streams into one round-robin stream
    let merged = futures_util::stream::select_all(streams);
    let mut merged = Box::pin(merged);

    while let Some(msg) = merged.next().await {
        handler.send(msg).await?;
    }

    Ok(())
}
```

**Key decision:** Fan-In via multi-topic consumer (all topics in one consumer group) vs. Fan-In via multiple consumers (each topic its own consumer group). Current architecture uses a single consumer group. If users need independent consumption offsets per topic, multiple `ConsumerRunner` instances are needed. The simpler case (shared consumer group, merge at dispatch) is achievable with the existing single-consumer approach by configuring the router to fan in at the routing layer.

**Connection to existing code:**
- `ConsumerRunner::new` already supports multi-topic subscribe
- `ConsumerStream` implements `Stream` and can be merged with `StreamExt::merge`
- `RoutingChain` + `RoutingDecision` already provide handler-based routing, which can serve as the Fan-In entry point (messages from multiple topics route to the same handler ID based on the chain)

---

### Python API Integration

**Fan-Out Python API:** The `@handler` decorator accepts `sinks=[...]` parameter (or similar). When `sinks` is set, the Rust-side dispatcher clones the message and sends to each sink queue in parallel via `JoinSet`.

**Fan-In Python API:** The `@handler` decorator accepts `sources=[...]` or the router configuration does topic -> handler ID mapping for multiple topics to the same handler.

**Integration with existing decorators:**
- `PythonHandler` already stores handler metadata including `mode`
- The `HandlerMode` enum differentiates `SingleAsync`, `SingleSync`, `BatchAsync`, `BatchSync`, `StreamingAsync`
- A new mode `FanOut` / `FanIn` or a flag on existing modes extends the dispatch decision tree

---

## What NOT to Add

| Rejected Choice | Why Rejected |
|-----------------|--------------|
| `async-std` runtime for Fan-In | Migration would rewrite the entire async layer. Tokio + `futures_util::StreamExt::merge` is sufficient. |
| `tokio::sync::broadcast` for Fan-Out result aggregation | Unnecessary -- `JoinSet::join_next()` handles all completion tracking. |
| `flume` or `crossbeam-channel` for cross-task communication | Existing `tokio::sync::mpsc` channels handle all inter-task messaging. |
| Separate thread pool per fan-out sink | Over-engineering. Tokio tasks handle fan-out parallelism; Rayon pool is for sync Python handlers only. |
| Distributed transaction coordinator (2PC) for fan-out | Explicitly out of scope per PROJECT.md. At-least-once semantics only. |
| New `SubscriptionManager` crate | Can be a module within the existing `consumer` module. No external crate needed. |
| `tracing` instrumentation beyond span propagation | Existing observability infrastructure handles metrics and tracing. Fan-Out/Fan-In metrics extend `WorkerPoolState`. |
| `serde` serialization for fan-out message cloning | `OwnedMessage` is already `Clone` and owns its data. No serialization needed for in-process cloning. |

---

## Summary

Fan-Out and Fan-In require **zero new Rust crates**. The dependency additions are:

```toml
# No changes to Cargo.toml for Fan-Out/Fan-In
# Existing crates provide:
# - tokio::task::JoinSet      -> Fan-Out parallel dispatch
# - rdkafka subscribe(&[&str]) -> Multi-topic Fan-In consumer
# - futures_util::StreamExt   -> Stream merging for Fan-In
```

The work is entirely integration code:
1. Fan-Out: Clone message per sink, spawn via `JoinSet`, aggregate results
2. Fan-In: Multi-topic subscribe + `StreamExt::merge` into a single handler queue

Sources:
- Existing `WorkerPool::join_set` usage confirms `JoinSet` is the correct tool
- `ConsumerRunner::subscribe` call in `runner.rs:62` confirms multi-topic support
- `futures_util` version `0.3` is already in `Cargo.toml`
