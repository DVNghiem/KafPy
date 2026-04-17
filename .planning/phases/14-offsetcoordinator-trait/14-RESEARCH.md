---
phase: 14-offsetcoordinator-trait
tags: [trait, offset-tracking, worker-pool, dependency-injection]

# Phase 14: OffsetCoordinator Trait — Research

**Research date:** 2026-04-17
**Status:** Complete — ready for planning

---

## 1. What Is Being Built

A trait `OffsetCoordinator` that abstracts offset tracking operations, implemented by `OffsetTracker`. The trait is injected into `WorkerPool` at construction via `Arc<dyn OffsetCoordinator>`.

**Purpose:** Decouple the worker pool from the concrete `OffsetTracker` implementation, enabling future coordinators (mock for tests, different commit strategies, etc.).

---

## 2. Relevant Codebase State

### OffsetTracker (`src/coordinator/offset_tracker.rs`)

Current public API:
- `fn ack(&self, topic: &str, partition: i32, offset: i64)`
- `fn highest_contiguous(&self, topic: &str, partition: i32) -> Option<i64>`
- `fn should_commit(&self, topic: &str, partition: i32) -> bool`
- `fn mark_failed(&self, topic: &str, partition: i32, offset: i64)`
- `fn committed_offset(&self, topic: &str, partition: i32) -> i64`

Implementation uses `parking_lot::Mutex` for thread-safety. All methods take `&self`.

### WorkerPool (`src/worker_pool/mod.rs`)

Current constructor signature (lines 132-164):
```rust
pub fn new(
    n_workers: usize,
    receivers: Vec<mpsc::Receiver<OwnedMessage>>,
    handler: Arc<PythonHandler>,
    executor: Arc<dyn Executor>,
    queue_manager: Arc<QueueManager>,
    shutdown_token: CancellationToken,
) -> Self
```

The `worker_loop` function (lines 31-112) currently:
- Calls `queue_manager.ack(&msg.topic, 1)` on `ExecutionResult::Ok` (line 60)
- Does NOT call any offset tracking methods

### ExecutionResult (`src/python/execution_result.rs`)

Three variants:
- `Ok` — handler succeeded
- `Error { exception, traceback }` — handler raised exception
- `Rejected { reason }` — handler rejected message

### Coordinator Module (`src/coordinator/mod.rs`)

Exports: `CommitConfig`, `TopicPartition`, `CoordinatorError`, `OffsetTracker`, `PartitionState`

---

## 3. Requirements Analysis

### WORKER-01: `OffsetCoordinator` trait

Trait must define:
- `record_ack(topic, partition, offset)` — corresponds to `OffsetTracker::ack`
- `mark_failed(topic, partition, offset)` — corresponds to `OffsetTracker::mark_failed`
- `graceful_shutdown()` — called at worker pool shutdown; future Phase 15 use case (no-op on OffsetTracker for now)

**Note:** `graceful_shutdown()` is defined in the success criteria but no concrete behavior is specified for Phase 14. It is a placeholder for Phase 15's requirement that "WorkerPool graceful_shutdown commits highest contiguous offset per topic-partition before workers exit."

### WORKER-02: `OffsetTracker` implements `OffsetCoordinator`

`OffsetTracker` already has all required methods. The implementation is straightforward delegation.

### WORKER-03: `WorkerPool::new()` accepts `Arc<dyn OffsetCoordinator>`

`WorkerPool::new()` gets a new parameter `offset_coordinator: Arc<dyn OffsetCoordinator>`. This is passed into `worker_loop` and stored in `WorkerPool` struct. `worker_loop` signature needs updating.

---

## 4. Design Decisions

### Decision: Trait method names vs OffsetTracker method names

The success criteria uses `record_ack` (not `ack`), `mark_failed` (matches), `graceful_shutdown` (new).

`OffsetTracker::ack` → `OffsetCoordinator::record_ack` (semantic wrapper)
`OffsetTracker::mark_failed` → `OffsetCoordinator::mark_failed` (identical)
`graceful_shutdown()` → no OffsetTracker method exists; placeholder for Phase 15

**Rationale:** `record_ack` is more descriptive for a coordinator abstraction. `ack` is internal tracker terminology.

### Decision: Where to define the trait

