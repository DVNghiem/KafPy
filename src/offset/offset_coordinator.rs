//! OffsetCoordinator trait — abstracts offset tracking for WorkerPool.
//!
//! Decouples WorkerPool from concrete OffsetTracker implementation.
//! Implementations must be thread-safe (Send + Sync) as the trait is
//! used across tokio task boundaries via Arc.

use crate::dlq::{DlqRouter, SharedDlqProducer};
use crate::failure::FailureReason;

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
    /// The `reason` parameter carries FailureReason for future DLQ routing.
    fn mark_failed(&self, topic: &str, partition: i32, offset: i64, reason: &FailureReason);

    /// Called when the worker pool is shutting down gracefully.
    ///
    /// Phase 15 will use this to commit highest contiguous offsets before exit.
    /// Phase 14 implementation is a no-op.
    fn graceful_shutdown(&self);

    /// Flushes all failed offsets to DLQ before final commit.
    ///
    /// Iterates all partitions, and for each one with failed_offsets, produces
    /// all failed messages to DLQ via the router and producer (D-02: flush all
    /// failed — retryable AND terminal).
    ///
    /// This is fire-and-forget — does not block waiting for broker acknowledgment.
    /// OffsetTracker does not store original payloads, so empty payloads are used.
    fn flush_failed_to_dlq(
        &self,
        dlq_router: &std::sync::Arc<dyn DlqRouter>,
        dlq_producer: &std::sync::Arc<SharedDlqProducer>,
    );
}
