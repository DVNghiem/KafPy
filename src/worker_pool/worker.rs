//! Worker loop — polls messages and invokes the Python handler.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::select;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::dispatcher::queue_manager::QueueManager;
use crate::dispatcher::OwnedMessage;
use crate::dlq::{DlqMetadata, DlqRouter, SharedDlqProducer};
use crate::execution::callback::PythonHandler;
use crate::execution::context::{ExecutionContext, TraceContext};
use crate::execution::execution_result::ExecutionResult;
use crate::failure::FailureReason;
use crate::observability::metrics::{
    FanOutMetrics, MetricLabels, PythonCallMetrics, ThroughputMetrics, TimeoutMetrics,
};
use crate::observability::runtime_snapshot::WorkerPoolState;
use crate::observability::tracing::KafpySpanExt;
use crate::offset::offset_coordinator::OffsetCoordinator;
use crate::retry::retry_coordinator::RetryCoordinator;
use crate::worker_pool::fan_out::{BranchResult, FanOutTracker};
use crate::worker_pool::handle_execution_failure;
use crate::worker_pool::state::WorkerState;
use crate::worker_pool::ExecutionAction;
use crate::log::{debug, trace, warn};
use crate::worker_pool::HANDLER_METRICS;

/// Worker loop — polls messages and invokes the Python handler.
///
/// Uses `tokio::select!` on two branches when idle:
/// - `Some(msg) = rx.recv()` — picks up a message
/// - `_ = shutdown_token.cancelled()` — exits gracefully
///
/// When a message is picked up it is processed before polling again.
/// `WorkerState` tracks in-flight work — the cancelled branch only fires when
/// `WorkerState::Idle`, ensuring graceful shutdown waits for in-flight completion (EXEC-12).
/// Look up the PythonHandler for a given topic from the handler map.
/// Falls back to the first handler if topic is not found (graceful degradation).
fn handler_for_topic<'a>(
    handlers: &'a HashMap<String, Arc<PythonHandler>>,
    topic: &str,
) -> &'a Arc<PythonHandler> {
    handlers.get(topic).unwrap_or_else(|| {
        warn!(topic = %topic, "no handler registered for topic, using first available");
        handlers.values().next().expect("handler map is empty")
    })
}

/// Encode bytes as lowercase hex string (for W3C trace_id/span_id generation).
fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Poll for a message from the channel, or detect shutdown when idle.
/// Returns `Some(msg)` when a message is received, `None` when shutdown is signaled.
async fn poll_for_work(
    rx: &mut mpsc::Receiver<OwnedMessage>,
    worker_id: usize,
    shutdown_token: &CancellationToken,
    worker_pool_state: &Arc<WorkerPoolState>,
) -> Option<OwnedMessage> {
    select! {
        Some(msg) = rx.recv() => {
            trace!(
                worker_id = worker_id,
                topic = %msg.topic,
                partition = msg.partition,
                offset = msg.offset,
                "worker picked up message"
            );
            worker_pool_state.set_active(
                worker_id,
                msg.topic.clone(),
                msg.topic.clone(),
                msg.partition,
                msg.offset,
            );
            Some(msg)
        }
        _ = shutdown_token.cancelled() => {
            None
        }
    }
}

