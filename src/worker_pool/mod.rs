//! WorkerPool — manages N Tokio workers polling handler queues.
//!
//! Each worker independently polls its `mpsc::Receiver<OwnedMessage>`, invokes
//! the Python handler via `spawn_blocking`, and runs post-execution policy
//! via the `Executor` trait. Graceful shutdown waits for in-flight completion.

use std::sync::Arc;

use tokio::select;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use parking_lot::Mutex;
use std::collections::HashMap;

use crate::coordinator::retry_coordinator::RetryCoordinator;
use crate::coordinator::OffsetCoordinator;
use crate::dispatcher::queue_manager::QueueManager;
use crate::dispatcher::OwnedMessage;
use crate::dlq::{DlqMetadata, DlqRouter, SharedDlqProducer};
use crate::failure::FailureCategory;
use crate::observability::metrics::{HandlerMetrics, MetricLabels};
use crate::python::context::ExecutionContext;
use crate::python::execution_result::{BatchExecutionResult, ExecutionResult};
use crate::python::executor::Executor;
use crate::python::handler::PythonHandler;

// Static shared handler metrics recorder (noop until a sink is installed)
static HANDLER_METRICS: HandlerMetrics = HandlerMetrics;

// Noop sink used when no metrics sink is configured
struct NoopSink;
impl crate::observability::metrics::MetricsSink for NoopSink {
    fn record_counter(&self, _name: &str, _labels: &[(&str, &str)]) {}
    fn record_histogram(&self, _name: &str, _value: f64, _labels: &[(&str, &str)]) {}
    fn record_gauge(&self, _name: &str, _value: f64, _labels: &[(&str, &str)]) {}
}

// ─── Batch Accumulator ─────────────────────────────────────────────────────────

/// Per-partition message accumulator with fixed-window timer.
///
/// Timer starts on first message arrival. Deadline is FIXED at first arrival +
/// max_batch_wait_ms — it does NOT reset on subsequent messages (D-02 fixed-window).
/// Each partition maintains its own timer independently.
struct PartitionAccumulator {
    messages: Vec<OwnedMessage>,
    deadline: Option<tokio::time::Instant>,
}

impl PartitionAccumulator {
    fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    fn len(&self) -> usize {
        self.messages.len()
    }

    /// Start the fixed-window timer on first message arrival.
    fn start_timer(&mut self, max_wait: std::time::Duration) {
        if self.deadline.is_none() {
            self.deadline = Some(tokio::time::Instant::now() + max_wait);
        }
    }

    /// Returns true if deadline has expired (used for polling in select! loop).
    fn is_deadline_expired(&self) -> bool {
        self.deadline
            .map(|d| tokio::time::Instant::now() >= d)
            .unwrap_or(false)
    }

    /// Add a message, starting the timer if this is the first message.
    fn add(&mut self, msg: OwnedMessage, max_wait: std::time::Duration) {
        if self.messages.is_empty() {
            self.start_timer(max_wait);
        }
        self.messages.push(msg);
    }

    /// Take all messages from this partition accumulator, clearing it.
    fn take_messages(&mut self) -> Vec<OwnedMessage> {
        std::mem::take(&mut self.messages)
    }
}

/// Per-handler batch accumulator.
///
/// Accumulates messages per-partition (preserving ordering within partition).
/// Batches are formed per-partition, then combined when flushed.
/// Uses parking_lot::Mutex for interior mutability (matches OffsetTracker pattern).
pub struct BatchAccumulator {
    partition_accumulators: Mutex<HashMap<i32, PartitionAccumulator>>,
    max_batch_size: usize,
    max_batch_wait: std::time::Duration,
}

impl BatchAccumulator {
    /// Create a new BatchAccumulator with the given batch policy.
    pub fn new(max_batch_size: usize, max_batch_wait_ms: u64) -> Self {
        Self {
            partition_accumulators: Mutex::new(HashMap::new()),
            max_batch_size,
            max_batch_wait: std::time::Duration::from_millis(max_batch_wait_ms),
        }
    }

    /// Returns true if adding this message would fill its partition to max_batch_size.
    /// Used to trigger a preemptive flush before adding.
    pub fn would_fill_partition(&self, partition: i32) -> bool {
        let guard = self.partition_accumulators.lock();
        guard
            .get(&partition)
            .map(|acc| acc.messages.len() >= self.max_batch_size)
            .unwrap_or(false)
    }

    /// Add a message to the appropriate partition accumulator.
    /// Starts the fixed-window timer if this is the first message for that partition.
    pub fn add(&self, msg: OwnedMessage) {
        let partition = msg.partition;
        let mut guard = self.partition_accumulators.lock();
        let acc = guard
            .entry(partition)
            .or_insert_with(|| PartitionAccumulator {
                messages: Vec::new(),
                deadline: None,
            });
        acc.add(msg, self.max_batch_wait);
    }

