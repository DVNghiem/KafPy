# Requirements: KafPy v2.0

**Defined:** 2026-04-20
**Core Value:** High-performance Rust Kafka client with idiomatic Python API

## v1 Requirements

### Duplication Elimination (DUP)

- [ ] **DUP-01**: Extract `handle_execution_failure()` helper to reduce error/DLQ branch duplication in worker_pool
- [ ] **DUP-02**: Extract `message_to_pydict()` helper to eliminate copied Python conversion logic
- [ ] **DUP-03**: Extract `flush_partition_batch()` helper to consolidate batch flush boilerplate (6 repetitions)
- [ ] **DUP-04**: Verify NoopSink duplication between worker_pool and observability — consolidate if redundant

### Module Split — worker_pool/ (SPLIT-A)

- [ ] **SPLIT-A-01**: Extract `PartitionAccumulator` from worker_pool/mod.rs into new `batch/` module
- [ ] **SPLIT-A-02**: Extract `batch_worker_loop()` to `batch.rs` in worker_pool
- [ ] **SPLIT-A-03**: Extract `worker_loop()` to `worker.rs` in worker_pool
- [ ] **SPLIT-A-04**: Extract `WorkerPool` struct to `pool.rs` in worker_pool
- [ ] **SPLIT-A-05**: Rename `PartitionAccumulator` to clarify its general-purpose nature
- [ ] **SPLIT-A-06**: Move `BatchAccumulator` to `python/batch.rs` (handler-mode specific)

### Module Split — dispatcher/ + runtime/ (SPLIT-B)

- [ ] **SPLIT-B-01**: Extract `ConsumerDispatcher` from dispatcher/mod.rs to own file
- [ ] **SPLIT-B-02**: Extract `RuntimeBuilder` from pyconsumer.rs to new `runtime/` module
- [ ] **SPLIT-B-03**: Verify PyO3 integration with new runtime/ builder pattern
- [ ] **SPLIT-B-04**: Ensure runtime assembly order preserved (config → tracker → dispatcher → handler → worker → committer)

### State Machine Extraction (STATE)

- [ ] **STATE-01**: Replace `Option<OwnedMessage>` implicit state in worker_loop with explicit `WorkerState` enum
- [ ] **STATE-02**: Replace `bool` backpressure flag in batch_worker_loop with explicit `BatchState` enum
- [ ] **STATE-03**: Replace `bool`-like `ShutdownPhase` struct with explicit `ShutdownState` enum (follows PartitionState pattern)
- [ ] **STATE-04**: Replace `bool`-like RetryCoordinator state with explicit `RetryState` enum
- [ ] **STATE-05**: Verify all state enum variants have exhaustive match arms (no wildcard `_`)

### Module Split — coordinator/ (SPLIT-C)

- [ ] **SPLIT-C-01**: Extract `OffsetTracker` + `OffsetCommitter` from coordinator/ to new `offset/` module
- [ ] **SPLIT-C-02**: Extract `ShutdownCoordinator` from coordinator/ to new `shutdown/` module
- [ ] **SPLIT-C-03**: Move `RetryCoordinator` from coordinator/ to existing `retry/` module (colocates with RetryPolicy)
- [ ] **SPLIT-C-04**: Update all import paths after module moves
- [ ] **SPLIT-C-05**: Verify offset commit semantics unchanged after split (highest-contiguous-offset, has_terminal gating)

### Type Safety (TYPES)

- [ ] **TYPES-01**: Create `HandlerId` newtype wrapper (`struct HandlerId(String)`) replacing `String` type alias
- [ ] **TYPES-02**: Verify `HandlerId` vs topic distinction — if always equal, consider removing newtype complexity
- [ ] **TYPES-03**: Audit `RoutingContext` and all dispatch paths to use `HandlerId` type
- [ ] **TYPES-04**: Consolidate error types — unified error module replacing scattered `error.rs` / `errors.rs`
- [ ] **TYPES-05**: Verify Send+Sync guarantees after all refactoring via compile-time assertions

### Code Quality Gates (GATES)

- [ ] **GATES-01**: Run `cargo clippy --all-targets` with `cognitive_complexity` and `type_complexity` lints — baseline before refactor
- [ ] **GATES-02**: Run `cargo test --lib` — all tests pass after each phase
- [ ] **GATES-03**: Run benchmark baseline — no regression after refactor
- [ ] **GATES-04**: Run `cargo geiger` — unsafe code usage isolated and minimal
- [ ] **GATES-05**: Verify no exported API changes (PyO3 boundary stable)

## v2 Requirements

### Structural Cleanup

- **ARCH-01**: Evaluate `WorkerPoolState` / `WorkerState` location — define in worker_pool/state.rs with re-exports
- **ARCH-02**: Evaluate `RuntimeSnapshotTask` location — runtime/ vs observability/
- **ARCH-03**: Move `failure/tests.rs` to `tests/` at crate root or inline as `#[cfg(test)]` blocks

## Out of Scope

| Feature | Reason |
|---------|--------|
| Inter-module boundary changes | Module boundaries are well-designed; refactoring is intra-module |
| PyO3 boundary changes | GIL boundary discipline is correct; no changes needed |
| Behavioral changes | Refactor preserves behavior; no new features |
| Performance optimization | Focus is code quality, not performance; benchmarks confirm no regression |
| New public API surface | Public API is stable; no additions in v2.0 |

## Traceability

| Requirement | Phase | Status |
|-------------|-------|--------|
| DUP-01 | Phase 1 | Pending |
| DUP-02 | Phase 1 | Pending |
| DUP-03 | Phase 1 | Pending |
| DUP-04 | Phase 1 | Pending |
| SPLIT-A-01 | Phase 2 | Pending |
| SPLIT-A-02 | Phase 2 | Pending |
| SPLIT-A-03 | Phase 2 | Pending |
| SPLIT-A-04 | Phase 2 | Pending |
| SPLIT-A-05 | Phase 2 | Pending |
| SPLIT-A-06 | Phase 2 | Pending |
| SPLIT-B-01 | Phase 3 | Pending |
| SPLIT-B-02 | Phase 3 | Pending |
| SPLIT-B-03 | Phase 3 | Pending |
| SPLIT-B-04 | Phase 3 | Pending |
| STATE-01 | Phase 4 | Pending |
| STATE-02 | Phase 4 | Pending |
| STATE-03 | Phase 4 | Pending |
| STATE-04 | Phase 4 | Pending |
| STATE-05 | Phase 4 | Pending |
| SPLIT-C-01 | Phase 5 | Pending |
| SPLIT-C-02 | Phase 5 | Pending |
| SPLIT-C-03 | Phase 5 | Pending |
| SPLIT-C-04 | Phase 5 | Pending |
| SPLIT-C-05 | Phase 5 | Pending |
| TYPES-01 | Phase 6 | Pending |
| TYPES-02 | Phase 6 | Pending |
| TYPES-03 | Phase 6 | Pending |
| TYPES-04 | Phase 6 | Pending |
| TYPES-05 | Phase 6 | Pending |
| GATES-01 | All phases | Pending |
| GATES-02 | All phases | Pending |
| GATES-03 | All phases | Pending |
| GATES-04 | All phases | Pending |
| GATES-05 | All phases | Pending |

**Coverage:**
- v1 requirements: 31 total
- Mapped to phases: 31
- Unmapped: 0 ✓

---
*Requirements defined: 2026-04-20*
*Last updated: 2026-04-20 after v2.0 research synthesis*
