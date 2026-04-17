---
phase: 14-offsetcoordinator-trait
plan: '01'
type: execute
wave: 1
depends_on: [11]
files_modified:
  - src/coordinator/offset_coordinator.rs
  - src/coordinator/offset_tracker.rs
  - src/coordinator/mod.rs
  - src/worker_pool/mod.rs
autonomous: true
requirements:
  - WORKER-01
  - WORKER-02
  - WORKER-03
---

# Phase 14 Plan: OffsetCoordinator Trait

## Context

Phase 11 built `OffsetTracker` with `ack`, `mark_failed`, `highest_contiguous`, `should_commit`, `committed_offset` methods.
Phase 14 abstracts these into an `OffsetCoordinator` trait for injection into `WorkerPool`.

## Tasks

### Task 1: Define `OffsetCoordinator` trait

<read_first>
- src/coordinator/mod.rs
- src/coordinator/offset_tracker.rs
</read_first>

<acceptance_criteria>
- grep "pub trait OffsetCoordinator" src/coordinator/offset_coordinator.rs
- grep "fn record_ack" src/coordinator/offset_coordinator.rs
- grep "fn mark_failed" src/coordinator/offset_coordinator.rs
- grep "fn graceful_shutdown" src/coordinator/offset_coordinator.rs
- grep "Send \+ Sync" src/coordinator/offset_coordinator.rs
</acceptance_criteria>

<action>
Create `src/coordinator/offset_coordinator.rs` with:

```rust
/// Trait abstracting offset tracking operations for the worker pool.
///
/// Decouples WorkerPool from concrete OffsetTracker implementation.
/// Implementations must be thread-safe (Send + Sync) as the trait is
/// used across tokio task boundaries via Arc.
pub trait OffsetCoordinator: Send + Sync {
    /// Records a successful ack for the given topic-partition at `offset`.
    fn record_ack(&self, topic: &str, partition: i32, offset: i64);

    /// Marks `offset` as failed for the given topic-partition.
    ///
    /// Does NOT advance committed offset — gap remains until retry succeeds.
    fn mark_failed(&self, topic: &str, partition: i32, offset: i64);

    /// Called when the worker pool is shutting down gracefully.
    ///
    /// Phase 15 will use this to commit highest contiguous offsets before exit.
    /// Phase 14 implementation is a no-op.
    fn graceful_shutdown(&self);
}
```
</action>

### Task 2: Export `OffsetCoordinator` from coordinator module

<read_first>
- src/coordinator/mod.rs
</read_first>

<acceptance_criteria>
- grep "offset_coordinator" src/coordinator/mod.rs
- grep "OffsetCoordinator" src/coordinator/mod.rs
</acceptance_criteria>

<action>
Add to `src/coordinator/mod.rs`:
```rust
pub mod offset_coordinator;
pub use offset_coordinator::OffsetCoordinator;
```
</action>

### Task 3: Implement `OffsetCoordinator` on `OffsetTracker`

<read_first>
- src/coordinator/offset_tracker.rs
</read_first>

<acceptance_criteria>
- grep "impl OffsetCoordinator for OffsetTracker" src/coordinator/offset_tracker.rs
- grep "fn record_ack.*OffsetTracker" src/coordinator/offset_tracker.rs
- grep "fn graceful_shutdown" src/coordinator/offset_tracker.rs
- cargo build --lib 2>&1 | grep -i error | head -20
</acceptance_criteria>

<action>
Add to end of `src/coordinator/offset_tracker.rs`:

```rust
impl OffsetCoordinator for OffsetTracker {
    fn record_ack(&self, topic: &str, partition: i32, offset: i64) {
        self.ack(topic, partition, offset);
    }

    fn mark_failed(&self, topic: &str, partition: i32, offset: i64) {
        OffsetTracker::mark_failed(self, topic, partition, offset);
    }

    fn graceful_shutdown(&self) {
        // Phase 15: commit highest contiguous per topic-partition
    }
}
```

Also add to the imports at top of `offset_tracker.rs`:
```rust
use super::OffsetCoordinator;
```
</action>

### Task 4: Update `WorkerPool::new()` to accept `Arc<dyn OffsetCoordinator>`

<read_first>
- src/worker_pool/mod.rs
</read_first>

<acceptance_criteria>
- grep "offset_coordinator: Arc<dyn OffsetCoordinator>" src/worker_pool/mod.rs
- grep "dyn OffsetCoordinator" src/worker_pool/mod.rs
- cargo build --lib 2>&1 | grep -i error | head -20
- cargo test worker_pool 2>&1 | tail -20
</acceptance_criteria>

<action>
In `src/worker_pool/mod.rs`:

1. Add import:
```rust
use crate::coordinator::OffsetCoordinator;
```

2. Add field to `WorkerPool` struct:
```rust
pub struct WorkerPool {
    join_set: JoinSet<()>,
    pub(crate) shutdown_token: CancellationToken,
    offset_coordinator: Arc<dyn OffsetCoordinator>,
}
```

3. Update `worker_loop` signature to accept `offset_coordinator: Arc<dyn OffsetCoordinator>`

4. Update `new()` to accept `offset_coordinator: Arc<dyn OffsetCoordinator>` and store it

5. Pass `Arc::clone(&offset_coordinator)` into each `worker_loop` call

6. Update tests in `#[cfg(test)]` to pass a coordinator (use `Arc::new(OffsetTracker::new()) as Arc<dyn OffsetCoordinator>`)
</action>

## Verification

```bash
cargo build --lib 2>&1 | grep -i error
cargo test --lib offset 2>&1 | tail -20
cargo test --lib worker_pool 2>&1 | tail -20
cargo clippy --lib -- -D warnings 2>&1 | grep -i error
```

## must_haves

- [ ] `OffsetCoordinator` trait defined with `record_ack`, `mark_failed`, `graceful_shutdown`
- [ ] `OffsetTracker` implements `OffsetCoordinator` via delegation
- [ ] `WorkerPool::new()` accepts `Arc<dyn OffsetCoordinator>` as parameter
- [ ] `WorkerPool` struct stores `Arc<dyn OffsetCoordinator>`
- [ ] All existing tests pass with new signatures
