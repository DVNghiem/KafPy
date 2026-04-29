# Pitfalls: Fan-Out and Fan-In Patterns in KafPy

**Domain:** Adding JoinSet-style parallel dispatch (fan-out) and multi-source merge (fan-in) to an existing Rust+Python Kafka consumer framework
**Context:** v2.0 milestone — extending WorkerPool with fan-out parallelism and multi-source consumption
**Researched:** 2026-04-29
**Confidence:** MEDIUM (combination of well-understood concurrency patterns and project-specific architecture assumptions)

---

## Fan-Out Pitfalls

### FOUT-01: Unbounded fan-out causes resource exhaustion

**What goes wrong:** A message with N sink targets spawns N concurrent handler tasks with no limit. A misconfigured fan-out rule produces 10,000 sink targets, exhausting memory or blocking the Tokio executor.

**Why it happens:** The natural `JoinSet` API accepts any number of tasks. No upper bound on fan-out degree is enforced at registration or dispatch time.

**Consequences:** OOM, event loop starvation for other partitions, crash during rebalance.

**Prevention:** Require an explicit `max_fan_out` configuration (e.g., `ConsumerConfigBuilder::max_fan_out(u32)`) validated at construction time (e.g., 1-64). Enforce the cap in the WorkerPool dispatch layer before spawning. Reject handler registration if fan-out degree exceeds the cap.

**Detection:** Memory usage spikes before OOM. Tokio executor metrics show saturated thread pool. `top`/`htop` shows threads at 100% CPU but no throughput.

**Phase:** Fan-Out Core (first phase)

---

### FOUT-02: Trace context split across branches loses parent relationship

**What goes wrong:** A traceparent header on the inbound Kafka message is propagated to the fan-out dispatcher, but each branch either (a) gets no trace context or (b) gets the original parent span with no way to correlate branches as siblings.

**Why it happens:** W3C traceparent format requires child context creation per branch (`00-parent-traceid-new-spanid-flags`). Without explicit branching logic, all fan-out branches share the original parent span, making distributed traces read as sequential instead of parallel.

**Consequences:** Observability traces show a single long span instead of sibling parallel spans. Root cause analysis becomes impossible for fan-out failures.

**Prevention:** Parse the incoming `traceparent` header, generate a child span ID per fan-out branch, and inject a new `traceparent: 00-{original-trace-id}-{new-span-id}-{flags}` header into each branch's dispatch context. Store the original trace-id to correlate siblings. Emit a synthetic parent span that subsumes all child spans.

**Detection:** Traces show one sequential chain instead of parallel fan structure.

**Phase:** Observability (if tracing already exists)

---

### FOUT-03: Primary message ACKed before all fan-out branches complete

**What goes wrong:** The WorkerPool acks the Kafka offset immediately after the primary handler returns success, but fan-out branches are still running asynchronously via `JoinSet`. When the process crashes before branches complete, the message is not redelivered.

**Why it happens:** `JoinSet` awaits are not integrated into the offset commit fence. The message appears processed before all side-effects are complete.

**Consequences:** At-least-once guarantee is violated for fan-out side-effects. Data inconsistency between primary sink and fan-out sinks.

**Prevention:** Do not ack until `JoinSet.join_all` completes (or all branches reach a terminal state: Ok, DLQ, or exhausted retries). Track branch completion separately from primary acking. If any critical branch fails after max retries, do not ack — let the retry/DLQ path handle it. Add a `fan_out_tracker: Arc<FanOutTracker>` to the WorkerPool that decrements a pending branch counter on each completion.

**Detection:** Fan-out sink lag metrics show entries behind the primary sink's committed offset.

**Phase:** Fan-Out Core

---

### FOUT-04: Partial failure creates duplicate processing on restart

**What goes wrong:** Fan-out to sinks A, B, C. Sink A succeeds. Sink B exhausts retries and goes to DLQ. Sink C is still running when the process shuts down. After restart, the message is redelivered. Sink A now processes it again (double-write). Sink B's DLQ entry is now a duplicate.

**Why it happens:** No shared fan-out execution ID linking the original message to all its branch outcomes. Each branch has independent retry/DLQ state, but the primary offset commit decision does not account for cross-branch state.

**Consequences:** Duplicate writes to sink A, duplicate DLQ entries for sink B, data corruption in fan-out sinks.

**Prevention:** Assign a unique `fan_out_id` (UUID) to each fan-out execution group. Store this ID with each branch's outcome. Do not commit offset until all branches reach terminal state. On restart, check `fan_out_id` to detect and deduplicate re-delivered fan-out groups. Emit a `fan_out_complete` event only when all branches reach terminal state.

**Detection:** Duplicate records in fan-out sinks, duplicate DLQ entries.

