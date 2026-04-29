# v2.0 Feature Research: Fan-Out and Fan-In Patterns

**Project:** KafPy
**Context:** v2.0 milestone — Fan-Out (multi-sink parallel) and Fan-In (multi-source round-robin)
**Research date:** 2026-04-29
**Confidence:** MEDIUM-HIGH

---

## 1. Feature Categories

### Fan-Out Features

#### Table Stakes

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| **Parallel async dispatch** | One message triggers N async handlers/sinks in parallel. Without this, users must manually spawn tasks and manage lifecycle. | Low | JoinSet-based; existing Tokio primitives are sufficient. |
| **Configurable fan-out degree** | Unbounded parallel dispatch causes resource exhaustion. Users need `max_concurrent` to limit simultaneous fan-out tasks. | Low | Semaphore-based limiting per handler; already exists for concurrency limiting. |
| **Fan-out result aggregation** | Caller needs to know overall success/failure. Aggregate N results into a single `ExecutionResult`. | Low | Simple: all ok = ok, any fail = error (configurable). |
| **Timeout propagation** | Individual fan-out tasks timing out should not block the primary handler. Each task needs its own timeout scope. | Medium | Reuse existing `invoke_mode_with_timeout` per fan-out task. |
| **Partial success handling** | When some fan-out sinks fail, the primary message should still be acknowledged (at-least-once) rather than retrying indefinitely. | Medium | Policy enum: `AllSucceed`, `AnySucceed`, `MajoritySucceed`. |

#### Differentiators

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **Fire-and-forget sink dispatch** | For non-critical side effects (logging, metrics), waiting for completion is wasted latency. Spawn and forget with bounded in-flight tracking. | Medium | Tasks tracked in background JoinSet, not awaited. |
| **Cancellable fan-out** | When the primary handler succeeds but a slow fan-out task is stuck, users need a way to cancel stragglers on commit. | High | Requires structured cancellation via `AbortHandle` per fan-out task. |
| **Per-sink error classification** | Different sinks may have different retry policies (e.g., non-retryable DB sink vs retryable HTTP sink). Error must carry per-sink classification. | Medium | Extend `ExecutionResult` with per-sink error metadata map. |
| **Ordered fan-out (sequential)** | Some fan-out patterns require ordering (sink B depends on sink A result). Support sequential execution as a policy. | Medium | Policy: `Sequential` — await each in order before proceeding. |
| **Fan-out metrics (per sink)** | Users want to know which sink is slow or failing, not just aggregate metrics. Per-sink latency histograms and error counters. | Medium | Sink identifier in metrics labels; existing MetricsSink extensible. |

#### Complexity Notes

| Driver | Why Complex |
|--------|------------|
| **GIL serialization across fan-out tasks** | If N fan-out tasks all call Python handlers, they serialize on the GIL anyway. Parallelism only helps if preprocessing (Rust) is CPU-intensive before Python calls. Users may expect parallelism and not understand why they're not getting it. |
| **Cancellation safety** | When cancelling fan-out tasks mid-execution, you must ensure no partial side effects (e.g., DB committed, cache not updated). Idempotency is the user's responsibility, but we must not leave tasks in a dangling state. |
| **Offset commit timing** | If fan-out is partial-success (some sinks fail), when do we commit the offset? Options: commit immediately on primary success, or commit after all fan-out tasks settle. The PROJECT.md says commit immediately. |
| **Memory under high fan-out degree** | Each fan-out task holds a message copy and a future. With bounded channel capacity and high fan-out degree, memory multiplies quickly. |

#### Dependencies

```
Existing: Arc<Semaphore> per handler key, JoinSet (tokio::task::join_set), ExecutionResult, invoke_mode_with_timeout
           │
           ├──► [Fan-Out Degree Limit] ──── Semaphore already limits concurrency; reuse
           │
           ├──► [Parallel Dispatch] ─────── JoinSet already in Tokio; straightforward
           │
           ├──► [Timeout per Task] ───────── invoke_mode_with_timeout already exists; wrap per fan-out task
           │
           └──► [Result Aggregation] ─────── new (simple collect + classify)
```

**Fan-Out does NOT depend on streaming handler infrastructure.** Fan-out is single-message dispatch to multiple sinks, not persistent async iterables. It can be implemented independently of fan-in.

---

### Fan-In Features

