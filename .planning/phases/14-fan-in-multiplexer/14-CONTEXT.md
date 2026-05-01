# Phase 14: Fan-In Multiplexer - Context

**Gathered:** 2026-05-01
**Status:** Ready for planning

<domain>
## Phase Boundary

One consumer subscribes to multiple topics via rdkafka `subscribe([topics])` and merges messages into a single round-robin stream via `tokio::select!`. `ExecutionContext.source_topic` field is populated for every fan-in message.

**Requirements:** FANIN-01 (multi-topic subscribe), FANIN-02 (round-robin merge via tokio::select!), FANIN-03 (source_topic field in ExecutionContext)

**Success Criteria:**
1. A single consumer can subscribe to N topics via rdkafka `subscribe([topics])`
2. Messages from all subscribed topics are delivered to one handler in round-robin order via `tokio::select!`
3. `ExecutionContext.source_topic` field is populated for every fan-in message

</domain>

<decisions>
## Implementation Decisions

### Multi-Topic Subscription (FANIN-01)
- **D-01:** Use `rdkafka::consumer::Consumer::subscribe(&topics)` with a `TopicPartitionList`. Single subscription call, multiple topics.
- **D-02:** Topics passed as `Vec<String>` to `register_fanin(handler_key, sources: Vec<String>)` — same pattern as FANIN-05 API.

### Round-Robin Merge (FANIN-02)
- **D-03:** Use `tokio::select!` with biased polling to interleave messages by arrival order — messages from all topics are delivered as they arrive, not in strict topic order.
- **D-04:** No ordering guarantee across sources (consistent with v2.0 decision: Fan-In round-robin, no ordering guarantee). Fast source doesn't wait for slow source.
- **D-05:** Each poll cycle iterates all topic partitions fairly — no starvation of any single topic.

### Source Topic Context (FANIN-03)
- **D-06:** `ExecutionContext.source_topic: String` field — populated for every fan-in message with the topic the message arrived from.
- **D-07:** `ExecutionContext.fan_in_id: Option<u64>` — set when the handler was registered via `register_fanin`. This identifies the fan-in group for metrics.

### Claude's Discretion
- Exact poll loop structure in worker_pool/streaming_loop.rs or new fan_in_loop.rs — planner decides
- How to handle rebalance during round-robin processing — planner decides (reuse existing rebalance-safe patterns)

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Requirements
- `.planning/REQUIREMENTS.md` §FANIN — FANIN-01, FANIN-02, FANIN-03

### Prior Phases
- `.planning/phases/10-streaming-handler/10-CONTEXT.md` — streaming handler state machine, tokio::select! patterns
- `.planning/phases/13-fan-out-python-api/13-CONTEXT.md` — FanOutConfig, handler registration pattern

### Implementation
- `src/dispatcher/consumer_dispatcher.rs` — existing Kafka consumer dispatch loop
- `src/python/context.rs` — ExecutionContext with source_topic field
- `src/python/handler.rs` — PythonHandler invoke patterns

</canonical_refs>

<specifics>
## Specific Ideas

No external specs referenced. Implementation decisions fully captured above.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

---

*Phase: 14-fan-in-multiplexer*
*Context gathered: 2026-05-01*