**Phase:** Fan-Out DLQ Design

---

### FOUT-05: GIL serialization creates false parallelism impression

**What goes wrong:** Fan-out spawns multiple Python handler tasks in parallel via `JoinSet`, expecting true parallelism. But Python's GIL means only one handler executes Python code at a time, effectively serializing the work despite Tokio tasks running concurrently.

**Why it happens:** PyO3 GIL acquisition via `Python::acquire_gil()` is a global lock. All Python handlers in the same process compete for it. `spawn_blocking` does not change this — it just moves the GIL-acquiring thread to a Rayon worker.

**Consequences:** Throughput is much lower than expected. Fan-out degree increase does not improve throughput after a small threshold. Developers blame the fan-out implementation when the real bottleneck is GIL.

**Prevention:** Document the GIL serialization behavior explicitly in API docs. Recommend CPU-bound Python handlers use multiprocessing or separate processes. Use fan-out primarily for I/O-bound async handlers or Rust-native sinks. Profile with `py-spy --gil` to validate actual parallelism.

**Detection:** CPU profiling shows only one Python handler executing at a time despite multiple Tokio tasks. Fan-out throughput does not scale with fan-out degree beyond ~2-3x.

**Phase:** Fan-Out Python Handler

---

### FOUT-06: DLQ for fan-out partial failure gets wrong message context

**What goes wrong:** Fan-out branch fails after retries. The DLQ entry contains the original Kafka message, but no indication which fan-out branch produced the failure, which original fan-out group it belonged to, or what sibling branch outcomes were.

**Why it happens:** DLQ handoff is designed for single-handler failures. It receives the message and error metadata, but fan-out introduces branch identity and group correlation that the existing DLQ schema does not capture.

**Consequences:** DLQ consumer cannot replay correctly — does not know which branch to skip, which to retry, or whether the original fan-out group should be replayed in full.

**Prevention:** Extend the DLQ metadata schema to include: `fan_out_id`, `branch_name` (sink identifier), `branch_index`, `sibling_outcomes: HashMap<branch_name, Outcome>`. On replay, inspect `sibling_outcomes` to decide whether to replay the full group or individual branches.

**Detection:** DLQ messages lack branch context; replay logic cannot reconstruct fan-out group state.

**Phase:** Fan-Out DLQ Design

---

### FOUT-07: Metrics aggregation double-counts or undercounts

**What goes wrong:** A single Kafka message triggers N fan-out branches. Message count metrics increment N times. Throughput metrics show Nx the actual Kafka throughput. Error rate metrics aggregate across branches, making it unclear whether an error was on the primary path or a secondary sink.

**Why it happens:** Existing metrics are per-message, per-handler. Fan-out requires per-branch accounting and a way to roll up to the original message level.

**Prevention:** Tag all fan-out metrics with `fan_out_id` and `branch_name`. Distinguish `fan_out_primary` from `fan_out_branch` in metric labels. Use a parent `fan_out_id` to roll up branch metrics for the same original message. Emit a synthetic `fan_out_complete` event only when all branches reach terminal state.

**Detection:** Dashboard shows throughput 3-10x expected Kafka ingress rate.

**Phase:** Fan-Out Core + Observability

---

### FOUT-08: JoinSet borrow checker friction with Python GIL

**What goes wrong:** `tokio::task::JoinSet` holds `Future`s that own Python objects (via PyO3). Tokio task-local state does not carry PyO3's `Python` GIL marker across `.await` points, causing "Python not acquired" panics when the future is polled after being stored in `JoinSet`.

**Why it happens:** PyO3 GIL tokens are not `Send`. `JoinSet` requires futures to be `Send`. A future that owns a `Python<'py>` GIL marker cannot cross thread boundaries or be stored in `JoinSet`.

**Consequences:** Build error: "Future is not Send" — or runtime panic: "Poisoned Python GIL acquired in a different thread".

**Prevention:** Do not store `Python<'py>` GIL tokens in the future itself. Acquire GIL inside the async task body (after `spawn`), not before. Use `unsafe { Python::assume_gil() }` only in non-Send contexts. For Python handlers in `JoinSet`, wrap the GIL acquisition inside the spawned task rather than capturing GIL markers in the future chain.

**Detection:** `JoinSet` compilation fails with `Future is not Send` errors or runtime GIL panics.

**Phase:** Fan-Out Core

---

## Fan-In Pitfalls

### FIN-01: Multi-topic subscription with conflicting partition assignments

**What goes wrong:** Fan-in merges messages from topics with different partition counts or replica sets. Partition offsets from Topic A (12 partitions) and Topic B (3 partitions) are tracked in the same queue. Rebalance events reassign partitions differently across topics, breaking the merge assumptions.