`src/coordinator/mod.rs` — the trait is logically part of the coordinator subsystem. This keeps all offset coordination in one place.

New exports:
- `OffsetCoordinator` trait
- `OffsetTracker::into_coordinator()` or the trait impl stays in `offset_tracker.rs`

### Decision: `graceful_shutdown` signature

```rust
fn graceful_shutdown(&self) {}
```

- Takes `&self` (not `&mut self`) — can be called on shared `Arc`
- No return value — Phase 15 implementation will do real work
- Empty body for Phase 14 — satisfying the trait definition without affecting behavior

### Decision: WorkerPool structural changes

Add `Arc<dyn OffsetCoordinator>` to `WorkerPool` struct:
```rust
pub struct WorkerPool {
    join_set: JoinSet<()>,
    pub(crate) shutdown_token: CancellationToken,
    offset_coordinator: Arc<dyn OffsetCoordinator>,
}
```

Update `new()` to accept the new parameter. Pass to each `worker_loop` invocation.

### Decision: `Send + Sync` bounds

`OffsetCoordinator` trait should be `pub trait OffsetCoordinator: Send + Sync` to allow `Arc<dyn OffsetCoordinator>` to be shared across tokio tasks safely.

---

## 5. Implementation Sketch

### New trait in `src/coordinator/offset_coordinator.rs` (new file)

```rust
use std::sync::Arc;

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

### `OffsetTracker` impl in `src/coordinator/offset_tracker.rs`

```rust
impl OffsetCoordinator for OffsetTracker {
    fn record_ack(&self, topic: &str, partition: i32, offset: i64) {
        self.ack(topic, partition, offset)
    }

    fn mark_failed(&self, topic: &str, partition: i32, offset: i64) {
        OffsetTracker::mark_failed(self, topic, partition, offset)
    }

    fn graceful_shutdown(&self) {
        // Phase 15: commit highest contiguous per topic-partition
    }
}
```

### WorkerPool changes

`worker_loop` receives `offset_coordinator: Arc<dyn OffsetCoordinator>` and calls:
- `offset_coordinator.record_ack(...)` on `ExecutionResult::Ok`
- `offset_coordinator.mark_failed(...)` on `ExecutionResult::Error` or `ExecutionResult::Rejected`

---

## 6. Dependency / Risk Analysis

### Risks
- None identified — trait definition is straightforward, delegation to existing methods

### Dependencies
- Phase 11 (OffsetTracker) — all methods already exist
- Phase 13 (ConsumerRunner store_offset) — not directly needed for this phase, but Phase 14 must not break it

### No-blockers
- This phase modifies `WorkerPool::new()` signature — callers (Phase 15 and Phase 16) will update accordingly

---

## 7. What I Need to Know to Plan This Phase

1. **Where to define `OffsetCoordinator` trait:** New file `src/coordinator/offset_coordinator.rs` or inline in `mod.rs`?
2. **Export strategy:** Re-export from `src/coordinator/mod.rs`
3. **Phase 15 integration point:** `graceful_shutdown()` will be implemented in Phase 15 to commit highest contiguous — Phase 14 just defines the signature with empty body

---

## 8. Success Criteria Confirmation

| # | Criterion | Status |
|---|-----------|--------|
| 1 | `OffsetCoordinator` trait defines `record_ack`, `mark_failed`, `graceful_shutdown` | Will implement |
| 2 | `OffsetTracker` implements `OffsetCoordinator` trait | Will implement |
| 3 | `WorkerPool::new()` accepts `Arc<dyn OffsetCoordinator>` at construction | Will implement |

---

## 9. Open Questions for Planning

1. Should `graceful_shutdown` return a `Result` or be async? (No — Phase 14 is sync trait method, no async in Phase 14)
2. Should we add `highest_contiguous` and `should_commit` to the trait too, or only the worker-facing methods (`record_ack`, `mark_failed`, `graceful_shutdown`)? — **Only worker-facing methods for Phase 14.** Phase 15 uses `should_commit` + `highest_contiguous` but those are accessed directly from the concrete `OffsetTracker` via `Arc<OffsetTracker>` in `OffsetCommitter` context, not via the trait.
3. Any need to rename `OffsetTracker::ack` → internal detail only, not part of trait public API

---

*Research complete. Next: write 14-PLAN.md.*