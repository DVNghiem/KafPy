# Technology Stack: Graceful Shutdown and Rebalance Handling

**Project:** KafPy
**Researched:** 2026-04-19
**Confidence:** MEDIUM-HIGH

## Executive Summary

The existing stack has foundational pieces for graceful shutdown (`tokio_util::CancellationToken`) and error types (`thiserror`). The missing components are **shutdown coordination** across components (consumer, worker pool, offset coordinator) and **rebalance event handling** via rdkafka's `ConsumerContext` trait. No new crate dependencies are required.

---

## Current Stack Assessment

### Already Available (DO NOT ADD)

| Technology | Status | Notes |
|------------|--------|-------|
| `tokio_util = "0.7.17"` | Already present | Provides `CancellationToken` used in `WorkerPool` |
| `thiserror = "2.0.17"` | Already present | Used in `errors.rs`, `coordinator/error.rs` |
| `tokio::select!` with cancellation | Already present | Worker loops already use `shutdown_token.cancelled()` branch |
| `broadcast::channel` for signals | Already present | `ConsumerRunner` uses `broadcast::Sender<()>` for stop signal |
| `parking_lot = "0.12"` | Already present | Used in `OffsetTracker`, `BatchAccumulator` |

### What Needs to Be Added

| Component | Approach | New Dependency |
|-----------|----------|----------------|
| Shutdown coordinator | New module tracking phase transitions | None |
| Rebalance event handling | Implement `ConsumerContext` from rdkafka | None |
| Lifecycle error types | Extend `coordinator/error.rs` with `thiserror` | None |
| Partition ownership state | New struct tracking revoked/assigned partitions | None |

---

## Shutdown Coordination

### Required: ShutdownCoordinator Module

**Location:** `src/coordinator/shutdown.rs` (new file)

The current `WorkerPool::shutdown()` calls `shutdown_token.cancel()` and then flushes offsets. However, there is no coordinated multi-phase shutdown across:
1. Consumer runner (stop accepting new messages)
2. Worker pool (drain in-flight)
3. Offset coordinator (flush and commit)
4. DLQ producer (close)

**Pattern:** Use `CancellationToken` hierarchy with phase tracking:

```rust
use tokio_util::sync::CancellationToken;
use std::sync::atomic::{AtomicU8, Ordering};

const PHASE_IDLE: u8 = 0;
const PHASE_CONSUMER_STOPPING: u8 = 1;
const PHASE_WORKERS_DRAINING: u8 = 2;
const PHASE_OFFSETS_COMMITTING: u8 = 3;
const PHASE_COMPLETE: u8 = 4;

pub struct ShutdownCoordinator {
    phase: AtomicU8,
    consumer_token: CancellationToken,
    workers_token: CancellationToken,
}

impl ShutdownCoordinator {
    pub fn new() -> Self {
        let consumer_token = CancellationToken::new();
        let workers_token = consumer_token.child_token();
        Self {
            phase: AtomicU8::new(PHASE_IDLE),
            consumer_token,
            workers_token,
        }
    }

    /// Returns child token for workers (cancelled when consumer phase ends)
    pub fn worker_token(&self) -> CancellationToken {
        self.workers_token.clone()
    }

    /// Initiate shutdown — stops consumer first, then workers
    pub async fn initiate(&self) {
        // Phase 1: Signal consumer to stop
        self.phase.store(PHASE_CONSUMER_STOPPING, Ordering::SeqCst);
        self.consumer_token.cancel();

        // Phase 2: Workers drain (tracked by caller via JoinSet::shutdown())

        // Phase 3: Offsets commit (tracked by caller)
        self.phase.store(PHASE_OFFSETS_COMMITTING, Ordering::SeqCst);

        // Phase 4: Complete
        self.phase.store(PHASE_COMPLETE, Ordering::SeqCst);
    }

    pub fn current_phase(&self) -> u8 {
        self.phase.load(Ordering::SeqCst)
    }
}
```

### Integration Points

1. **ConsumerRunner** — currently uses `broadcast::channel(1)` for shutdown. Replace with `CancellationToken` from `ShutdownCoordinator` for hierarchical cancellation.

