//! WorkerPool — manages N Tokio workers polling handler queues.

use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::coordinator::RetryCoordinator;
use crate::coordinator::ShutdownCoordinator;
use crate::coordinator::OffsetCoordinator;
use crate::dispatcher::queue_manager::QueueManager;
use crate::dispatcher::OwnedMessage;
use crate::dlq::{DlqRouter, SharedDlqProducer};
use crate::observability::runtime_snapshot::WorkerPoolState;
use crate::python::executor::Executor;
use crate::python::handler::PythonHandler;
use crate::worker_pool::batch_loop::batch_worker_loop;
use crate::worker_pool::worker::worker_loop;

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
    dlq_producer: Arc<SharedDlqProducer>,
    dlq_router: Arc<dyn DlqRouter>,
    /// Shared state for runtime introspection (OBS-31).
    pub(crate) worker_pool_state: Arc<WorkerPoolState>,
    /// Shutdown coordinator for accessing drain timeout.
    coordinator: Arc<ShutdownCoordinator>,
}

impl WorkerPool {
    /// Create a new WorkerPool with `n_workers` tasks.
    ///
    /// Each worker gets its own receiver from `receivers`. The `handler` is
    /// shared across all workers via `Arc`. Uses `DefaultExecutor` (EXEC-04).
    /// The `shutdown_token` is supplied by the owner (Consumer) so that
    /// `stop()` can cancel all workers by cancelling the shared token.
    pub(crate) fn new(
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
        coordinator: Arc<ShutdownCoordinator>,
    ) -> Self {
        let mut join_set = JoinSet::new();

        // Create shared worker pool state for runtime introspection (OBS-31)
        let worker_pool_state = Arc::new(WorkerPoolState::new(n_workers));

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
            let worker_pool_state = Arc::clone(&worker_pool_state);

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
                    worker_pool_state,
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
                    worker_pool_state,
                ));
            }
        }

        tracing::info!(n_workers = n_workers, "WorkerPool created");
        Self {
            join_set,
            shutdown_token,
            offset_coordinator,
            dlq_producer,
            dlq_router,
            worker_pool_state,
            coordinator,
        }
    }

    /// Returns current worker states for RuntimeSnapshot (OBS-31).
    pub fn worker_states(
        &self,
    ) -> std::collections::HashMap<
        usize,
        crate::observability::runtime_snapshot::WorkerStatus,
    > {
        self.worker_pool_state.get_states()
    }

    /// Run the worker pool — awaits all workers until shutdown.
    pub async fn run(mut self) {
        self.join_set.shutdown().await;
    }

    /// Trigger graceful shutdown and await completion (EXEC-12).
    ///
    /// Uses the drain timeout from ShutdownCoordinator. If drain exceeds the
    /// timeout, forces abort of all remaining workers.
    pub async fn shutdown(&mut self) {
        tracing::info!("initiating worker pool shutdown");
        self.shutdown_token.cancel();
        // LSC-03: Drain with timeout from coordinator
        let drain_timeout = self.coordinator.drain_timeout();
        match tokio::time::timeout(drain_timeout, self.join_set.shutdown()).await {
            Ok(()) => {
                tracing::info!("worker pool drained gracefully");
            }
            Err(_) => {
                tracing::warn!(
                    timeout_secs = drain_timeout.as_secs(),
                    "drain timeout exceeded, forcing abort"
                );
                self.join_set.abort_all();
            }
        }
        // LSC-03: Flush pending retries to DLQ before finalizing
        self.offset_coordinator
            .flush_failed_to_dlq(&self.dlq_router, &self.dlq_producer);
        self.offset_coordinator.graceful_shutdown();
        tracing::info!("worker pool shutdown complete");
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinator::OffsetCoordinator;
    use crate::dispatcher::queue_manager::QueueManager;
    use crate::dlq::router::DefaultDlqRouter;
    use crate::python::DefaultExecutor;
    use pyo3::prelude::*;
    use std::sync::Arc;

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
            std::sync::Arc::new(crate::coordinator::ShutdownCoordinator::new(30)),
        );
        let _ = tx;
    }
}
