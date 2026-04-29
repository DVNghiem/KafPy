# Requirements: KafPy v2.0 Fan-Out/Fan-In

**Defined:** 2026-04-29
**Core Value:** Python developers can write Kafka message handlers easily while Rust controls the hard runtime problems (concurrency, backpressure, retries, DLQ, offset tracking, graceful shutdown).

## v2.0 Requirements

Requirements for milestone v2.0. Each maps to roadmap phases.

### Fan-Out (FANOUT)

One message triggers multiple async handlers/sinks in parallel via JoinSet.

- [ ] **FANOUT-01**: One message dispatched to multiple sinks in parallel via JoinSet
- [ ] **FANOUT-02**: `max_fan_out` config enforced at dispatch (bounded degree, max 64)
- [ ] **FANOUT-03**: Fan-out partial success — primary ACKed immediately, sink failures logged/tracked but non-blocking
- [ ] **FANOUT-04**: Per-fan-out-task timeout propagated to each sink handler
- [ ] **FANOUT-05**: Per-sink error classification and DLQ routing (branch_id + fan_out_id in metadata)
- [ ] **FANOUT-06**: `register_fanout(group_name, sink_topics: Vec<String>)` Python API

### Fan-In (FANIN)

Multiple async sources merged into single handler (round-robin).

- [ ] **FANIN-01**: Multi-topic Kafka subscription via rdkafka `subscribe(&[topics])`
- [ ] **FANIN-02**: Round-robin merge via `tokio::select!` biased loop — messages interleaved by arrival
- [ ] **FANIN-03**: `ExecutionContext.source_topic` field populated for fan-in handler
- [ ] **FANIN-04**: Per-source independent backpressure (PausePartition per topic+partition)
- [ ] **FANIN-05**: `register_fanin(handler_key, sources: Vec<String>)` Python API

### Observability (OBSV)

Fan-out/fan-in metrics and tracing additions.

- [ ] **OBSV-01**: Fan-out metrics tagged with `fan_out_id`, `branch_name` (not double-count throughput)
- [ ] **OBSV-02**: Trace context branching — child span per fan-out branch with parent correlation
- [ ] **OBSV-03**: Per-source consumer lag metrics (topic label) for fan-in

## v3 Requirements (Deferred)

Future release. Tracked but not in current roadmap.

### Advanced Fan-Out

- **FANOUT-07**: Sequential ordered fan-out (await each sink before next)
- **FANOUT-08**: Per-sink timeout vs shared timeout pool
- **FANOUT-09**: Fire-and-forget dispatch (spawn and track in background, no await)

### Advanced Fan-In

- **FANIN-06**: Priority-based source ordering (weighted round-robin)
- **FANIN-07**: Cross-source aggregation window (buffer N messages across sources before processing)
- **FANIN-08**: Source health monitoring (per-topic lag alerting)

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| Distributed transactions across fan-out sinks | Two-phase commit incompatible with at-least-once semantics |
| Exactly-once fan-in | Checkpointing state complexity explodes |
| Unbounded fan-out | Resource exhaustion risk — max_fan_out enforced |
| Global ordering across fan-in sources | Kafka only guarantees within-partition ordering |
| Blocking fan-out tasks in Python | GIL serializes anyway; use async handlers |
| Fan-out + Fan-In combined | Complex interaction, defer to future |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| FANOUT-01 | Phase 11 | Pending |
| FANOUT-02 | Phase 11 | Pending |
| FANOUT-03 | Phase 11 | Pending |
| FANOUT-04 | Phase 12 | Pending |
| FANOUT-05 | Phase 12 | Pending |
| FANOUT-06 | Phase 13 | Pending |
| FANIN-01 | Phase 14 | Pending |
| FANIN-02 | Phase 14 | Pending |
| FANIN-03 | Phase 14 | Pending |
| FANIN-04 | Phase 15 | Pending |
| FANIN-05 | Phase 15 | Pending |
| OBSV-01 | Phase 13 | Pending |
| OBSV-02 | Phase 13 | Pending |
| OBSV-03 | Phase 15 | Pending |

**Coverage:**
- v2.0 requirements: 13 total
- Mapped to phases: 13
- Unmapped: 0

---
*Requirements defined: 2026-04-29*
*Last updated: 2026-04-29 after v2.0 requirements definition*