#### Table Stakes

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| **Multi-source subscription** | Single handler receives messages from N topics. Kafka consumer must subscribe to all topics simultaneously. | Low | rdkafka `client.subscribe(topics)`, already supported. |
| **Round-robin interleaving** | Messages from multiple topics are interleaved in arrival order. Handler sees a unified stream without needing to manage N subscriptions. | Low | Merge at dispatcher level; interleave by arrival timestamp. |
| **Source identification in context** | Handler needs to know which topic a message came from. `ExecutionContext.source_topic` field already exists conceptually; needs wiring. | Low | Add `source_topic: String` to `ExecutionContext`. |
| **Independent per-source backpressure** | If one source is slow, it should not block other sources. Each topic/queue must have independent backpressure signaling. | Medium | Each source gets its own bounded channel; PausePartition applies per topic+partition. |
| **Graceful handling of source imbalance** | Topic A produces 1000 msg/s, Topic B produces 1 msg/s. Slow source should not cause backlog on fast source. | Medium | Independent queue depths per source. |

#### Differentiators

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **Priority-based source ordering** | Some sources are more important (e.g., `alerts` before `events`). Users can set priority so high-priority messages are processed first even if queue is fuller for low-priority. | High | Requires weighted round-robin or priority queue per source. |
| **Source-aware retry policies** | Non-retryable source (malformed messages) vs retryable source (transient failures). Error classification per source. | Medium | Per-source retry config map. |
| **Cross-source aggregation window** | For time-series or batch aggregation across sources (e.g., compute correlation between topic A and B over a window). | High | Needs buffering + window trigger logic; out of scope for basic fan-in. |
| **Fan-In with topic partition routing** | Handler can route messages to sub-handlers based on topic. E.g., events → analytics, alerts → pager. | Medium | Fan-in as router, not just merger. |
| **Source health monitoring** | Detect when a source topic is lagging or stalled. Per-source consumer lag metrics. | Medium | Expose per-topic lag via MetricsSink with topic label. |

#### Complexity Notes

| Driver | Why Complex |
|--------|------------|
| **Partition ordering across topics is not guaranteed** | Topic A partition 0 and Topic B partition 0 each have their own ordering guarantees. KafPy cannot promise global ordering across topics. Users must understand this. |
| **Rebalance complexity with multiple topics** | When a rebalance occurs, all topic subscriptions must be revoked and reassigned atomically. rdkafka handles this, but KafPy's partition tracking per topic becomes more complex. |
| **Backpressure across sources with different rates** | Fast source filling its bounded channel while slow source drains slowly. Must pause fast producer without pausing slow producer. Requires per-source `PausePartition` signals. |
| **Message schema differences across topics** | Fan-in handler receives a generic `msg.value()`. If Topic A and Topic B have different schemas, the handler must do runtime type checking or schema validation. Python-side concern, but worth documenting. |
| **Consumer group metadata** | When subscribing to multiple topics, consumer group state is spread across partitions of multiple topics. Committing offset on one topic must not affect others. Offset coordinator must track per-topic offset state. |

#### Dependencies

```
Existing: QueueManager (per handler bounded channels), ConsumerRunner::next() (OwnedMessage), PythonAsyncFuture
           │
           ├──► [Round-robin merge] ─────── New: Interleave multiple sources by arrival order
           │
           ├──► [Source identification] ──── Add source_topic to ExecutionContext
           │
           ├──► [Per-source backpressure] ── PausePartition already exists; apply per topic+partition
           │
           └──► [Multi-topic subscription] ─ rdkafka supports this; routing must extend to N topics
```

**Fan-In depends on streaming handler async iterator infrastructure** for the merge combinator. The `AsyncMessageStream` from streaming handler (v1.1) is what allows multiple async sources to be merged. Fan-In and Streaming share the async iterator abstraction.

---

## 2. Feature Dependencies Summary

```
Fan-Out (independent):
  Message → JoinSet spawn N tasks → timeout per task → aggregate results → commit

Fan-In (depends on streaming infra):
  N sources → AsyncIterator merge (round-robin) → Python async for loop
            → per-source backpressure via PausePartition

Shared infrastructure:
  - Semaphore-based concurrency limiting (existing)
  - invoke_mode_with_timeout (existing)
  - ExecutionResult (existing)
  - PausePartition/ResumePartition (existing)
  - AsyncMessageStream (streaming handler, v1.1) — Fan-In depends on this
```

---

## 3. Anti-Features

Features explicitly NOT building for v2.0 fan-out/fan-in, with rationale.