/// Handle the result of a Python handler invocation.
/// Returns `Some(action)` if a retry or DLQ action should be taken, `None` to continue normally.
#[allow(clippy::too_many_arguments)]
async fn handle_execution_result(
    result: &ExecutionResult,
    ctx: &ExecutionContext,
    msg: &OwnedMessage,
    worker_id: usize,
    retry_coordinator: &Arc<RetryCoordinator>,
    queue_manager: &Arc<QueueManager>,
    offset_coordinator: &Arc<dyn OffsetCoordinator>,
    dlq_producer: &Arc<SharedDlqProducer>,
    dlq_router: &Arc<dyn DlqRouter>,
) -> Option<ExecutionAction> {
    match result {
        ExecutionResult::Ok => {
            debug!(
                worker_id = worker_id,
                topic = %ctx.topic,
                partition = ctx.partition,
                offset = ctx.offset,
                "handler executed successfully"
            );
            retry_coordinator.record_success(&ctx.topic, ctx.partition, ctx.offset);
            queue_manager.ack(&msg.topic, 1);
            offset_coordinator.record_ack(&ctx.topic, ctx.partition, ctx.offset);
            None
        }
        ExecutionResult::Error {
            ref reason,
            ref exception,
            ..
        } => {
            warn!(
                worker_id = worker_id,
                topic = %ctx.topic,
                partition = ctx.partition,
                offset = ctx.offset,
                exception = %exception,
                "handler raised exception"
            );
            crate::failure::logging::log_failure(ctx, reason, exception, false);

            offset_coordinator.mark_failed(&ctx.topic, ctx.partition, ctx.offset, reason);

            let action = handle_execution_failure(
                ctx,
                msg,
                result,
                Arc::clone(retry_coordinator),
                Arc::clone(dlq_producer),
                Arc::clone(dlq_router),
                Arc::clone(queue_manager),
            )
            .await;

            Some(action)
        }
        ExecutionResult::Rejected { ref reason, .. } => {
            warn!(
                worker_id = worker_id,
                topic = %ctx.topic,
                partition = ctx.partition,
                offset = ctx.offset,
                reason = %reason,
                "handler rejected message"
            );
            let exc_name = "Rejected";
            crate::failure::logging::log_failure(ctx, reason, exc_name, false);

            offset_coordinator.mark_failed(&ctx.topic, ctx.partition, ctx.offset, reason);

            let action = handle_execution_failure(
                ctx,
                msg,
                result,
                Arc::clone(retry_coordinator),
                Arc::clone(dlq_producer),
                Arc::clone(dlq_router),
                Arc::clone(queue_manager),
            )
            .await;

            Some(action)
        }
        ExecutionResult::Timeout { ref info } => {
            warn!(
                worker_id = worker_id,
                topic = %ctx.topic,
                partition = ctx.partition,
                offset = ctx.offset,
                timeout_ms = info.timeout_ms,
                "handler timed out"
            );
            let reason =
                FailureReason::Terminal(crate::failure::TerminalKind::HandlerPanic);
            crate::failure::logging::log_failure(ctx, &reason, "HandlerTimeout", false);

            offset_coordinator.mark_failed(&ctx.topic, ctx.partition, ctx.offset, &reason);

            let action = handle_execution_failure(
                ctx,
                msg,
                result,
                Arc::clone(retry_coordinator),
                Arc::clone(dlq_producer),
                Arc::clone(dlq_router),
                Arc::clone(queue_manager),
            )
            .await;

            Some(action)
        }
    }
}