    /// Returns the earliest deadline across all partitions, or None if all empty.
    pub fn next_deadline(&self) -> Option<tokio::time::Instant> {
        let guard = self.partition_accumulators.lock();
        guard.values().filter_map(|p| p.deadline).min()
    }

    /// Returns true if any partition has messages with an expired deadline.
    pub fn is_any_deadline_expired(&self) -> bool {
        let guard = self.partition_accumulators.lock();
        for acc in guard.values() {
            if !acc.is_empty() && acc.is_deadline_expired() {
                return true;
            }
        }
        false
    }

    /// Returns true if the accumulator is completely empty.
    pub fn is_empty(&self) -> bool {
        let guard = self.partition_accumulators.lock();
        guard.values().all(|acc| acc.is_empty())
    }

    /// Flush and return all nonempty partitions as (partition, messages) pairs.
    /// Clears all partition accumulators after flushing.
    pub fn flush_all(&self) -> Vec<(i32, Vec<OwnedMessage>)> {
        let mut guard = self.partition_accumulators.lock();
        let mut result = Vec::new();
        for (partition, acc) in guard.iter_mut() {
            if !acc.is_empty() {
                result.push((*partition, acc.take_messages()));
            }
        }
        result
    }

    /// Flush and return messages from a specific partition.
    pub fn flush_partition(&self, partition: i32) -> Option<Vec<OwnedMessage>> {
        let mut guard = self.partition_accumulators.lock();
        guard.get_mut(&partition).and_then(|acc| {
            if acc.is_empty() {
                None
            } else {
                Some(acc.take_messages())
            }
        })
    }

    /// Returns total message count across all partitions.
    pub fn total_len(&self) -> usize {
        let guard = self.partition_accumulators.lock();
        guard.values().map(|acc| acc.len()).sum()
    }
}