**Why it happens:** rdkafka partition assignment is per-topic-group, not global. A consumer in a fan-in group may receive assignments that are not synchronized across topics.

**Consequences:** Messages from one topic pile up while waiting for partition reassignment. Head-of-line blocking in the merge queue. Offset tracking becomes inconsistent.

**Prevention:** Use a single consumer instance per fan-in source topic, track offsets per topic+partition separately, and merge at the queue layer only after offset tracking is complete. Alternatively, use separate consumer instances for each topic and merge their streams in the application layer.

**Detection:** Fan-in queue depth grows for one topic but not others. Rebalance causes message reordering beyond normal partition offset semantics.

**Phase:** Fan-In Core

---

### FIN-02: Round-robin fan-in loses message ordering within key scope

**What goes wrong:** Fan-in merges multiple sources round-robin, which is correct for throughput but loses ordering guarantees for messages with the same partition key. Downstream sinks that expect "last write wins" per key see out-of-order delivery.

**Why it happens:** Round-robin interleaves messages from independent sources without respecting per-key ordering. Kafka guarantees within-partition ordering, but fan-in sources may span multiple partitions across topics.

**Consequences:** Stale reads in fan-in sinks. Idempotency violations if downstream assumes per-key ordering.

**Prevention:** Document that fan-in round-robin does NOT preserve per-key ordering. If ordering is required, use a sharded fan-in approach: hash messages by key and route to key-specific queues that maintain ordering within each key.

**Detection:** Downstream sinks report out-of-order events for the same logical entity.

**Phase:** Fan-In Core

---

### FIN-03: Fan-in source exhaustion causes indefinite blocking

**What goes wrong:** Fan-in waits on multiple async sources. One source closes (EOF, partition revocation) but the merge loop continues waiting on it, causing the merged stream to stall.

**Why it happens:** `tokio::select!` or similar merge loops may not handle source closure correctly. If one branch returns `None` but the loop does not terminate, it blocks waiting on already-closed channels.

**Consequences:** Fan-in stream stalls. Messages from other sources pile up in memory. Graceful shutdown does not complete.

**Prevention:** Implement proper stream termination: when all sources return `None`, signal end-of-stream to the merged output. Use `tokio::select!` with `biased` to prioritize orderly shutdown, or count active sources and decrement on each source's termination.

**Detection:** Process hangs on shutdown. `tokio` task dump shows merge loop blocked on a closed channel.

**Phase:** Fan-In Core

---

## Interaction with Existing Rayon/v1.1 Pitfalls

The existing PITFALLS.md (Rayon integration) documents pitfalls that interact with fan-out:

| Interaction | Issue |
|------------|-------|
| FOUT-01 + Rayon | Unbounded fan-out spawning N tasks each using Rayon for parallel Python preprocessing — memory multiplies |
| FOUT-05 + Rayon | Rayon's GIL-via-spawn_blocking pattern means fan-out Python handlers still serialize through the Tokio blocking pool, not Rayon |
| FOUT-08 + Rayon | Python futures in `JoinSet` have the same GIL `Send` problem as with Rayon — future must not carry GIL markers |

**Key architectural principle from v1.1 that still applies:** Python calls always go through `spawn_blocking` (not Rayon threads directly). Fan-out Python handlers inherit this constraint. The Tokio blocking pool becomes the effective serialization point for all Python handler fan-out calls.

---

## Prevention Strategies

### Enforce bounded fan-out degree at construction time

```rust
// In ConsumerConfigBuilder
pub fn max_fan_out(mut self, n: u32) -> Self {
    assert!(n >= 1 && n <= 64, "max_fan_out must be 1-64");
    self.max_fan_out = Some(n);
    self
}
```

Cap at 64. Enforce in WorkerPool dispatch: if targets > max, reject registration or batch into groups.

### Implement trace branching in the dispatcher layer

Parse `traceparent` header at message intake. For each fan-out branch, generate a child trace context with the same `trace-id` but new `span-id`. Inject as `traceparent` header on each branch dispatch. Emit a synthetic parent span that owns all child spans.

### Serialize offset commit to fan-out completion

Add a `fan_out_tracker: Arc<FanOutTracker>` to the WorkerPool. On fan-out dispatch, register pending branches. On each branch completion, decrement counter. Only call `queue_manager.ack()` when counter reaches zero.

### Extend DLQ schema for fan-out context

```rust
struct FanOutDLQMetadata {
    original_topic: String,
    original_partition: i32,
    original_offset: i64,
    fan_out_id: Uuid,
    branch_name: String,
    branch_index: u32,
    sibling_outcomes: HashMap<String, BranchOutcome>,
}
```

### Document GIL serialization limits

