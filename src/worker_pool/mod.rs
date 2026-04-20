//! WorkerPool — manages N Tokio workers polling handler queues.

use std::sync::Arc;
use tokio::sync::mpsc;
use crate::coordinator::retry_coordinator::RetryCoordinator;
use crate::coordinator::shutdown::ShutdownCoordinator;
use crate::coordinator::OffsetCoordinator;
use crate::dispatcher::queue_manager::QueueManager;
use crate::dispatcher::OwnedMessage;
use crate::dlq::{DlqMetadata, DlqRouter, SharedDlqProducer};
use crate::failure::FailureReason;
use crate::observability::metrics::HandlerMetrics;
use crate::observability::tracing::KafpySpanExt;
use crate::python::context::ExecutionContext;

pub(crate) static HANDLER_METRICS: HandlerMetrics = HandlerMetrics;

pub mod accumulator;
pub mod batch_loop;
pub mod pool;
pub mod state;
pub mod worker;

pub use accumulator::PerPartitionBuffer;
pub use batch_loop::{batch_worker_loop, flush_partition_batch, handle_batch_result_inline};
pub use pool::WorkerPool;
pub use state::{BatchState, WorkerState};
pub use worker::worker_loop;

// ─── Execution Action ─────────────────────────────────────────────────────────

/// Result of failure handling — returned by `handle_execution_failure`.
#[derive(Debug)]
pub enum ExecutionAction {
    /// Message was processed (no retry, no DLQ) — caller should ack.
    Ack,
    /// Schedule retry with the given delay, then re-process the same message.
    Retry { delay: std::time::Duration },
    /// Message was routed to DLQ — caller should ack (DLQ is fire-and-forget).
    Dlq,
}

/// Handles retry/DLQ routing for Error and Rejected execution results.
/// Called after `offset_coordinator.mark_failed()` has recorded the failure.
/// Note: `msg` is borrowed, not consumed — caller retains ownership for retry re-enqueue.
pub async fn handle_execution_failure(
    ctx: &ExecutionContext,
    msg: &OwnedMessage,
    reason: &FailureReason,
    retry_coordinator: Arc<RetryCoordinator>,
    dlq_producer: Arc<SharedDlqProducer>,
    dlq_router: Arc<dyn DlqRouter>,
    queue_manager: Arc<QueueManager>,
) -> ExecutionAction {
    let (should_retry, should_dlq, delay) =
        retry_coordinator.record_failure(&ctx.topic, ctx.partition, ctx.offset, reason);

    if should_retry {
        if let Some(d) = delay {
            tracing::info!(
                topic = %ctx.topic, partition = ctx.partition, offset = ctx.offset,
                attempt = retry_coordinator.attempt_count(&ctx.topic, ctx.partition, ctx.offset),
                delay_ms = d.as_millis(), "scheduling retry"
            );
            return ExecutionAction::Retry { delay: d };
        }
    }

    if should_dlq {
        let metadata = DlqMetadata::new(
            ctx.topic.clone(), ctx.partition, ctx.offset, reason.to_string(),
            retry_coordinator.attempt_count(&ctx.topic, ctx.partition, ctx.offset) as u32,
            chrono::Utc::now(), chrono::Utc::now(),
        );

        let dlq_span = tracing::Span::current().kafpy_dlq_route(
            ctx.topic.as_str(), &reason.to_string(), ctx.partition,
        );
        let tp = dlq_span.in_scope(|| dlq_router.route(&metadata));
        tracing::error!(
            topic = %ctx.topic, partition = ctx.partition, offset = ctx.offset,
            dlq_topic = %tp.topic, dlq_partition = tp.partition,
            reason = %reason, attempt_count = metadata.attempt_count, "routing message to DLQ"
        );

        // Fire-and-forget — don't await
        dlq_producer.produce_async(
            tp.topic.clone(), tp.partition,
            msg.payload.clone().unwrap_or_default(), msg.key.clone(), &metadata,
        );

        queue_manager.ack(&msg.topic, 1);
        return ExecutionAction::Dlq;
    }

    queue_manager.ack(&msg.topic, 1);
    ExecutionAction::Ack
}