2. **WorkerPool** — already uses `CancellationToken`. The `worker_token()` from `ShutdownCoordinator` replaces the current flat token.

3. **OffsetCoordinator** — `graceful_shutdown()` exists but needs to be called at correct phase (after workers drain, before final commit).

4. **DlqProducer** — needs `close()` method to flush pending produce operations before final shutdown.

---

## Rebalance Handling

### Required: RebalanceEvent Handler

**Location:** `src/coordinator/rebalance.rs` (new file)

rdkafka's `StreamConsumer` supports rebalance callbacks via implementing the `ConsumerContext` trait. This is the 0.38 API — older string-based config is deprecated.

**Key rebalance events:**
- `Rebalance::Revoke` — partitions revoked (stop processing)
- `Rebalance::Assign` — partitions assigned (resume processing)
- `Rebalance::RevokeAll` / `Rebalance::AssignAll` — group leave/join

**Recommended pattern:**

```rust
use rdkafka::consumer::{Consumer, Rebalance, ConsumerContext};
use rdkafka::error::KafkaResult;
use std::sync::Arc;

pub struct RebalanceHandler {
    offset_coordinator: Arc<dyn OffsetCoordinator>,
    partition_ownership: parking_lot::Mutex<HashMap<(String, i32), PartitionOwner>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PartitionOwner {
    SelfOwned,
    Revoked,
}

impl RebalanceHandler {
    pub fn new(offset_coordinator: Arc<dyn OffsetCoordinator>) -> Self {
        Self {
            offset_coordinator,
            partition_ownership: parking_lot::Mutex::new(HashMap::new()),
        }
    }

    pub fn handle(&self, rebalance: Rebalance) {
        match rebalance {
            Rebalance::Revoke(partitions) => {
                let mut ownership = self.partition_ownership.lock();
                for partition in &partitions {
                    ownership.insert(
                        (partition.topic().to_string(), partition.partition()),
                        PartitionOwner::Revoked,
                    );
                }
                tracing::info!(partitions = ?partitions, "partitions revoked");
            }
            Rebalance::Assign(partitions) => {
                let mut ownership = self.partition_ownership.lock();
                for partition in &partitions {
                    ownership.insert(
                        (partition.topic().to_string(), partition.partition()),
                        PartitionOwner::SelfOwned,
                    );
                }
                tracing::info!(partitions = ?partitions, "partitions assigned");
            }
            Rebalance::RevokeAll | Rebalance::AssignAll => {
                let mut ownership = self.partition_ownership.lock();
                ownership.clear();
            }
        }
    }

    pub fn is_owned(&self, topic: &str, partition: i32) -> bool {
        let ownership = self.partition_ownership.lock();
        ownership
            .get(&(topic.to_string(), partition))
            .map(|o| *o == PartitionOwner::SelfOwned)
            .unwrap_or(false)
    }
}
```

### Integration with ConsumerRunner

rdkafka 0.38 uses `ConsumerContext` trait for rebalance callbacks. The callback is set by implementing `ConsumerContext` and passing to `StreamConsumer::new()`:

```rust
// Custom context that wires in our RebalanceHandler
pub struct KafPyConsumerContext {
    rebalance_handler: Arc<RebalanceHandler>,
}

impl ConsumerContext for KafPyConsumerContext {
    fn rebalance(
        &self,
        consumer: &BaseConsumer,
        rebalance: Rebalance,
    ) -> KafkaResult<()> {
        self.rebalance_handler.handle(rebalance);
        Ok(())
    }
}

// In ConsumerRunner::new():
let consumer: StreamConsumer = config
    .clone()
    .into_rdkafka_config()
    .create_with_context(KafPyConsumerContext::new(Arc::new(...)))
    .map_err(ConsumerError::Kafka)?;
```

---

## Error Types (thiserror)

