//! Batch worker loop and result handler.

use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::dispatcher::queue_manager::QueueManager;
use crate::dispatcher::OwnedMessage;
use crate::dlq::{DlqMetadata, DlqRouter, SharedDlqProducer};
use crate::execution::batch::BatchAccumulator;
use crate::execution::callback::PythonHandler;
use crate::execution::context::ExecutionContext;
use crate::execution::execution_result::BatchExecutionResult;
use crate::log::{debug, error, info, warn, Span};
use crate::observability::metrics::{MetricLabels, PythonCallMetrics};
use crate::observability::runtime_snapshot::WorkerPoolState;
use crate::observability::tracing::KafpySpanExt;
use crate::offset::offset_coordinator::OffsetCoordinator;
use crate::retry::retry_coordinator::RetryCoordinator;
use crate::worker_pool::state::BatchState;
use crate::worker_pool::HANDLER_METRICS;

/// Flushes a single partition batch through the Python handler.
///
/// Common helper used by all 6 flush sites in batch_worker_loop:
/// - deadline expiry, backpressure flush, preemptive flush, max_batch_size flush,
///   channel-closed drain, shutdown drain.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn flush_partition_batch(
    partition: i32,
    batch: Vec<OwnedMessage>,
    handler: Arc<PythonHandler>,
    worker_id: usize,
    worker_pool_state: Arc<WorkerPoolState>,
    queue_manager: Arc<QueueManager>,
    offset_coordinator: Arc<dyn OffsetCoordinator>,
    retry_coordinator: Arc<RetryCoordinator>,
    dlq_producer: Arc<SharedDlqProducer>,
    dlq_router: Arc<dyn DlqRouter>,
    prometheus_sink: crate::observability::SharedPrometheusSink,
) {
    if batch.is_empty() {
        return;
    }
    let topic = batch[0].topic.clone();
    let ctx = ExecutionContext::new(topic.clone(), partition, batch[0].offset, worker_id);
    let span = Span::current().kafpy_handler_invoke(
        topic.as_str(),
        handler.name(),
        topic.as_str(),
        partition,
        batch[0].offset,
        handler.mode().as_str(),
        0, // batch: attempt starts at 0
    );
    worker_pool_state.set_busy(worker_id, "shared".to_string());
    let py_call_start = std::time::Instant::now();
    let result = span
        .in_scope(|| async { handler.invoke_mode_batch(&ctx, batch.clone()).await })
        .await;
    PythonCallMetrics::record_call(
        &prometheus_sink,
        "handler",
        handler.mode().as_str(),
        py_call_start.elapsed(),
        batch.len(),
    );
    handle_batch_result_inline(
        result,
        batch,
        &topic,
        partition,
        queue_manager,
        offset_coordinator,
        retry_coordinator,
        dlq_producer,
        dlq_router,
        prometheus_sink,
    )
    .await;
    worker_pool_state.set_idle(worker_id);
}