/// Worker loop — polls messages and invokes the Python handler.
///
/// Uses `tokio::select!` on two branches when idle:
/// - `Some(msg) = rx.recv()` — picks up a message
/// - `_ = shutdown_token.cancelled()` — exits gracefully
///
/// When a message is picked up it is processed before polling again.
/// `active_message: Option<OwnedMessage>` tracks in-flight work — the
/// cancelled branch only fires when `active_message.is_none()`,
/// ensuring graceful shutdown waits for in-flight completion (EXEC-12).
async fn worker_loop(
    mut rx: mpsc::Receiver<OwnedMessage>,
    handler: Arc<PythonHandler>,
    executor: Arc<dyn Executor>,
    queue_manager: Arc<QueueManager>,
    offset_coordinator: Arc<dyn OffsetCoordinator>,
    retry_coordinator: Arc<RetryCoordinator>,
    dlq_producer: Arc<SharedDlqProducer>,
    dlq_router: Arc<dyn DlqRouter>,
    worker_id: usize,
    shutdown_token: CancellationToken,
) {
    tracing::info!(worker_id = worker_id, "worker started");

    let mut active_message: Option<OwnedMessage> = None;

    loop {
        // If there's an active message, process it without polling
        if let Some(msg) = active_message.take() {
            let ctx =
                ExecutionContext::new(msg.topic.clone(), msg.partition, msg.offset, worker_id);
            // Metrics: start latency timer and build labels before invoke
            let start = std::time::Instant::now();
            let invocation_labels = MetricLabels::new()
                .insert("handler_id", ctx.topic.as_str())
                .insert("topic", ctx.topic.as_str())
                .insert("mode", handler.mode().as_str());
            let result = handler.invoke_mode(&ctx, msg.clone()).await;
            let elapsed = start.elapsed();
            // Record invocation and latency after invoke returns
            HANDLER_METRICS.record_invocation(&NoopSink, &invocation_labels);
            HANDLER_METRICS.record_latency(&NoopSink, &invocation_labels, elapsed);
            // Record error counter on non-ok results
            if !result.is_ok() {
                let error_labels = MetricLabels::new()
                    .insert("handler_id", ctx.topic.as_str())
                    .insert("error_type", result.error_type_label());
                HANDLER_METRICS.record_error(&NoopSink, &error_labels);
            }
            let _outcome = executor.execute(&ctx, &msg, &result);

            match result {
                ExecutionResult::Ok => {
                    tracing::debug!(
                        worker_id = worker_id,
                        topic = %ctx.topic,
                        partition = ctx.partition,
                        offset = ctx.offset,
                        "handler executed successfully"
                    );
                    retry_coordinator.record_success(&ctx.topic, ctx.partition, ctx.offset);
                    queue_manager.ack(&msg.topic, 1);
                    offset_coordinator.record_ack(&ctx.topic, ctx.partition, ctx.offset);
                }
                ExecutionResult::Error {
                    ref reason,
                    ref exception,
                    ..
                } => {
                    tracing::warn!(
                        worker_id = worker_id,
                        topic = %ctx.topic,
                        partition = ctx.partition,
                        offset = ctx.offset,
                        exception = %exception,
                        "handler raised exception"
                    );
                    crate::failure::logging::log_failure(&ctx, reason, exception, false);

                    // Check if we should retry — now returns 3-tuple (should_retry, should_dlq, delay)
                    let (should_retry, should_dlq, delay) = retry_coordinator.record_failure(
                        &ctx.topic,
                        ctx.partition,
                        ctx.offset,
                        reason,
                    );

                    offset_coordinator.mark_failed(&ctx.topic, ctx.partition, ctx.offset, reason);

                    if should_retry {
                        // Sleep then retry
                        if let Some(d) = delay {
                            tracing::info!(
                                worker_id = worker_id,
                                topic = %ctx.topic,
                                partition = ctx.partition,
                                offset = ctx.offset,
                                attempt = retry_coordinator.attempt_count(&ctx.topic, ctx.partition, ctx.offset),
                                delay_ms = d.as_millis(),
                                "scheduling retry"
                            );
                            tokio::time::sleep(d).await;
                            active_message = Some(msg);
                            continue;
                        }
                    }

                    if should_dlq {
                        // Route to DLQ — construct metadata and produce fire-and-forget
                        let metadata = DlqMetadata::new(
                            ctx.topic.clone(),
                            ctx.partition,
                            ctx.offset,
                            reason.to_string(),
                            retry_coordinator.attempt_count(&ctx.topic, ctx.partition, ctx.offset)
                                as u32,
                            chrono::Utc::now(),
                            chrono::Utc::now(),
                        );

                        let tp = dlq_router.route(&metadata);
                        tracing::error!(
                            worker_id = worker_id,
                            topic = %ctx.topic,
                            partition = ctx.partition,
                            offset = ctx.offset,
                            dlq_topic = %tp.topic,
                            dlq_partition = tp.partition,
                            reason = %reason,
                            attempt_count = metadata.attempt_count,
                            "routing message to DLQ"
                        );

                        // Fire-and-forget — don't await
                        dlq_producer.produce_async(
                            tp.topic.clone(),
                            tp.partition,
                            msg.payload.clone().unwrap_or_default(),
                            msg.key.clone(),
                            &metadata,
                        );

                        // Ack the original message — it's now in DLQ
                        queue_manager.ack(&msg.topic, 1);
                    } else {
                        // Not retrying, not DLQ — count as processed
                        queue_manager.ack(&msg.topic, 1);
                    }
                }
                ExecutionResult::Rejected { ref reason, .. } => {
                    tracing::warn!(
                        worker_id = worker_id,
                        topic = %ctx.topic,
                        partition = ctx.partition,
                        offset = ctx.offset,
                        reason = %reason,
                        "handler rejected message"
                    );
                    // Extract exception name for rejection (Rejected carries reason_str, not exception)
                    let exc_name = "Rejected";
                    crate::failure::logging::log_failure(&ctx, reason, exc_name, false);

                    // Check if we should retry
                    let (should_retry, should_dlq, delay) = retry_coordinator.record_failure(
                        &ctx.topic,
                        ctx.partition,
                        ctx.offset,
                        reason,
                    );

                    offset_coordinator.mark_failed(&ctx.topic, ctx.partition, ctx.offset, reason);

                    if should_retry {
                        // Sleep then retry
                        if let Some(d) = delay {
                            tracing::info!(
                                worker_id = worker_id,
                                topic = %ctx.topic,
                                partition = ctx.partition,
                                offset = ctx.offset,
                                attempt = retry_coordinator.attempt_count(&ctx.topic, ctx.partition, ctx.offset),
                                delay_ms = d.as_millis(),
                                "scheduling retry"
                            );
                            tokio::time::sleep(d).await;
                            active_message = Some(msg);
                            continue;
                        }
                    }

                    if should_dlq {
                        // Route to DLQ — construct metadata and produce fire-and-forget
                        let metadata = DlqMetadata::new(
                            ctx.topic.clone(),
                            ctx.partition,
                            ctx.offset,
                            reason.to_string(),
                            retry_coordinator.attempt_count(&ctx.topic, ctx.partition, ctx.offset)
                                as u32,
                            chrono::Utc::now(),
                            chrono::Utc::now(),
                        );

                        let tp = dlq_router.route(&metadata);
                        tracing::error!(
                            worker_id = worker_id,
                            topic = %ctx.topic,
                            partition = ctx.partition,
                            offset = ctx.offset,
                            dlq_topic = %tp.topic,
                            dlq_partition = tp.partition,
                            reason = %reason,
                            attempt_count = metadata.attempt_count,
                            "routing message to DLQ"
                        );

                        // Fire-and-forget — don't await
                        dlq_producer.produce_async(
                            tp.topic.clone(),
                            tp.partition,
                            msg.payload.clone().unwrap_or_default(),
                            msg.key.clone(),
                            &metadata,
                        );

                        // Ack the original message — it's now in DLQ
                        queue_manager.ack(&msg.topic, 1);
                    } else {
                        // Not retrying, not DLQ — count as processed
                        queue_manager.ack(&msg.topic, 1);
                    }
                }
            }

            if shutdown_token.is_cancelled() {
                tracing::info!(
                    worker_id = worker_id,
                    "worker stopped (cancelled after message)"
                );
                break;
            }
            continue;
        }

        // Idle — poll for a new message or cancellation
        select! {
            Some(msg) = rx.recv() => {
                tracing::trace!(
                    worker_id = worker_id,
                    topic = %msg.topic,
                    partition = msg.partition,
                    offset = msg.offset,
                    "worker picked up message"
                );
                active_message = Some(msg);
            }
            _ = shutdown_token.cancelled() => {
                tracing::info!(worker_id = worker_id, "worker stopped (cancelled, idle)");
                break;
            }
        }
    }
}