**Extend existing:** `src/coordinator/error.rs`

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoordinatorError {
    #[error("topic '{0}' partition {1} not registered in offset tracker")]
    TopicPartitionNotFound(String, i32),

    #[error("offset {0} is invalid (must be >= 0)")]
    InvalidOffset(i64),

    // --- Graceful Shutdown Errors ---
    #[error("shutdown timeout expired during phase {phase}")]
    ShutdownTimeout { phase: String },

    #[error("consumer shutdown failed: {0}")]
    ConsumerShutdown(String),

    #[error("worker pool shutdown failed: {0}")]
    WorkerPoolShutdown(String),

    #[error("offset commit failed during shutdown: {0}")]
    OffsetCommitDuringShutdown(String),

    // --- Rebalance Errors ---
    #[error("rebalance failed: {0}")]
    RebalanceFailed(String),

    #[error("partition {topic}:{partition} revoked during in-flight processing")]
    PartitionRevokedDuringProcessing { topic: String, partition: i32 },

    #[error("cannot commit offsets for unowned partition {topic}:{partition}")]
    CommitForUnownedPartition { topic: String, partition: i32 },
}
```

---

## Partition Ownership State

**New struct in:** `src/coordinator/partition_ownership.rs`

```rust
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

/// Tracks which topic-partitions are currently owned by this consumer.
/// Used to ignore messages from revoked partitions during rebalance.
pub struct PartitionOwnership {
    ownership: Mutex<HashMap<(String, i32), OwnerState>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OwnerState {
    Owned,
    Revoked,
    PendingAssign,
}

impl PartitionOwnership {
    pub fn new() -> Self {
        Self {
            ownership: Mutex::new(HashMap::new()),
        }
    }

    pub fn set_owned(&self, topic: String, partition: i32) {
        let mut guard = self.ownership.lock();
        guard.insert((topic, partition), OwnerState::Owned);
    }

    pub fn set_revoked(&self, topic: &str, partition: i32) {
        let mut guard = self.ownership.lock();
        guard.insert((topic.to_string(), partition), OwnerState::Revoked);
    }

    pub fn is_owned(&self, topic: &str, partition: i32) -> bool {
        let guard = self.ownership.lock();
        guard
            .get(&(topic.to_string(), partition))
            .map(|s| *s == OwnerState::Owned)
            .unwrap_or(false)
    }

    pub fn clear(&self) {
        let mut guard = self.ownership.lock();
        guard.clear();
    }
}
```

---

## No New Dependencies Required

The existing `Cargo.toml` already includes all required primitives:

| Dependency | Used For |
|------------|----------|
| `tokio = { version = "1.40", features = ["full"] }` | Async runtime, `select!`, `spawn` |
| `tokio-util = "0.7.17"` | `CancellationToken` (already used in WorkerPool) |
| `thiserror = "2.0.17"` | Error types (already used in errors.rs) |
| `parking_lot = "0.12"` | Synchronization (already used in OffsetTracker, BatchAccumulator) |
| `rdkafka = { version = "0.38" }` | Rebalance callbacks via `ConsumerContext` trait |

---

## Architecture: Shutdown Sequence

```
1. ShutdownCoordinator::initiate()
   |
   +---> Phase: CONSUMER_STOPPING
   |     +---> consumer_token.cancel()
   |     +---> ConsumerRunner loop exits on next recv() or broadcast
   |
   +---> Phase: WORKERS_DRAINING
   |     +---> workers_token.cancel() (child of consumer_token)
   |     +---> Worker loops: process in-flight, then exit
   |     +---> JoinSet::shutdown() awaits completion
   |
   +---> Phase: OFFSETS_COMMITTING
         +---> offset_coordinator.flush_failed_to_dlq()
         +---> offset_coordinator.graceful_shutdown()
         +---> dlq_producer.close()
```

---

## Sources

- [Tokio CancellationToken](https://docs.rs/tokio-util/latest/tokio_util/sync/struct.CancellationToken.html) — HIGH confidence
- [rdkafka ConsumerContext](https://docs.rs/rdkafka/latest/rdkafka/config/trait.ConsumerContext.html) — MEDIUM confidence (0.38 API)
- [WorkerPool shutdown pattern](../src/worker_pool/mod.rs) — existing code review — HIGH confidence
- [thiserror crate](https://docs.rs/thiserror/latest/thiserror/) — HIGH confidence
- [parking_lot crate](https://docs.rs/parking_lot/latest/parking_lot/) — HIGH confidence

---

*Stack research for: graceful shutdown and rebalance handling*
*Researched: 2026-04-19*