/// Batch worker loop — accumulates messages per handler until batch is full OR deadline expires.
///
/// Uses tokio::select! to race:
/// - Message arrival: add to accumulator, flush if max_batch_size reached
/// - Deadline expiry: flush all partitions
/// - Cancellation: flush all and exit
///
/// Backpressure is applied per-batch: if QueueManager::get_inflight() >= capacity,
#[allow(clippy::too_many_arguments)]
pub(crate) async fn batch_worker_loop(
    mut rx: mpsc::Receiver<OwnedMessage>,
    handler: Arc<PythonHandler>,
    queue_manager: Arc<QueueManager>,
    offset_coordinator: Arc<dyn OffsetCoordinator>,
    retry_coordinator: Arc<RetryCoordinator>,
    dlq_producer: Arc<SharedDlqProducer>,
    dlq_router: Arc<dyn DlqRouter>,
    worker_id: usize,
    shutdown_token: CancellationToken,
    worker_pool_state: Arc<WorkerPoolState>,
    prometheus_sink: crate::observability::SharedPrometheusSink,
) {
    info!(worker_id = worker_id, "batch worker started");

    let batch_policy = handler
        .batch_policy()
        .expect("BatchSync requires batch_policy");
    let accumulator =
        BatchAccumulator::new(batch_policy.max_batch_size, batch_policy.max_batch_wait_ms);

    // Track whether we are in a backpressure-blocking state
    let mut batch_state = BatchState::Normal;

    loop {
        // Determine the next deadline for the select! sleep
        let next_deadline = accumulator.next_deadline();

        tokio::select! {
            // Branch 1: Deadline fires — flush all partitions
            _ = async {
                if let Some(deadline) = next_deadline {
                    tokio::time::sleep_until(deadline).await;
                } else {
                    // No messages accumulated — wait indefinitely (will be woken by message or shutdown)
                    tokio::time::sleep(std::time::Duration::MAX).await;
                }
            } => {
                // Deadline expired — flush all
                if !accumulator.is_empty() {
                    let partitions = accumulator.flush_all();
                    for (partition, batch) in partitions {
                        flush_partition_batch(
                            partition,
                            batch,
                            Arc::clone(&handler),
                            worker_id,
                            Arc::clone(&worker_pool_state),
                            Arc::clone(&queue_manager),
                            Arc::clone(&offset_coordinator),
                            Arc::clone(&retry_coordinator),
                            Arc::clone(&dlq_producer),
                            Arc::clone(&dlq_router),
                            prometheus_sink.clone(),
                        )
                        .await;
                    }
                }
            }

            // Branch 2: Message arrives
            msg = rx.recv() => {
                match msg {
                    Some(msg) => {
                        // Backpressure check: if at capacity, flush first then block
                        if !batch_state.is_backpressure() {
                            if let Some(capacity) = queue_manager.get_capacity(&msg.topic) {
                                if let Some(inflight) = queue_manager.get_inflight(&msg.topic) {
                                    if inflight >= capacity {
                                        // Flush current accumulator before blocking
                                        batch_state = BatchState::Backpressure;
                                        let partitions = accumulator.flush_all();
                                        for (partition, batch) in partitions {
                                            flush_partition_batch(
                                                partition,
                                                batch,
                                                Arc::clone(&handler),
                                                worker_id,
                                                Arc::clone(&worker_pool_state),
                                                Arc::clone(&queue_manager),
                                                Arc::clone(&offset_coordinator),
                                                Arc::clone(&retry_coordinator),
                                                Arc::clone(&dlq_producer),
                                                Arc::clone(&dlq_router),
                                                prometheus_sink.clone(),
                                            )
                                            .await;
                                        }
                                    }
                                }
                            }
                        }

                        // Check if adding would fill the partition — flush preemptively
                        if accumulator.would_fill_partition(msg.partition) {
                            if let Some(batch) = accumulator.flush_partition(msg.partition) {
                                flush_partition_batch(
                                    batch[0].partition,
                                    batch,
                                    Arc::clone(&handler),
                                    worker_id,
                                    Arc::clone(&worker_pool_state),
                                    Arc::clone(&queue_manager),
                                    Arc::clone(&offset_coordinator),
                                    Arc::clone(&retry_coordinator),
                                    Arc::clone(&dlq_producer),
                                    Arc::clone(&dlq_router),
                                    prometheus_sink.clone(),
                                )
                                .await;
                            }
                        }

                        // Add the message to accumulator
                        let msg_partition = msg.partition;
                        accumulator.add(msg);

                        // If we just hit max_batch_size after adding, flush immediately
                        if accumulator.would_fill_partition(msg_partition) {
                            if let Some(batch) = accumulator.flush_partition(msg_partition) {
                                flush_partition_batch(
                                    batch[0].partition,
                                    batch,
                                    Arc::clone(&handler),
                                    worker_id,
                                    Arc::clone(&worker_pool_state),
                                    Arc::clone(&queue_manager),
                                    Arc::clone(&offset_coordinator),
                                    Arc::clone(&retry_coordinator),
                                    Arc::clone(&dlq_producer),
                                    Arc::clone(&dlq_router),
                                    prometheus_sink.clone(),
                                )
                                .await;
                            }
                        }
                    }
                    None => {
                        // Channel closed — drain accumulator and exit
                        info!(
                            worker_id = worker_id,
                            "batch worker: channel closed, draining"
                        );
                        let partitions = accumulator.flush_all();
                        for (partition, batch) in partitions {
                            flush_partition_batch(
                                partition,
                                batch,
                                Arc::clone(&handler),
                                    worker_id,
                                    Arc::clone(&worker_pool_state),
                                    Arc::clone(&queue_manager),
                                Arc::clone(&offset_coordinator),
                                Arc::clone(&retry_coordinator),
                                Arc::clone(&dlq_producer),
                                Arc::clone(&dlq_router),
                                prometheus_sink.clone(),
                            )
                            .await;
                        }
                        break;
                    }
                }
            }

            // Branch 3: Shutdown signal — flush all and exit
            _ = shutdown_token.cancelled() => {
                info!(
                    worker_id = worker_id,
                    "batch worker: shutdown signal, draining"
                );
                let partitions = accumulator.flush_all();
                for (partition, batch) in partitions {
                    flush_partition_batch(
                        partition,
                        batch,
                        Arc::clone(&handler),
                        worker_id,
                        Arc::clone(&worker_pool_state),
                        Arc::clone(&queue_manager),
                        Arc::clone(&offset_coordinator),
                        Arc::clone(&retry_coordinator),
                        Arc::clone(&dlq_producer),
                        Arc::clone(&dlq_router),
                        prometheus_sink.clone(),
                    )
                    .await;
                }
                break;
            }
        }
    }

    info!(worker_id = worker_id, "batch worker stopped");
}