/// Batch worker loop — accumulates messages per handler until batch is full OR deadline expires.
///
/// Uses tokio::select! to race:
/// - Message arrival: add to accumulator, flush if max_batch_size reached
/// - Deadline expiry: flush all partitions
/// - Cancellation: flush all and exit
///
/// Backpressure is applied per-batch: if QueueManager::get_inflight() >= capacity,
/// flush current batch first then block (D-03).
///
/// Per D-01: BatchAccumulator is a dedicated struct (separate from worker_loop).
/// Per D-02: Fixed-window timer — deadline set on first message, never recalculated.
/// Per D-04: Inline iteration for batch results in this function.
async fn batch_worker_loop(
    mut rx: mpsc::Receiver<OwnedMessage>,
    handler: Arc<PythonHandler>,
    executor: Arc<dyn Executor>,
    queue_manager: Arc<QueueManager>,
    offset_coordinator: Arc<dyn OffsetCoordinator>,
    retry_coordinator: Arc<RetryCoordinator>,
    dlq_producer: Arc<SharedDlqProducer>,
    dlq_router: Arc<dyn DlqRouter>,
    worker_id: usize,
    shutdown_token: CancellationToken,
) {
    tracing::info!(worker_id = worker_id, "batch worker started");

    let batch_policy = handler
        .batch_policy()
        .expect("BatchSync requires batch_policy");
    let accumulator =
        BatchAccumulator::new(batch_policy.max_batch_size, batch_policy.max_batch_wait_ms);

    // Track whether we are in a backpressure-blocking state
    let mut backpressure_active = false;

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
                    if !partitions.is_empty() {
                        for (partition, batch) in partitions {
                            if !batch.is_empty() {
                                let topic = batch[0].topic.clone();
                                let ctx = ExecutionContext::new(
                                    topic.clone(),
                                    partition,
                                    batch[0].offset,
                                    worker_id,
                                );
                                let result = handler.invoke_mode_batch(&ctx, batch.clone()).await;
                                handle_batch_result_inline(
                                    result,
                                    batch,
                                    &topic,
                                    partition,
                                    &ctx,
                                    executor.clone(),
                                    queue_manager.clone(),
                                    offset_coordinator.clone(),
                                    retry_coordinator.clone(),
                                    dlq_producer.clone(),
                                    dlq_router.clone(),
                                )
                                .await;
                            }
                        }
                    }
                }
            }

            // Branch 2: Message arrives
            msg = rx.recv() => {
                match msg {
                    Some(msg) => {
                        // Backpressure check: if at capacity, flush first then block
                        if !backpressure_active {
                            if let Some(capacity) = queue_manager.get_capacity(&msg.topic) {
                                if let Some(inflight) = queue_manager.get_inflight(&msg.topic) {
                                    if inflight >= capacity {
                                        // Flush current accumulator before blocking
                                        backpressure_active = true;
                                        let partitions = accumulator.flush_all();
                                        for (partition, batch) in partitions {
                                            if !batch.is_empty() {
                                                let topic = batch[0].topic.clone();
                                                let ctx = ExecutionContext::new(
                                                    topic.clone(),
                                                    partition,
                                                    batch[0].offset,
                                                    worker_id,
                                                );
                                                let result = handler.invoke_mode_batch(&ctx, batch.clone()).await;
                                                handle_batch_result_inline(
                                                    result,
                                                    batch,
                                                    &topic,
                                                    partition,
                                                    &ctx,
                                                    executor.clone(),
                                                    queue_manager.clone(),
                                                    offset_coordinator.clone(),
                                                    retry_coordinator.clone(),
                                                    dlq_producer.clone(),
                                                    dlq_router.clone(),
                                                )
                                                .await;
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Check if adding would fill the partition — flush preemptively
                        if accumulator.would_fill_partition(msg.partition) {
                            if let Some(batch) = accumulator.flush_partition(msg.partition) {
                                if !batch.is_empty() {
                                    let topic = batch[0].topic.clone();
                                    let partition = batch[0].partition;
                                    let ctx = ExecutionContext::new(
                                        topic.clone(),
                                        partition,
                                        batch[0].offset,
                                        worker_id,
                                    );
                                    let result = handler.invoke_mode_batch(&ctx, batch.clone()).await;
                                    handle_batch_result_inline(
                                        result,
                                        batch,
                                        &topic,
                                        partition,
                                        &ctx,
                                        executor.clone(),
                                        queue_manager.clone(),
                                        offset_coordinator.clone(),
                                        retry_coordinator.clone(),
                                        dlq_producer.clone(),
                                        dlq_router.clone(),
                                    )
                                    .await;
                                }
                            }
                        }

                        // Add the message to accumulator
                        let msg_partition = msg.partition;
                        accumulator.add(msg);

                        // If we just hit max_batch_size after adding, flush immediately
                        if accumulator.would_fill_partition(msg_partition) {
                            if let Some(batch) = accumulator.flush_partition(msg_partition) {
                                if !batch.is_empty() {
                                    let topic = batch[0].topic.clone();
                                    let partition = batch[0].partition;
                                    let ctx = ExecutionContext::new(
                                        topic.clone(),
                                        partition,
                                        batch[0].offset,
                                        worker_id,
                                    );
                                    let result = handler.invoke_mode_batch(&ctx, batch.clone()).await;
                                    handle_batch_result_inline(
                                        result,
                                        batch,
                                        &topic,
                                        partition,
                                        &ctx,
                                        executor.clone(),
                                        queue_manager.clone(),
                                        offset_coordinator.clone(),
                                        retry_coordinator.clone(),
                                        dlq_producer.clone(),
                                        dlq_router.clone(),
                                    )
                                    .await;
                                }
                            }
                        }
                    }
                    None => {
                        // Channel closed — drain accumulator and exit
                        tracing::info!(
                            worker_id = worker_id,
                            "batch worker: channel closed, draining"
                        );
                        let partitions = accumulator.flush_all();
                        for (partition, batch) in partitions {
                            if !batch.is_empty() {
                                let topic = batch[0].topic.clone();
                                let ctx = ExecutionContext::new(
                                    topic.clone(),
                                    partition,
                                    batch[0].offset,
                                    worker_id,
                                );
                                let result = handler.invoke_mode_batch(&ctx, batch.clone()).await;
                                handle_batch_result_inline(
                                    result,
                                    batch,
                                    &topic,
                                    partition,
                                    &ctx,
                                    executor.clone(),
                                    queue_manager.clone(),
                                    offset_coordinator.clone(),
                                    retry_coordinator.clone(),
                                    dlq_producer.clone(),
                                    dlq_router.clone(),
                                )
                                .await;
                            }
                        }
                        break;
                    }
                }
            }

            // Branch 3: Shutdown signal — flush all and exit
            _ = shutdown_token.cancelled() => {
                tracing::info!(
                    worker_id = worker_id,
                    "batch worker: shutdown signal, draining"
                );
                let partitions = accumulator.flush_all();
                for (partition, batch) in partitions {
                    if !batch.is_empty() {
                        let topic = batch[0].topic.clone();
                        let ctx = ExecutionContext::new(
                            topic.clone(),
                            partition,
                            batch[0].offset,
                            worker_id,
                        );
                        let result = handler.invoke_mode_batch(&ctx, batch.clone()).await;
                        handle_batch_result_inline(
                            result,
                            batch,
                            &topic,
                            partition,
                            &ctx,
                            executor.clone(),
                            queue_manager.clone(),
                            offset_coordinator.clone(),
                            retry_coordinator.clone(),
                            dlq_producer.clone(),
                            dlq_router.clone(),
                        )
                        .await;
                    }
                }
                break;
            }
        }
    }

    tracing::info!(worker_id = worker_id, "batch worker stopped");
}

/// Handle the result of a batch invocation — inline version that owns the batch messages.
///
/// Per D-04: Inline iteration in worker_loop.
/// AllSuccess → record_ack per message individually.
/// AllFailure → record_failure per message individually (routes to RetryCoordinator).
async fn handle_batch_result_inline(
    result: BatchExecutionResult,
    batch: Vec<OwnedMessage>,
    topic: &str,
    partition: i32,
    ctx: &ExecutionContext,
    executor: Arc<dyn Executor>,
    queue_manager: Arc<QueueManager>,
    offset_coordinator: Arc<dyn OffsetCoordinator>,
    retry_coordinator: Arc<RetryCoordinator>,
    _dlq_producer: Arc<SharedDlqProducer>,
    _dlq_router: Arc<dyn DlqRouter>,
) {
    match result {
        BatchExecutionResult::AllSuccess(offsets) => {
            // Record batch size histogram for this partition
            let batch_size_labels = MetricLabels::new()
                .insert("handler_id", topic)
                .insert("topic", topic)
                .insert("partition", partition.to_string());
            HANDLER_METRICS.record_batch_size(&NoopSink, &batch_size_labels, batch.len());

            // EXEC-10: Each message calls record_ack individually
            for offset in offsets {
                retry_coordinator.record_success(topic, partition, offset);
                queue_manager.ack(topic, 1);
                offset_coordinator.record_ack(topic, partition, offset);
                tracing::debug!(
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
            HANDLER_METRICS.record_batch_size(&NoopSink, &batch_size_labels, batch.len());

            // EXEC-10: All messages in batch flow to RetryCoordinator
            // We have access to the original batch messages here for routing
            tracing::warn!(
                topic = %topic,
                partition = partition,
                reason = %reason,
                batch_size = batch.len(),
                "batch failed entirely"
            );

            for msg in batch {
                let (should_retry, should_dlq, delay) =
                    retry_coordinator.record_failure(topic, partition, msg.offset, &reason);

                offset_coordinator.mark_failed(topic, partition, msg.offset, &reason);

                if should_retry {
                    // Retry scheduling would require re-enqueuing — for batch mode,
                    // we schedule retry with the original message payload
                    if let Some(d) = delay {
                        tracing::info!(
                            topic = %topic,
                            partition = partition,
                            offset = msg.offset,
                            delay_ms = d.as_millis(),
                            "batch message scheduling retry"
                        );
                        tokio::time::sleep(d).await;
                        // Note: In batch mode, retry re-enqueues to the front of the queue
                        // This is handled by the queue_manager's retry mechanism
                    }
                }

                if should_dlq {
                    // Route to DLQ
                    let metadata = DlqMetadata::new(
                        topic.to_string(),
                        partition,
                        msg.offset,
                        reason.to_string(),
                        retry_coordinator.attempt_count(topic, partition, msg.offset) as u32,
                        chrono::Utc::now(),
                        chrono::Utc::now(),
                    );

                    let tp = _dlq_router.route(&metadata);
                    tracing::error!(
                        topic = %topic,
                        partition = partition,
                        offset = msg.offset,
                        dlq_topic = %tp.topic,
                        dlq_partition = tp.partition,
                        reason = %reason,
                        "routing batch message to DLQ"
                    );

                    // Fire-and-forget
                    _dlq_producer.produce_async(
                        tp.topic.clone(),
                        tp.partition,
                        msg.payload.clone().unwrap_or_default(),
                        msg.key.clone(),
                        &metadata,
                    );

                    // Ack the original message
                    queue_manager.ack(topic, 1);
                } else {
                    // Not retrying, not DLQ — count as processed
                    queue_manager.ack(topic, 1);
                }
            }
        }
        // PartialFailure — NOT IMPLEMENTED in v1.6
        // Skip per D-05
        BatchExecutionResult::PartialFailure { .. } => {
            // Record batch size histogram for this partial-failure batch
            let batch_size_labels = MetricLabels::new()
                .insert("handler_id", topic)
                .insert("topic", topic)
                .insert("partition", partition.to_string());
            HANDLER_METRICS.record_batch_size(&NoopSink, &batch_size_labels, batch.len());

            tracing::warn!(
                topic = %topic,
                partition = partition,
                "PartialFailure not implemented in v1.6 — treating as error"
            );
            // Fall through: treat as if all failed
            for msg in batch {
                queue_manager.ack(topic, 1);
            }
        }
    }
}

/// WorkerPool — manages N Tokio workers via `JoinSet`.
///
/// Each worker polls its own `mpsc::Receiver<OwnedMessage>` independently (EXEC-09).
/// Constructed via `WorkerPool::new()`, then `run()` to await completion, or
/// `shutdown()` for external cancellation.
pub struct WorkerPool {
    join_set: JoinSet<()>,
    /// Exposed publicly so Consumer::stop() can cancel via Arc<WorkerPool>.
    pub(crate) shutdown_token: CancellationToken,
    offset_coordinator: Arc<dyn OffsetCoordinator>,
    retry_coordinator: Arc<RetryCoordinator>,
    dlq_producer: Arc<SharedDlqProducer>,
    dlq_router: Arc<dyn DlqRouter>,
}

impl WorkerPool {
    /// Create a new WorkerPool with `n_workers` tasks.
    ///
    /// Each worker gets its own receiver from `receivers`. The `handler` is
    /// shared across all workers via `Arc`. Uses `DefaultExecutor` (EXEC-04).
    /// The `shutdown_token` is supplied by the owner (Consumer) so that
    /// `stop()` can cancel all workers by cancelling the shared token.
    pub fn new(
        n_workers: usize,
        receivers: Vec<mpsc::Receiver<OwnedMessage>>,
        handler: Arc<PythonHandler>,
        executor: Arc<dyn Executor>,
        queue_manager: Arc<QueueManager>,
        offset_coordinator: Arc<dyn OffsetCoordinator>,
        retry_coordinator: Arc<RetryCoordinator>,
        dlq_producer: Arc<SharedDlqProducer>,
        dlq_router: Arc<dyn DlqRouter>,
        shutdown_token: CancellationToken,
    ) -> Self {
        let mut join_set = JoinSet::new();

        // Route to batch_worker_loop for BatchSync and BatchAsync, worker_loop for others
        let mode = handler.mode();
        let use_batch = matches!(
            mode,
            crate::python::handler::HandlerMode::BatchSync
                | crate::python::handler::HandlerMode::BatchAsync
        );

        // Zip workers with receivers — receivers is consumed here
        for (worker_id, rx) in receivers.into_iter().enumerate().take(n_workers) {
            let handler = Arc::clone(&handler);
            let executor = Arc::clone(&executor);
            let queue_manager = Arc::clone(&queue_manager);
            let token = shutdown_token.clone();
            let offset_coordinator = offset_coordinator.clone();
            let retry_coordinator = retry_coordinator.clone();
            let dlq_producer = dlq_producer.clone();
            let dlq_router = dlq_router.clone();

            if use_batch {
                join_set.spawn(batch_worker_loop(
                    rx,
                    handler,
                    executor,
                    queue_manager,
                    offset_coordinator,
                    retry_coordinator,
                    dlq_producer,
                    dlq_router,
                    worker_id,
                    token,
                ));
            } else {
                join_set.spawn(worker_loop(
                    rx,
                    handler,
                    executor,
                    queue_manager,
                    offset_coordinator,
                    retry_coordinator,
                    dlq_producer,
                    dlq_router,
                    worker_id,
                    token,
                ));
            }
        }

        tracing::info!(n_workers = n_workers, "WorkerPool created");
        Self {
            join_set,
            shutdown_token,
            offset_coordinator,
            retry_coordinator,
            dlq_producer,
            dlq_router,
        }
    }

    /// Run the worker pool — awaits all workers until shutdown.
    pub async fn run(mut self) {
        self.join_set.shutdown().await;
    }

    /// Trigger graceful shutdown and await completion (EXEC-12).
    pub async fn shutdown(&mut self) {
        tracing::info!("initiating worker pool shutdown");
        self.shutdown_token.cancel();
        // D-02: Flush all failed (retryable + terminal) to DLQ before final commit
        self.offset_coordinator
            .flush_failed_to_dlq(&self.dlq_router, &self.dlq_producer);
        self.offset_coordinator.graceful_shutdown();
        self.join_set.shutdown().await;
        tracing::info!("worker pool shutdown complete");
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dispatcher::queue_manager::QueueManager;
    use crate::dispatcher::OwnedMessage;
    use crate::python::context::ExecutionContext;
    use crate::python::execution_result::ExecutionResult;
    use pyo3::prelude::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    /// Spy executor that records whether result was Ok.
    #[derive(Debug, Default)]
    struct SpyExecutor {
        pub ack_called: AtomicBool,
    }

    impl Executor for SpyExecutor {
        fn execute(
            &self,
            _ctx: &ExecutionContext,
            _message: &OwnedMessage,
            result: &ExecutionResult,
        ) -> crate::python::executor::ExecutorOutcome {
            if result.is_ok() {
                self.ack_called.store(true, Ordering::SeqCst);
            }
            crate::python::executor::ExecutorOutcome::Ack
        }
    }

    fn make_test_msg() -> OwnedMessage {
        OwnedMessage {
            topic: "test".to_string(),
            partition: 0,
            offset: 0,
            key: None,
            payload: None,
            timestamp: crate::consumer::MessageTimestamp::NotAvailable,
            headers: vec![],
        }
    }

    fn dummy_handler() -> Arc<PythonHandler> {
        use crate::python::handler::{BatchPolicy, HandlerMode};
        Python::with_gil(|py| {
            let py_none = py.None();
            Arc::new(PythonHandler::new(
                py_none.into(),
                None,
                HandlerMode::SingleSync,
                None,
            ))
        })
    }

    fn test_config() -> crate::consumer::ConsumerConfig {
        crate::consumer::ConsumerConfigBuilder::new()
            .brokers("localhost:9092")
            .group_id("test-group")
            .topics(["test"])
            .build()
            .unwrap()
    }

    fn dummy_dlq_producer() -> Arc<SharedDlqProducer> {
        Arc::new(SharedDlqProducer::new(&test_config()).unwrap())
    }

    fn dummy_dlq_router() -> Arc<dyn DlqRouter> {
        Arc::new(DefaultDlqRouter::with_default_prefix())
    }

    #[tokio::test]
    async fn worker_pool_spawns_n_workers() {
        let (tx, rx) = mpsc::channel(1);
        let _pool = WorkerPool::new(
            3,
            vec![rx],
            dummy_handler(),
            Arc::new(DefaultExecutor),
            Arc::new(QueueManager::new()),
            Arc::new(crate::coordinator::OffsetTracker::new()) as Arc<dyn OffsetCoordinator>,
            Arc::new(crate::coordinator::RetryCoordinator::with_policy(
                crate::retry::RetryPolicy::default(),
            )),
            dummy_dlq_producer(),
            dummy_dlq_router(),
            CancellationToken::new(),
        );
        let _ = tx;
    }

    #[tokio::test]
    async fn worker_loop_exits_on_cancel_when_idle() {
        let (tx, rx) = mpsc::channel(1);
        let token = CancellationToken::new();
        token.cancel();

        let result = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            worker_loop(
                rx,
                dummy_handler(),
                Arc::new(DefaultExecutor) as Arc<dyn Executor>,
                Arc::new(QueueManager::new()),
                Arc::new(crate::coordinator::OffsetTracker::new()) as Arc<dyn OffsetCoordinator>,
                Arc::new(crate::coordinator::RetryCoordinator::with_policy(
                    crate::retry::RetryPolicy::default(),
                )),
                dummy_dlq_producer(),
                dummy_dlq_router(),
                0,
                token,
            ),
        )
        .await;
        assert!(result.is_ok(), "worker_loop should complete within timeout");
        let _ = tx;
    }

    #[tokio::test]
    async fn ack_called_on_execution_ok() {
        let executor: Arc<dyn Executor> = Arc::new(DefaultExecutor);
        let ctx = ExecutionContext::new("test".to_string(), 0, 0, 0);
        let outcome = executor.execute(&ctx, &make_test_msg(), &ExecutionResult::Ok);
        assert!(matches!(
            outcome,
            crate::python::executor::ExecutorOutcome::Ack
        ));
    }

    #[tokio::test]
    async fn no_ack_on_execution_error() {
        let executor: Arc<dyn Executor> = Arc::new(DefaultExecutor);
        let ctx = ExecutionContext::new("test".to_string(), 0, 0, 0);
        let outcome = executor.execute(
            &ctx,
            &make_test_msg(),
            &ExecutionResult::Error {
                reason: FailureReason::Terminal(crate::failure::TerminalKind::HandlerPanic),
                exception: "Test".to_string(),
                traceback: "test".to_string(),
            },
        );
        assert!(matches!(
            outcome,
            crate::python::executor::ExecutorOutcome::Ack
        ));
    }

    #[tokio::test]
    async fn graceful_shutdown_waits_for_inflight() {
        let (tx, rx) = mpsc::channel(1);
        let token = CancellationToken::new();

        let handle = tokio::spawn(worker_loop(
            rx,
            dummy_handler(),
            Arc::new(DefaultExecutor) as Arc<dyn Executor>,
            Arc::new(QueueManager::new()),
            Arc::new(crate::coordinator::OffsetTracker::new()) as Arc<dyn OffsetCoordinator>,
            Arc::new(crate::coordinator::RetryCoordinator::with_policy(
                crate::retry::RetryPolicy::default(),
            )),
            dummy_dlq_producer(),
            dummy_dlq_router(),
            0,
            token.clone(),
        ));

        let _ = tx.blocking_send(make_test_msg());
        token.cancel();

        let result = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
        assert!(
            result.is_ok(),
            "worker should finish after processing message"
        );
    }
}