In the Python handler docs and API guide, clearly state that fan-out to Python handlers is serialized by the GIL. Recommend I/O-bound handlers (network calls, DB queries) for fan-out and CPU-bound handlers for multiprocessing or separate processes.

### Add fan-out metrics with proper labeling

```rust
// Metric labels
labels: {
    "handler": handler_name,
    "fan_out_id": uuid,
    "branch_name": sink_name,
    "branch_index": idx,
    "outcome": "ok" | "dlq" | "error",
}
```

Emit `fan_out_spawned`, `fan_out_branch_complete`, `fan_out_all_complete` events.

---

## Which Phase Should Address Each

| Pitfall ID | Description | Phase Suggestion | Notes |
|------------|-------------|-----------------|-------|
| FOUT-01 | Unbounded fan-out resource exhaustion | Fan-Out Core (first) | Enforce max bound before any fan-out dispatch |
| FOUT-02 | Trace context split across branches | Observability (if tracing already exists) | Requires trace infrastructure to already exist |
| FOUT-03 | Primary message ACKed before branches complete | Fan-Out Core | Fan-out tracker must integrate into offset commit |
| FOUT-04 | Partial failure duplicate processing | Fan-Out DLQ Design | DLQ schema changes needed first |
| FOUT-05 | GIL serialization false parallelism | Fan-Out Python Handler | Document and profile to confirm |
| FOUT-06 | DLQ wrong context for fan-out branches | Fan-Out DLQ Design | Requires DLQ metadata schema change |
| FOUT-07 | Metrics double-counting | Fan-Out Core + Observability | Metric tagging required alongside dispatch |
| FOUT-08 | JoinSet borrow checker GIL friction | Fan-Out Core | Rust-level concern; may need architectural workaround |
| FIN-01 | Multi-topic partition conflict | Fan-In Core | Consumer config per topic needed |
| FIN-02 | Round-robin loses key ordering | Fan-In Core | Explicit in docs; sharded variant as future work |
| FIN-03 | Fan-In source exhaustion blocking | Fan-In Core | Stream termination must be correct |

---

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| FOUT-01 Unbounded fan-out | HIGH | Standard tokio JoinSet pattern; well-understood risk |
| FOUT-02 Trace context | MEDIUM | W3C traceparent is standard but implementation specifics depend on existing tracing crate |
| FOUT-03 Offset commit ordering | MEDIUM | Requires verifying existing ack mechanism in WorkerPool |
| FOUT-04 Partial failure duplicates | MEDIUM | Fan-out deduplication strategy needs design decision |
| FOUT-05 GIL serialization | HIGH | PyO3 GIL behavior is well-documented; verified by existing invoke pattern |
| FOUT-06 DLQ schema | MEDIUM | Existing DLQ structure not fully examined; needs verification |
| FOUT-07 Metrics | MEDIUM | Existing metrics structure needs verification |
| FOUT-08 JoinSet GIL | MEDIUM | Requires verifying actual Send requirements for Python futures |
| FIN-01 Multi-topic | MEDIUM | rdkafka behavior in multi-topic fan-in not verified in detail |
| FIN-02 Ordering | HIGH | Round-robin semantics well-understood; ordering loss is inherent |
| FIN-03 Source exhaustion | HIGH | Tokio stream pattern; well-understood pitfall |

---

## Gaps Needing Phase-Specific Research

1. **Existing DLQ schema** — FOUT-04 and FOUT-06 depend on understanding the current DLQ metadata structure. Requires reading `src/dlq/` files.
2. **Existing metrics crate and labeling scheme** — FOUT-07 requires knowing current Prometheus metric naming and labeling conventions.
3. **Existing trace infrastructure** — FOUT-02 requires knowing which tracing crate (tracing, opentelemetry, etc.) and current header injection pattern.
4. **JoinSet Send requirement in current PythonHandler** — FOUT-08 requires verifying whether current Python future type is actually Send, and if not, what the migration path is.
5. **rdkafka multi-topic consumer behavior** — FIN-01 requires verifying whether a single consumer can subscribe to multiple topics and maintain partition offset tracking correctly.

---

## Sources

- [Tokio JoinSet documentation](https://docs.rs/tokio/latest/tokio/task/struct.JoinSet.html) — Send requirements for stored futures
- [PyO3 GIL guide](https://pyo3.rs/latest/guide/python.html#the-gil) — GIL acquisition patterns
- [W3C traceparent specification](https://www.w3.org/TR/trace-context-1/) — Child span creation per branch
- [rdkafka-rs multi-topic subscription](https://docs.rs/rdkafka/latest/rdkafka/) — partition assignment per topic group
- [Tokio stream fan-in patterns](https://docs.rs/tokio-stream/latest/tokio_stream/) — merge and zip combinators