/// Handle the result of a batch invocation — inline version that owns the batch messages.
///
/// AllSuccess → record_ack per message individually.
/// AllFailure → record_failure per message individually (routes to RetryCoordinator).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn handle_batch_result_inline(
    result: BatchExecutionResult,
    batch: Vec<OwnedMessage>,
    topic: &str,
    partition: i32,
    queue_manager: Arc<QueueManager>,
    offset_coordinator: Arc<dyn OffsetCoordinator>,
    retry_coordinator: Arc<RetryCoordinator>,
    dlq_producer: Arc<SharedDlqProducer>,
    dlq_router: Arc<dyn DlqRouter>,
    prometheus_sink: crate::observability::SharedPrometheusSink,
) {
    match result {
        BatchExecutionResult::AllSuccess(offsets) => {
            // Record batch size histogram for this partition
            let batch_size_labels = MetricLabels::new()
                .insert("handler_id", topic)
                .insert("topic", topic)
                .insert("partition", partition.to_string());
            HANDLER_METRICS.record_batch_size(&prometheus_sink, &batch_size_labels, batch.len());

            // Record throughput for each message in the batch
            let mode = "BatchSync";
            for _ in offsets.iter() {
                crate::observability::metrics::ThroughputMetrics::record_throughput(
                    &prometheus_sink,
                    topic,
                    topic,
                    mode,
                );
            }

            // EXEC-10: Each message calls record_ack individually
            for offset in offsets {
                retry_coordinator.record_success(topic, partition, offset);
                queue_manager.ack(topic, 1);
                offset_coordinator.record_ack(topic, partition, offset);
                debug!(
                    topic = %topic,
                    partition = partition,
                    offset = offset,
                    "batch message acked"
                );
            }
        }
        BatchExecutionResult::AllFailure(reason) => {
            // Record batch size histogram for this failed batch
            let batch_size_labels = MetricLabels::new()
                .insert("handler_id", topic)
                .insert("topic", topic)
                .insert("partition", partition.to_string());
            HANDLER_METRICS.record_batch_size(&prometheus_sink, &batch_size_labels, batch.len());

            // EXEC-10: All messages in batch flow to RetryCoordinator
            // We have access to the original batch messages here for routing
            warn!(
                topic = %topic,
                partition = partition,
                reason = %reason,
                batch_size = batch.len(),
                "batch failed entirely"
            );

            for msg in batch {
                // Record failure. In batch mode we do not retry inline: retrying a
                // batch requires re-enqueuing through the ingestion path, which is
                // not available here. Instead, route retryable failures directly to
                // DLQ so the batch worker is never blocked by retry sleeps.
                // Removing the blocking sleep here is critical for burst resilience —
                // a sleeping batch worker cannot drain its queue.
                let (should_retry, should_dlq, _delay) =
                    retry_coordinator.record_failure(topic, partition, msg.offset, &reason);

                offset_coordinator.mark_failed(topic, partition, msg.offset, &reason);

                let route_to_dlq = should_dlq || should_retry;

                if route_to_dlq {
                    let attempt = retry_coordinator.attempt_count(topic, partition, msg.offset);
                    if should_retry {
                        warn!(
                            topic = %topic,
                            partition = partition,
                            offset = msg.offset,
                            attempt = attempt,
                            "batch message routed to DLQ (batch mode does not support inline retry)"
                        );
                    }
                    let metadata = DlqMetadata::new(
                        topic.to_string(),
                        partition,
                        msg.offset,
                        reason.to_string(),
                        attempt as u32,
                        chrono::Utc::now(),
                        chrono::Utc::now(),
                        None,
                        None,
                        None,
                        None,
                    );

                    let dlq_span =
                        Span::current().kafpy_dlq_route(topic, &reason.to_string(), partition);
                    let tp = dlq_span.in_scope(|| dlq_router.route(&metadata));
                    error!(
                        topic = %topic,
                        partition = partition,
                        offset = msg.offset,
                        dlq_topic = %tp.topic,
                        dlq_partition = tp.partition,
                        reason = %reason,
                        "routing batch message to DLQ"
                    );

                    // Fire-and-forget
                    dlq_producer.produce_async(
                        tp.topic.clone(),
                        tp.partition,
                        msg.payload.clone().unwrap_or_default(),
                        msg.key.clone(),
                        &metadata,
                    );
                }

                // Always ack so queue counters don't leak
                queue_manager.ack(topic, 1);
            }
        }
    }
}