| Anti-Feature | Why Avoid | What to Do Instead |
|--------------|-----------|-------------------|
| **Distributed transactions across fan-out sinks** | Two-phase commit is incompatible with at-least-once delivery semantics and adds massive complexity. | Idempotent handlers + at-least-once delivery. Primary ACKs immediately on partial success. |
| **Exactly-once fan-in** | Checkpointing state for exactly-once across multiple async sources is exponential complexity. | At-least-once with per-source deduplication via message keys. |
| **Unbounded fan-out degree** | Without a configurable max, high fan-out degree exhausts memory and file descriptors. | Configurable `max_fan_out: usize` with hard cap. |
| **Global ordering across fan-in sources** | Kafka provides ordering within a partition, not across topics. Enforcing global ordering would require a total order broadcast, which Kafka does not support. | Document that ordering is preserved per source topic+partition only. |
| **Blocking fan-out tasks in Python** | GIL would serialize all fan-out Python calls anyway, making parallelism illusory. Fire-and-forget is acceptable for non-critical sinks. | Fan-out tasks are Tokio async tasks; Python calls still go through spawn_blocking. |

---

## 4. MVP Recommendation

### Fan-Out MVP (Phase 1 of v2.0)

**Priority features to implement first:**

1. **Parallel dispatch via JoinSet** (Table stakes) — Core mechanism; straightforward with existing Tokio.
2. **Configurable fan-out degree** (Table stakes) — Semaphore reuse; prevents resource exhaustion.
3. **Partial success with immediate commit** (Table stakes) — PROJECT.md decision; primary ACKs on primary success regardless of sink failures.
4. **Timeout propagation per fan-out task** (Table stakes) — Reuse existing timeout wrapper.
5. **Per-sink timeout + error classification** (Differentiator, Medium) — Per-sink metrics and error metadata.

**Explicitly defer:**
- Cancellation of straggler fan-out tasks (can be added later)
- Sequential ordered fan-out (rarely needed; users can chain handlers instead)

---

### Fan-In MVP (Phase 2 of v2.0)

**Priority features to implement first:**

1. **Multi-topic Kafka subscription** (Table stakes) — rdkafka supports; extend routing to N topics.
2. **Round-robin merge at dispatcher** (Table stakes) — Interleave by arrival; simplest merge strategy.
3. **Source identification in ExecutionContext** (Table stakes) — `source_topic` field; already designed.
4. **Independent per-source backpressure** (Table stakes) — PausePartition per topic+partition; already exists.
5. **Per-source consumer lag metrics** (Differentiator, Medium) — Per-topic labels on existing lag metrics.

**Explicitly defer:**
- Priority-based source ordering (rare; high complexity)
- Cross-source aggregation windows (separate feature, out of scope for v2.0)

---

## 5. Key Architectural Decisions to Make

| Decision | Options | Recommended | Rationale |
|----------|---------|-------------|-----------|
| **Fan-out offset commit timing** | Commit on primary success only, or wait for all sinks | Commit immediately on primary success | At-least-once delivery; sinks are fire-and-forget side effects |
| **Fan-in merge strategy** | Round-robin, priority queue, full merge | Round-robin | Simplest; no head-of-line blocking; sufficient for most use cases |
| **Fan-out sink failure visibility** | Aggregate error only, or per-sink error in result | Per-sink error metadata in ExecutionResult | Users need to know which sink failed for monitoring |
| **Fan-in source identification** | Topic name only, or topic+partition | topic+partition | Enables fine-grained backpressure and lag metrics per partition |
| **Fan-out degree default** | Unlimited, or N | N = 4 | Prevents resource exhaustion by default; configurable |

---

## 6. Sources

**Evidence basis:**
- PROJECT.md v2.0 Fan-Out/Fan-In decisions (commit on primary success, round-robin fan-in)
- `tokio::task::join_set::JoinSet` — parallel task tracking (Tokio docs)
- Existing `Arc<Semaphore>` per handler key for concurrency limiting (confirmed in PITFALLS.md)
- Existing `PausePartition`/`ResumePartition` for per-partition backpressure (confirmed in v1.1)
- Existing `invoke_mode_with_timeout` for timeout propagation (FEATURES.md v1.1)
- Existing `ExecutionContext` structure — source_topic field needed (confirmed in v1.1 fan-in description)
- rdkafka `client.subscribe(topics)` supports multi-topic subscription (rdkafka docs)

**Confidence:** MEDIUM-HIGH — Fan-Out uses only existing primitives (JoinSet, Semaphore, timeout wrapper). Fan-In depends on streaming handler async iterator infrastructure, which is v1.1 — confirmed shipped.
