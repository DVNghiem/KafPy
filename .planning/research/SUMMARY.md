# Research Summary: KafPy v1.8 Graceful Shutdown & Rebalance Handling

**Synthesized:** 2026-04-19
**Based on:** STACK.md, FEATURES.md, ARCHITECTURE.md, PITFALLS.md

---

## Executive Summary

KafPy v1.8 adds graceful shutdown coordination and rebalance event handling to prevent data loss, duplicate processing, and infinite hangs during consumer lifecycle transitions. The existing codebase has solid foundations (CancellationToken, OffsetTracker with highest-contiguous-offset semantics, WorkerPool drain), but lacks orchestration across components and rebalance awareness.

**Key findings:**
- **No new crate dependencies required** -- all primitives exist in current Cargo.toml
- **Missing core piece:** `ShutdownCoordinator` to orchestrate 3-phase shutdown (stop consumer, drain workers, commit offsets)
- **Missing rebalance infrastructure:** `RebalanceHandler` + `PartitionOwnership` to respond to assignment changes
- **Critical gap:** No `rd_kafka_consumer_close()` call -- consumer stays in group until session.timeout expires
- **Top risk:** Circular wait between dispatcher and workers during shutdown (deadlock)

---

## Key Findings

### From STACK.md

| Component | Status | Notes |
|-----------|--------|-------|
| `tokio_util::CancellationToken` | Already present | Used in WorkerPool; needed for hierarchical shutdown |
| `thiserror = "2.0.17"` | Already present | Extend `coordinator/error.rs` for shutdown/rebalance errors |
| `parking_lot = "0.12"` | Already present | Use for PartitionOwnership (not hot path) |
| `rdkafka = "0.38"` | Already present | Rebalance via `ConsumerContext` trait (0.38 API) |

**Three new modules needed:**
1. `src/coordinator/shutdown.rs` -- ShutdownCoordinator with phase tracking
2. `src/coordinator/rebalance.rs` -- RebalanceHandler + PartitionOwnership
3. Extend `src/coordinator/error.rs` -- shutdown/rebalance error variants

### From FEATURES.md

**Table Stakes (must have for v1.8):**
- Coordinated shutdown sequence (stop consumer -> drain workers -> commit -> close)
- `ConsumerRunner::close()` calling `rd_kafka_consumer_close()`
- Basic rebalance callback (ASSIGN/REVOKE via ConsumerContext)
- Partition ownership state enum (Assigned, Processing, Draining, Revoked)

**Differentiators (defer to v1.9+):**
- Drain timeout with graceful degradation
- Python-visible rebalance callbacks
- Incremental rebalance (KIP-848 / Kafka 4.0+)
- Rebalance-induced DLQ flush

**Anti-patterns to avoid:**
- Auto-commit during shutdown (KafPy already avoids via manual offset management)
- Blocking shutdown with no timeout (workers can hang indefinitely)
- Global consumer close without per-partition state tracking

### From ARCHITECTURE.md

**Shutdown sequence (correct order):**
```
Phase 1: Signal stop
  - runner.stop() (broadcast)
  - abort(dispatcher_handle)
  - abort(committer_handle)
  - drain_token.cancel()

Phase 2: Drain inflight
  - pool.shutdown() (waits with timeout)
  - flush_pending_retries()

Phase 3: Finalize
  - offset_tracker.graceful_shutdown()
  - phase = Done
```

**Rebalance detection approach:** Compare `consumer.assignment()` on each `consumer.recv()` iteration. Emit `RebalanceEvent` via broadcast channel when assignment changes.

**New component structure:**
- `ShutdownCoordinator` -- owns phase enum, cancellation tokens, handles
- `RebalanceHandler` -- listens for events, coordinates pause/resume/drain
- `PartitionOwnership` -- tracks assignment state with RwLock

### From PITFALLS.md

**Top 5 pitfalls to avoid:**

| Priority | Pitfall | Prevention |
|----------|---------|------------|
| CRITICAL | Circular wait deadlock (dispatcher waiting for workers, workers waiting for dispatcher) | Correct shutdown order: dispatcher stops first, then workers drain |
| CRITICAL | Partition revoked before offset commit (data loss/duplication) | Immediate commit trigger on rebalance; ownership check before `record_ack` |
| CRITICAL | Lost in-flight messages on shutdown | Drain-before-cancel: wait for idle, then cancel |
| HIGH | Boolean flag soup creating impossible states | Explicit `LifecycleState` enum with validated transitions |
| HIGH | Heavy rebalance callbacks causing rebalance storms | Fast-path: mark revoking + pause; background: commit + drain |

