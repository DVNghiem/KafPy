# Requirements: KafPy v1.8 — Graceful Shutdown & Rebalance Handling

**Defined:** 2026-04-19
**Core Value:** High-performance Rust Kafka client with idiomatic Python API

## v1 Requirements

### Lifecycle State (LSC-01)

- [x] **LSC-01**: ShutdownCoordinator owns ShutdownPhase enum (Running, Draining, Finalizing, Done)
- [x] **LSC-02**: Consumer::stop() signals ShutdownCoordinator which orchestrates correct shutdown order
- [x] **LSC-03**: Shutdown order: dispatcher stop → worker drain → offset commit → component close
- [x] **LSC-04**: ShutdownCoordinator drain timeout (default 30s) with force-abort fallback
- [x] **LSC-05**: ConsumerRunner::close() calls rd_kafka_consumer_close() on drop

### Rebalance Handling (RB-01)

- [ ] **RB-01**: PartitionOwnership tracks OwnerState per TP: Owned, Revoked, PendingAssign
- [ ] **RB-02**: RebalanceHandler emits RebalanceEvent (Assign/Revoke/Error) via broadcast channel
- [ ] **RB-03**: ConsumerDispatcher pauses revoked partitions, resumes assigned partitions
- [ ] **RB-04**: record_ack() checks PartitionOwnership before recording — prevents offset advance on revoked partitions
- [ ] **RB-05**: RetryCoordinator::flush_pending_retries() sends pending messages to DLQ before shutdown

### Integration (INT-01)

- [ ] **INT-01**: ShutdownCoordinator wired to OffsetCommitter, Dispatcher, WorkerPool, RebalanceHandler
- [ ] **INT-02**: BatchAccumulator::revoke_partition() drains messages for revoked TP
- [ ] **INT-03**: All lifecycle transitions observable via structured logging (tracing events)
- [ ] **INT-04**: All select! loops in shutdown/rebalance paths use `biased` for deterministic ordering

## Out of Scope

| Feature | Reason |
|---------|--------|
| Python-visible rebalance callbacks | Opt-in feature, defer to v1.9 |
| Drain timeout per-handler | Complex configuration, defer |
| Incremental rebalance (KIP-848) | Kafka 4.0+ concern, future |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| LSC-01 through LSC-05 | Phase 33 | Complete |
| RB-01 through RB-05 | Phase 34 | Pending |
| INT-01 through INT-04 | Phase 35 | Pending |

**Coverage:**
- v1 requirements: 14 total
- Mapped to phases: 14
- Unmapped: 0 ✓

---
*Requirements defined: 2026-04-19*
