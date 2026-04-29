# KafPy Roadmap

**Project:** Rust-Core, Python-Logic Kafka Consumer Framework

---

## Milestones

- [x] **v1.0 MVP** — Phases 1-6 (shipped 2026-04-29)
- [x] **v1.1 Async & Concurrency Hardening** — Phases 7-10 (shipped 2026-04-29)
- [ ] **v2.0 Fan-Out/Fan-In** — Phases 11-15 (in progress)

## Phases

### v2.0 Fan-Out/Fan-In (Phases 11-15)

- [ ] **Phase 11: Fan-Out Core** — JoinSet dispatch, bounded fan-out, partial success dispatch
- [ ] **Phase 12: Fan-Out Offset Commit** — Fan-out tracker, offset gating, per-sink error classification
- [ ] **Phase 13: Fan-Out Python API** — register_fanout API, fan-out metrics, trace context branching
- [ ] **Phase 14: Fan-In Multiplexer** — Multi-topic subscription, round-robin merge, source topic context
- [ ] **Phase 15: Fan-In Integration** — QueueManager wiring, per-source backpressure, register_fanin API

---

## Phase Details

### Phase 11: Fan-Out Core

**Goal:** One message triggers multiple sinks in parallel via JoinSet, with bounded fan-out degree enforced

**Depends on:** Phase 10 (Streaming Handler)

**Requirements:** FANOUT-01, FANOUT-02, FANOUT-03

**Success Criteria** (what must be TRUE):
1. A message dispatched to N registered sinks spawns N parallel handler futures via JoinSet
2. Fan-out degree is capped at configured max_fan_out (default 4, max 64) and enforced at dispatch time
3. Primary sink completion triggers message ACK immediately; sink failures are non-blocking

**Plans:** TBD

**UI hint:** no

---

### Phase 12: Fan-Out Offset Commit

**Goal:** Offset commit gated on all fan-out branch completions; per-sink errors classified and routed

**Depends on:** Phase 11

**Requirements:** FANOUT-04, FANOUT-05

**Success Criteria** (what must be TRUE):
1. Offset is only committed after all fan-out branches complete (tracked by FanOutTracker)
2. Each fan-out branch has its own timeout scope propagated via invoke_mode_with_timeout
3. Per-sink errors carry branch_id and fan_out_id metadata for DLQ routing

**Plans:** TBD

**UI hint:** no

---

### Phase 13: Fan-Out Python API

**Goal:** Python developers can register fan-out groups and observe fan-out metrics/traces

**Depends on:** Phase 12

**Requirements:** FANOUT-06, OBSV-01, OBSV-02

**Success Criteria** (what must be TRUE):
1. Python code can call `consumer.register_fanout(group_name, sink_topics: List[str])` and receive a handler registration
2. Fan-out metrics include fan_out_id and branch_name labels without double-counting throughput
3. Each fan-out branch spawns a child trace span with parent correlation

**Plans:** TBD

**UI hint:** no

---

### Phase 14: Fan-In Multiplexer

**Goal:** One consumer subscribes to multiple topics and merges messages into a single round-robin stream

**Depends on:** Phase 13

**Requirements:** FANIN-01, FANIN-02, FANIN-03

**Success Criteria** (what must be TRUE):
1. A single consumer can subscribe to N topics via rdkafka subscribe([topics])
2. Messages from all subscribed topics are delivered to one handler in round-robin order via tokio::select!
3. ExecutionContext.source_topic field is populated for every fan-in message

**Plans:** TBD

**UI hint:** no

---

### Phase 15: Fan-In Integration

**Goal:** Fan-in multiplexers are wired into QueueManager with per-source backpressure; Python API exposed

**Depends on:** Phase 14

**Requirements:** FANIN-04, FANIN-05, OBSV-03

**Success Criteria** (what must be TRUE):
1. Slow source triggers PausePartition only for that topic+partition; fast sources are unaffected
2. Python code can call `consumer.register_fanin(handler_key, sources: List[str])`
3. Consumer lag metrics include topic label for per-source monitoring

**Plans:** TBD

**UI hint:** no

---

## Phase Dependencies

```
Phase 10 (Streaming Handler) [v1.1]
  └── Phase 11 (Fan-Out Core) [v2.0]
        └── Phase 12 (Fan-Out Offset Commit) [v2.0]
              └── Phase 13 (Fan-Out Python API) [v2.0]
                    └── Phase 14 (Fan-In Multiplexer) [v2.0]
                          └── Phase 15 (Fan-In Integration) [v2.0]
```

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
| 7 | v1.1 | 1/1 | Complete | 2026-04-29 |
| 8 | v1.1 | 2/2 | Complete | 2026-04-29 |
| 9 | v1.1 | 3/3 | Complete | 2026-04-29 |
| 10 | v1.1 | 4/4 | Complete | 2026-04-29 |
| 11 | v2.0 | 0/3 | Not started | - |
| 12 | v2.0 | 0/2 | Not started | - |
| 13 | v2.0 | 0/3 | Not started | - |
| 14 | v2.0 | 0/3 | Not started | - |
| 15 | v2.0 | 0/3 | Not started | - |

**Full milestone history at `.planning/milestones/`**

---
*Last updated: 2026-04-29 after v2.0 requirements defined*