---

## Implications for Roadmap

### Recommended Phase Structure

**Phase 33: ShutdownCoordinator (Foundation)**
- Create `ShutdownCoordinator` with `ShutdownPhase` enum (Running, Draining, Finalizing, Done)
- Wire `ShutdownCoordinator::shutdown()` to `Consumer::stop()`
- Define correct shutdown order (dispatcher first, then workers, then commit)
- Add drain timeout (default 30s) with force-abort fallback
- Add `RetryCoordinator::flush_pending_retries()` method
- **Pitfalls to avoid:** Pitfalls 4 (deadlock), 7 (boolean flags), 14 (no timeout), 15 (has_terminal blocking)

**Phase 34: Rebalance Handling**
- Create `PartitionOwnership` struct with `OwnerState` enum (Owned, Revoked, PendingAssign)
- Create `RebalanceHandler` that listens for `RebalanceEvent` broadcast
- Modify `ConsumerRunner` to emit rebalance events on assignment change
- Implement `ConsumerDispatcher::on_rebalance()` (pause revoked, resume owned)
- Add ownership check in `OffsetCoordinator::record_ack()` (safety gate)
- **Pitfalls to avoid:** Pitfalls 1 (premature revocation), 2 (unsafe offset), 3 (race), 8 (heavy callbacks), 9 (in-flight can't complete), 13 (parking_lot in async)

**Phase 35: Integration & Hardening**
- Integrate `ShutdownCoordinator` with all spawned tasks (committer, dispatcher, rebalance handler)
- Add `BatchAccumulator::revoke_partition()` for targeted drain
- Add `biased` to all shutdown/rebalance-sensitive `select!` loops
- Add structured log warnings for rebalance events, drain timeouts, ack for non-owned partition
- **Pitfalls to avoid:** Pitfall 10 (OffsetCommitter no shutdown), 11 (BatchAccumulator not drained), 12 (missing biased)

### Build Order Rationale

1. **ShutdownCoordinator first** -- establishes orchestration foundation; other components depend on it
2. **PartitionOwnership second** -- needed by RebalanceHandler; independent of shutdown
3. **RebalanceHandler third** -- depends on PartitionOwnership; the most complex new component
4. **Integration last** -- wires everything together; validates against existing code paths

---

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | All primitives confirmed in existing Cargo.toml; no new deps needed |
| Features | HIGH | Table stakes well-defined from librdkafka docs and other client libraries |
| Architecture | MEDIUM-HIGH | Patterns clear, but rdkafka rebalance detection granularity needs verification |
| Pitfalls | MEDIUM-HIGH | Based on code analysis and documented patterns; some edge cases theoretical |

### Gaps to Address

1. **rdkafka rebalance detection granularity:** Does rdkafka 0.38 expose assignment vs revocation type at Rust level, or only resulting diff?
2. **OffsetCommitter abort safety:** Idempotency of `store_offset` + `commit` when aborted mid-operation
3. **Python handler timeout enforcement:** Need to verify `spawn_blocking` can enforce timeouts on Python code

---

## Research Flags

**Needs research during planning:**
- Phase 34: rdkafka `ConsumerContext` rebalance callback API exact signature for 0.38
- Phase 34: Per-partition worker mapping for targeted cancellation

**Standard patterns (skip research):**
- Phase 33: Tokio CancellationToken hierarchy -- well-documented
- Phase 33: Shutdown sequence ordering -- standard pattern from librdkafka INTRODUCTION.md
- Phase 35: `biased` select -- already used correctly in existing code

---

## Sources

- librdkafka INTRODUCTION.md -- consumer group termination sequence, KIP-848, static membership
- librdkafka STATISTICS.md -- consumer_lag_stored, offset commit semantics
- rdkafka ConsumerContext trait (0.38 API) -- rebalance callbacks
- Tokio CancellationToken docs -- hierarchical shutdown
- Kafka consumer group rebalance protocol -- session.timeout semantics
- Spring Kafka, confluent-kafka-python, segmentio/kafka-go -- pattern reference

---

*Summary synthesized from parallel research: STACK.md, FEATURES.md, ARCHITECTURE.md, PITFALLS.md*
*For: gsd-roadmapper agent*