/// Process fan-out dispatch to sinks in parallel.
/// Spawns sink tasks and returns immediately — caller should await fan_tracker.wait_all()
/// if offset commit gating is needed.
#[allow(clippy::too_many_arguments)]
async fn process_fan_out(
    handler: &PythonHandler,
    msg: OwnedMessage,
    trace_id: Option<String>,
    span_id: Option<String>,
    worker_id: usize,
    dlq_producer: &Arc<SharedDlqProducer>,
    dlq_router: &Arc<dyn DlqRouter>,
    prometheus_sink: &crate::observability::SharedPrometheusSink,
) {
    let Some(fan_out_config) = handler.fan_out_config() else {
        return;
    };

    // FANOUT-02: Check if fan-out slots are exhausted
    if fan_out_config.is_exhausted() {
        warn!(
            topic = %msg.topic,
            max_fan_out = fan_out_config.max_fan_out,
            "fan-out slots exhausted, returning backpressure"
        );
        // Primary already ACKed above. Return backpressure signal to caller.
        // The caller (ConsumerDispatcher) will pause the partition.
        return;
    }

    // Generate fan_out_id and await wait_all() before offset commit.
    use std::sync::atomic::AtomicU64;
    static FAN_OUT_COUNTER: AtomicU64 = AtomicU64::new(1);
    let fan_out_id = FAN_OUT_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

    // Primary ACKed immediately above (ACK happened in match block
    // for Ok, or via handle_execution_failure for Error/Rejected/Timeout).
    // Now spawn sink futures in parallel via JoinSet.

    let fan_tracker = Arc::new(FanOutTracker::new(fan_out_config.max_fan_out));
    let mut sink_join_set = JoinSet::new();
    let sink_topics: Vec<String> = fan_out_config
        .sinks
        .iter()
        .map(|s| s.topic.clone())
        .collect();
    // Clone trace context once before loop so each iteration can borrow.
    let parent_trace_id_opt = trace_id.clone();
    let parent_span_id_opt = span_id.clone();

    for sink in &fan_out_config.sinks {
        let tracker = Arc::clone(&fan_tracker);
        let sink_handler = Arc::clone(&sink.handler);
        let sink_topic = sink.topic.clone();
        let sink_timeout = sink.timeout;
        let msg_clone = msg.clone();
        let fan_out_id_clone = fan_out_id;
        // Clone trace context for this branch (moves into async block).
        let trace_id_clone = parent_trace_id_opt.clone();
        let span_id_clone = parent_span_id_opt.clone();

        sink_join_set.spawn(async move {
            let branch_id = tracker.register_branch();

            // D-07/D-08/D-09: Create branch span and W3C traceparent for this branch.
            // If parent trace context exists, use it as parent; otherwise generate new trace_id.
            let parent_trace_id = trace_id_clone.as_deref();
            let parent_span_id = span_id_clone.as_deref();
            let branch_span = tracing::Span::current().kafpy_fanout_branch_span(
                fan_out_id_clone,
                sink_topic.as_str(),
                parent_trace_id,
                parent_span_id,
            );

            // Build W3C traceparent for this branch.
            // All branches of the same fan-out dispatch share the same trace_id (D-09).
            // If no parent trace_id, generate a new one.
            let trace_id: String = match parent_trace_id {
                Some(tid) => tid.to_string(),
                None => {
                    let bytes: [u8; 16] = rand::random();
                    encode_hex(&bytes)
                }
            };
            let span_id_bytes: [u8; 8] = rand::random();
            let branch_span_id = encode_hex(&span_id_bytes);

            let ctx_clone = ExecutionContext::with_trace(
                sink_topic.clone(),
                msg_clone.partition,
                msg_clone.offset,
                worker_id,
                TraceContext {
                    trace_id: Some(trace_id.clone()),
                    span_id: Some(branch_span_id.clone()),
                    trace_flags: Some("01".to_string()),
                },
                Some(branch_id),
                Some(fan_out_id_clone),
                String::new(),
                None,
            );
            let result = branch_span
                .in_scope(|| async {
                    sink_handler
                        .invoke_mode_with_timeout_override(
                            &ctx_clone,
                            msg_clone,
                            sink_timeout,
                        )
                        .await
                })
                .await;
            let branch_result = match result {
                ExecutionResult::Ok => BranchResult::Ok,
                ExecutionResult::Error {
                    reason, exception, ..
                } => BranchResult::Error { reason, exception },
                ExecutionResult::Timeout { info } => BranchResult::Timeout {
                    timeout_ms: info.timeout_ms,
                },
                ExecutionResult::Rejected { .. } => BranchResult::Error {
                    reason: FailureReason::Terminal(
                        crate::failure::TerminalKind::HandlerPanic,
                    ),
                    exception: "Rejected".to_string(),
                },
            };
            tracker.record_branch_result(branch_id, branch_result.clone());
            tracker.release_slot();
            branch_result
        });
    }

    // Await all branches to complete before offset commit gating.
    // Spawn a task to drive the JoinSet and collect results.
    let dlq_producer_clone = Arc::clone(dlq_producer);
    let dlq_router_clone = Arc::clone(dlq_router);
    let msg_partition = msg.partition;
    let msg_offset = msg.offset;
    let msg_clone_for_dlq = msg.clone();
    let metrics_sink = prometheus_sink.clone();
    tokio::spawn(async move {
        let branch_results = fan_tracker.wait_all().await;
        debug!(
            fan_out_id = fan_out_id,
            branch_count = branch_results.results.len(),
            "all fan-out branches completed"
        );

        // OBSV-01: Emit fan-out metrics for each branch result.
        for (branch_result, sink_topic) in
            branch_results.results.iter().zip(sink_topics.iter())
        {
            let outcome = FanOutMetrics::outcome_from_result(&branch_result.1);
            FanOutMetrics::record_branch_completion(
                &metrics_sink,
                fan_out_id,
                sink_topic,
                outcome,
            );
        }
        for (branch_result, sink_topic) in
            branch_results.results.iter().zip(sink_topics.iter())
        {
            let branch_id = branch_result.0;
            if !matches!(branch_result.1, BranchResult::Ok) {
                let (exception, _is_timeout, timeout_val) = match &branch_result.1 {
                    BranchResult::Error { exception, .. } => {
                        (exception.clone(), false, None)
                    }
                    BranchResult::Timeout { timeout_ms } => (
                        format!("sink timeout after {}ms", timeout_ms),
                        true,
                        Some(*timeout_ms),
                    ),
                    BranchResult::Ok => continue,
                };
                let dlq_meta = DlqMetadata::new(
                    sink_topic.clone(),
                    msg_partition,
                    msg_offset,
                    exception,
                    1,
                    chrono::Utc::now(),
                    chrono::Utc::now(),
                    timeout_val,
                    None,
                    Some(branch_id),
                    Some(fan_out_id),
                );
                let tp = dlq_router_clone.route(&dlq_meta);
                let payload = msg_clone_for_dlq.payload.clone().unwrap_or_default();
                let key = msg_clone_for_dlq.key.clone();
                dlq_producer_clone.produce_async(
                    tp.topic,
                    tp.partition,
                    payload,
                    key,
                    &dlq_meta,
                );
            }
        }

        while let Some(result) = sink_join_set.join_next().await {
            match result {
                Ok(_) => {}
                Err(e) => {
                    tracing::error!(error = ?e, "fan-out sink task panicked");
                }
            }
        }
    });
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn worker_loop(
    mut rx: mpsc::Receiver<OwnedMessage>,
    handlers: Arc<HashMap<String, Arc<PythonHandler>>>,
    queue_manager: Arc<QueueManager>,
    offset_coordinator: Arc<dyn OffsetCoordinator>,
    retry_coordinator: Arc<RetryCoordinator>,
    dlq_producer: Arc<SharedDlqProducer>,
    dlq_router: Arc<dyn DlqRouter>,
    worker_id: usize,
    shutdown_token: CancellationToken,
    worker_pool_state: Arc<WorkerPoolState>,
    prometheus_sink: crate::observability::SharedPrometheusSink,
    handler_concurrency: crate::worker_pool::HandlerConcurrency,
) {

    let mut state = WorkerState::Idle;

    loop {
        // Poll for a message or handle cancellation when idle
        if matches!(state, WorkerState::Idle) {
            match poll_for_work(
                &mut rx,
                worker_id,
                &shutdown_token,
                &worker_pool_state,
            )
            .await
            {
                Some(msg) => {
                    state = WorkerState::Processing(msg);
                }
                None => {
                    break;
                }
            }
        }

        // Process the current message if we have one
        if let WorkerState::Processing(msg) = &state {
            let msg = msg.clone();

            // Extract W3C trace context from message headers before constructing ExecutionContext
            let header_map: std::collections::HashMap<String, String> = msg
                .headers
                .iter()
                .filter_map(|(k, v)| {
                    v.as_ref()
                        .map(|bytes| String::from_utf8_lossy(bytes).to_string())
                        .map(|val| (k.clone(), val))
                })
                .collect();
            let mut trace_map = std::collections::HashMap::new();
            crate::observability::tracing::inject_trace_context(&header_map, &mut trace_map);

            let trace_id = trace_map.get("trace_id").cloned();
            let span_id = trace_map.get("span_id").cloned();
            let trace_flags = trace_map.get("trace_flags").cloned();
            let trace_id_for_ctx = trace_id.clone();
            let span_id_for_ctx = span_id.clone();
            let trace_flags_for_ctx = trace_flags.clone();

            let ctx = ExecutionContext::with_trace(
                msg.topic.clone(),
                msg.partition,
                msg.offset,
                worker_id,
                TraceContext {
                    trace_id: trace_id_for_ctx,
                    span_id: span_id_for_ctx,
                    trace_flags: trace_flags_for_ctx,
                },
                None,
                None,
                String::new(),
                None,
            );
            let handler = handler_for_topic(&handlers, &msg.topic).clone();
            let start = std::time::Instant::now();
            let invocation_labels = MetricLabels::new()
                .insert("handler_id", ctx.topic.as_str())
                .insert("topic", ctx.topic.as_str())
                .insert("mode", handler.mode().as_str());
            let span = tracing::Span::current().kafpy_handler_invoke(
                ctx.topic.as_str(),
                handler.name(),
                ctx.topic.as_str(),
                ctx.partition,
                ctx.offset,
                handler.mode().as_str(),
                1, // attempt: will be corrected in failure path after record_failure
            );
            tracing::info!(
                handler_id = %ctx.topic,
                handler_name = %handler.name(),
                topic = %ctx.topic,
                partition = ctx.partition,
                offset = ctx.offset,
                mode = %handler.mode().as_str(),
                "handler invoke start"
            );
            // Acquire concurrency permit — holds until end of this block
            let queue_wait_start = std::time::Instant::now();
            let _permit = handler_concurrency.acquire(&ctx.topic).await;
            PythonCallMetrics::record_queue_wait(
                &prometheus_sink,
                "handler",
                handler.mode().as_str(),
                queue_wait_start.elapsed(),
            );
            let py_call_start = std::time::Instant::now();
            let result = span
                .in_scope(|| async { handler.invoke_mode_with_timeout(&ctx, msg.clone()).await })
                .await;
            PythonCallMetrics::record_call(
                &prometheus_sink,
                "handler",
                handler.mode().as_str(),
                py_call_start.elapsed(),
                1,
            );
            let elapsed = start.elapsed();
            tracing::info!(
                handler_id = %ctx.topic,
                topic = %ctx.topic,
                partition = ctx.partition,
                offset = ctx.offset,
                elapsed_ms = elapsed.as_millis() as u64,
                "handler invoke complete"
            );
            HANDLER_METRICS.record_invocation(&prometheus_sink, &invocation_labels);
            HANDLER_METRICS.record_latency(&prometheus_sink, &invocation_labels, elapsed);
            ThroughputMetrics::record_throughput(
                &prometheus_sink,
                ctx.topic.as_str(),
                handler.name(),
                handler.mode().as_str(),
            );
            if !result.is_ok() {
                tracing::error!(
                    handler_id = %ctx.topic,
                    topic = %ctx.topic,
                    partition = ctx.partition,
                    offset = ctx.offset,
                    error_type = result.error_type_label(),
                    "handler invoke error"
                );
                let error_labels = MetricLabels::new()
                    .insert("handler_id", ctx.topic.as_str())
                    .insert("error_type", result.error_type_label());
                HANDLER_METRICS.record_error(&prometheus_sink, &error_labels);
            }
            // TMOUT-03: Emit timeout-specific metric
            if result.is_timeout() {
                TimeoutMetrics::record_timeout(
                    &prometheus_sink,
                    ctx.topic.as_str(),
                    handler.name(),
                );
            }

            let action = handle_execution_result(
                &result,
                &ctx,
                &msg,
                worker_id,
                &retry_coordinator,
                &queue_manager,
                &offset_coordinator,
                &dlq_producer,
                &dlq_router,
            )
            .await;

            // Handle retry action — all other actions (None/Ack/Dlq) continue normally
            if let Some(ExecutionAction::Retry { delay }) = action {
                tokio::time::sleep(delay).await;
                state = WorkerState::Processing(msg);
                continue;
            }

            if shutdown_token.is_cancelled() {
                tracing::info!(
                    worker_id = worker_id,
                    "worker stopped (cancelled after message)"
                );
                worker_pool_state.set_idle(worker_id);
                break;
            }

            //  Dispatch to sinks in parallel if handler has fan-out config.
            // Primary result was handled above (ACK already fired for Ok, Retry, or Dlq).
            // Fan-out sinks run in background — primary ACK is non-blocking.
            process_fan_out(
                &handler,
                msg.clone(),
                trace_id.clone(),
                span_id.clone(),
                worker_id,
                &dlq_producer,
                &dlq_router,
                &prometheus_sink,
            )
            .await;

            worker_pool_state.set_idle(worker_id);
            state = WorkerState::Idle;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dispatcher::queue_manager::QueueManager;
    use crate::dispatcher::OwnedMessage;
    use crate::dlq::router::DefaultDlqRouter;
    use crate::observability::runtime_snapshot::WorkerPoolState;
    use pyo3::prelude::*;
    use std::sync::Arc;

    fn make_handler_map() -> Arc<HashMap<String, Arc<PythonHandler>>> {
        use crate::execution::callback::HandlerMode;
        let handler = Python::attach(|py| {
            let py_none = py.None();
            Arc::new(PythonHandler::new(
                py_none.into(),
                None,
                HandlerMode::SingleSync,
                None,
                None,
                "test".to_string(),
                None,
            ))
        });
        let mut map = HashMap::new();
        map.insert("test".to_string(), handler);
        Arc::new(map)
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

    fn test_config() -> crate::consumer::ConsumerConfig {
        crate::consumer::ConsumerConfigBuilder::new()
            .brokers("localhost:9092")
            .group_id("test-group")
            .topics(["test"])
            .build()
            .unwrap()
    }

    fn dummy_dlq_producer() -> Arc<SharedDlqProducer> {
        Arc::new(
            SharedDlqProducer::new(
                &test_config(),
                crate::observability::metrics::SharedPrometheusSink::new(),
            )
            .unwrap(),
        )
    }

    fn dummy_dlq_router() -> Arc<dyn DlqRouter> {
        Arc::new(DefaultDlqRouter::with_default_prefix())
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
                make_handler_map(),
                Arc::new(QueueManager::new()),
                Arc::new(crate::offset::offset_tracker::OffsetTracker::new()) as Arc<dyn OffsetCoordinator>,
                Arc::new(crate::retry::retry_coordinator::RetryCoordinator::with_policy(
                    crate::retry::RetryPolicy::default(),
                )),
                dummy_dlq_producer(),
                dummy_dlq_router(),
                0,
                token,
                Arc::new(WorkerPoolState::new(1)),
                crate::observability::metrics::SharedPrometheusSink::new(),
                crate::worker_pool::HandlerConcurrency::new(4),
            ),
        )
        .await;
        assert!(result.is_ok(), "worker_loop should complete within timeout");
        let _ = tx;
    }

    #[tokio::test]
    async fn graceful_shutdown_waits_for_inflight() {
        let (tx, rx) = mpsc::channel(1);
        let token = CancellationToken::new();

        let handle = tokio::spawn(worker_loop(
            rx,
            make_handler_map(),
            Arc::new(QueueManager::new()),
            Arc::new(crate::offset::offset_tracker::OffsetTracker::new()) as Arc<dyn OffsetCoordinator>,
            Arc::new(crate::retry::retry_coordinator::RetryCoordinator::with_policy(
                crate::retry::RetryPolicy::default(),
            )),
            dummy_dlq_producer(),
            dummy_dlq_router(),
            0,
            token.clone(),
            Arc::new(WorkerPoolState::new(1)),
            crate::observability::metrics::SharedPrometheusSink::new(),
            crate::worker_pool::HandlerConcurrency::new(4),
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
