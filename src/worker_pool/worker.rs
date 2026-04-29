//! Worker loop — polls messages and invokes the Python handler.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::select;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::coordinator::OffsetCoordinator;
use crate::coordinator::RetryCoordinator;
use crate::dispatcher::queue_manager::QueueManager;
use crate::dispatcher::OwnedMessage;
use crate::dlq::{DlqRouter, SharedDlqProducer};
use crate::observability::metrics::{MetricLabels, ThroughputMetrics};
use crate::observability::runtime_snapshot::WorkerPoolState;
use crate::observability::tracing::KafpySpanExt;
use crate::python::context::ExecutionContext;
use crate::python::execution_result::ExecutionResult;
use crate::python::executor::Executor;
use crate::python::handler::PythonHandler;
use crate::python::logger;
use crate::worker_pool::handle_execution_failure;
use crate::worker_pool::state::WorkerState;
use crate::worker_pool::ExecutionAction;
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
        tracing::warn!(topic = %topic, "no handler registered for topic, using first available");
        handlers.values().next().expect("handler map is empty")
    })
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn worker_loop(
    mut rx: mpsc::Receiver<OwnedMessage>,
    handlers: Arc<HashMap<String, Arc<PythonHandler>>>,
    executor: Arc<dyn Executor>,
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
    logger::log("INFO", &format!("worker started worker_id={}", worker_id));

    let mut state = WorkerState::Idle;

    loop {
        // Poll for a message or handle cancellation when idle
        if matches!(state, WorkerState::Idle) {
            select! {
                Some(msg) = rx.recv() => {
                    tracing::trace!(
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
                    state = WorkerState::Processing(msg);
                }
                _ = shutdown_token.cancelled() => {
                    logger::log("INFO", &format!("worker stopped (cancelled, idle) worker_id={}", worker_id));
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

            let ctx = ExecutionContext::with_trace(
                msg.topic.clone(),
                msg.partition,
                msg.offset,
                worker_id,
                trace_id,
                span_id,
                trace_flags,
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
            logger::log("INFO", &format!(
                "handler invoke start handler_id={} handler_name={} topic={} partition={} offset={} mode={}",
                ctx.topic, handler.name(), ctx.topic, ctx.partition, ctx.offset, handler.mode().as_str()
            ));
            // Acquire concurrency permit — holds until end of this block
            let _permit = handler_concurrency.acquire(&ctx.topic).await;
            let result = span
                .in_scope(|| async { handler.invoke_mode_with_timeout(&ctx, msg.clone()).await })
                .await;
            let elapsed = start.elapsed();
            logger::log("INFO", &format!(
                "handler invoke complete handler_id={} topic={} partition={} offset={} elapsed_ms={}",
                ctx.topic, ctx.topic, ctx.partition, ctx.offset, elapsed.as_millis() as u64
            ));
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

                    offset_coordinator.mark_failed(&ctx.topic, ctx.partition, ctx.offset, reason);

                    let action = handle_execution_failure(
                        &ctx,
                        &msg,
                        reason,
                        Arc::clone(&retry_coordinator),
                        Arc::clone(&dlq_producer),
                        Arc::clone(&dlq_router),
                        Arc::clone(&queue_manager),
                    )
                    .await;

                    match action {
                        ExecutionAction::Ack => {}
                        ExecutionAction::Retry { delay } => {
                            tokio::time::sleep(delay).await;
                            state = WorkerState::Processing(msg);
                            continue;
                        }
                        ExecutionAction::Dlq => {}
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
                    let exc_name = "Rejected";
                    crate::failure::logging::log_failure(&ctx, reason, exc_name, false);

                    offset_coordinator.mark_failed(&ctx.topic, ctx.partition, ctx.offset, reason);

                    let action = handle_execution_failure(
                        &ctx,
                        &msg,
                        reason,
                        Arc::clone(&retry_coordinator),
                        Arc::clone(&dlq_producer),
                        Arc::clone(&dlq_router),
                        Arc::clone(&queue_manager),
                    )
                    .await;

                    match action {
                        ExecutionAction::Ack => {}
                        ExecutionAction::Retry { delay } => {
                            tokio::time::sleep(delay).await;
                            state = WorkerState::Processing(msg);
                            continue;
                        }
                        ExecutionAction::Dlq => {}
                    }
                }
            }

            if shutdown_token.is_cancelled() {
                logger::log(
                    "INFO",
                    &format!(
                        "worker stopped (cancelled after message) worker_id={}",
                        worker_id
                    ),
                );
                worker_pool_state.set_idle(worker_id);
                break;
            }
            worker_pool_state.set_idle(worker_id);
            state = WorkerState::Idle;
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinator::OffsetCoordinator;
    use crate::dispatcher::queue_manager::QueueManager;
    use crate::dispatcher::OwnedMessage;
    use crate::dlq::router::DefaultDlqRouter;
    use crate::observability::runtime_snapshot::WorkerPoolState;
    use crate::python::DefaultExecutor;
    use pyo3::prelude::*;
    use std::sync::Arc;

    fn make_handler_map() -> Arc<HashMap<String, Arc<PythonHandler>>> {
        use crate::python::handler::HandlerMode;
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
