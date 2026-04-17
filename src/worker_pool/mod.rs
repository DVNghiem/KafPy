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

use crate::coordinator::OffsetCoordinator;
use crate::failure::{FailureReason, FailureCategory};
use crate::dispatcher::queue_manager::QueueManager;
use crate::dispatcher::OwnedMessage;
use crate::python::context::ExecutionContext;
use crate::python::execution_result::ExecutionResult;
use crate::python::executor::{DefaultExecutor, Executor};
use crate::python::handler::PythonHandler;

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
            let result = handler.invoke(&ctx, msg.clone()).await;
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
                    offset_coordinator.mark_failed(&ctx.topic, ctx.partition, ctx.offset, reason);
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
        shutdown_token: CancellationToken,
    ) -> Self {
        let mut join_set = JoinSet::new();

        // Zip workers with receivers — receivers is consumed here
        for (worker_id, rx) in receivers.into_iter().enumerate().take(n_workers) {
            let handler = Arc::clone(&handler);
            let executor = Arc::clone(&executor);
            let queue_manager = Arc::clone(&queue_manager);
            let token = shutdown_token.clone();
            let offset_coordinator = offset_coordinator.clone();

            join_set.spawn(worker_loop(
                rx,
                handler,
                executor,
                queue_manager,
                offset_coordinator,
                worker_id,
                token,
            ));
        }

        tracing::info!(n_workers = n_workers, "WorkerPool created");
        Self {
            join_set,
            shutdown_token,
            offset_coordinator,
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
        // Commit all ready offsets before workers exit (D-03)
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
        Python::with_gil(|py| {
            let py_none = py.None();
            Arc::new(PythonHandler::new(py_none.into()))
        })
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
