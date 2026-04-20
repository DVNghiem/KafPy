//! Worker loop — polls messages and invokes the Python handler.

use std::sync::Arc;

use tokio::select;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::coordinator::RetryCoordinator;
use crate::coordinator::OffsetCoordinator;
use crate::dispatcher::queue_manager::QueueManager;
use crate::dispatcher::OwnedMessage;
use crate::dlq::{DlqRouter, SharedDlqProducer};
use crate::failure::FailureReason;
use crate::observability::metrics::MetricLabels;
use crate::observability::NoopSink;
use crate::observability::runtime_snapshot::WorkerPoolState;
use crate::observability::tracing::KafpySpanExt;
use crate::python::context::ExecutionContext;
use crate::python::execution_result::ExecutionResult;
use crate::python::executor::Executor;
use crate::python::handler::PythonHandler;
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
pub async fn worker_loop(
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
    worker_pool_state: Arc<WorkerPoolState>,
) {
    tracing::info!(worker_id = worker_id, "worker started");

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
                        "shared".to_string(),
                        msg.topic.clone(),
                        msg.partition,
                        msg.offset,
                    );
                    state = WorkerState::Processing(msg);
                }
                _ = shutdown_token.cancelled() => {
                    tracing::info!(worker_id = worker_id, "worker stopped (cancelled, idle)");
                    break;
                }
            }
        }

        // Process the current message if we have one
        if let WorkerState::Processing(msg) = &state {
            let msg = msg.clone();
            let ctx =
                ExecutionContext::new(msg.topic.clone(), msg.partition, msg.offset, worker_id);
            let start = std::time::Instant::now();
            let invocation_labels = MetricLabels::new()
                .insert("handler_id", ctx.topic.as_str())
                .insert("topic", ctx.topic.as_str())
                .insert("mode", handler.mode().as_str());
            let span = tracing::Span::current().kafpy_handler_invoke(
                ctx.topic.as_str(),
                ctx.topic.as_str(),
                ctx.partition,
                ctx.offset,
                handler.mode().as_str(),
            );
            tracing::info!(
                handler_id = %ctx.topic,
                topic = %ctx.topic,
                partition = ctx.partition,
                offset = ctx.offset,
                mode = handler.mode().as_str(),
                "handler invoke start"
            );
            let result = span.in_scope(|| async {
                handler.invoke_mode(&ctx, msg.clone()).await
            }).await;
            let elapsed = start.elapsed();
            tracing::info!(
                handler_id = %ctx.topic,
                topic = %ctx.topic,
                partition = ctx.partition,
                offset = ctx.offset,
                elapsed_ms = elapsed.as_millis() as u64,
                "handler invoke complete"
            );
            HANDLER_METRICS.record_invocation(&NoopSink, &invocation_labels);
            HANDLER_METRICS.record_latency(&NoopSink, &invocation_labels, elapsed);
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
                ExecutionResult::Error { ref reason, ref exception, .. } => {
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
                tracing::info!(
                    worker_id = worker_id,
                    "worker stopped (cancelled after message)"
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
    use crate::coordinator::RetryCoordinator;
    use crate::dispatcher::queue_manager::QueueManager;
    use crate::dispatcher::OwnedMessage;
    use crate::dlq::DefaultDlqRouter;
    use crate::python::DefaultExecutor;
    use crate::observability::runtime_snapshot::WorkerPoolState;
    use pyo3::prelude::*;
    use std::sync::Arc;

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
        use crate::python::handler::HandlerMode;
        Python::attach(|py| {
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
                Arc::new(WorkerPoolState::new(1)),
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
            Arc::new(WorkerPoolState::new(1)),
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
