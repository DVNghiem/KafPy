//! WorkerPool — manages N Tokio workers polling handler queues.

use std::collections::HashMap;
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
use crate::python::logger;
use crate::worker_pool::concurrency::HandlerConcurrency;
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
    /// Per-handler concurrency control via Arc<Semaphore>.
    handler_concurrency: HandlerConcurrency,
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
        handlers: HashMap<String, Arc<PythonHandler>>,
        executor: Arc<dyn Executor>,
        queue_manager: Arc<QueueManager>,
        offset_coordinator: Arc<dyn OffsetCoordinator>,
        retry_coordinator: Arc<RetryCoordinator>,
        dlq_producer: Arc<SharedDlqProducer>,
        dlq_router: Arc<dyn DlqRouter>,
        shutdown_token: CancellationToken,
        coordinator: Arc<ShutdownCoordinator>,
        handler_concurrency: HandlerConcurrency,
    ) -> Self {
        let mut join_set = JoinSet::new();

        // Create shared worker pool state for runtime introspection (OBS-31)
        let worker_pool_state = Arc::new(WorkerPoolState::new(n_workers));

        // Determine whether ALL handlers are batch mode
        let all_batch = handlers.values().all(|h| {
            matches!(
                h.mode(),
                crate::python::handler::HandlerMode::BatchSync
                    | crate::python::handler::HandlerMode::BatchAsync
            )
        });

        // Share the handler map across all workers via Arc
        let handlers_arc = Arc::new(handlers);

        // Zip workers with receivers — receivers is consumed here
        for (worker_id, rx) in receivers.into_iter().enumerate().take(n_workers) {
            let handlers = Arc::clone(&handlers_arc);
            let executor = Arc::clone(&executor);
            let queue_manager = Arc::clone(&queue_manager);
            let token = shutdown_token.clone();
            let offset_coordinator = offset_coordinator.clone();
            let retry_coordinator = retry_coordinator.clone();
            let dlq_producer = dlq_producer.clone();
            let dlq_router = dlq_router.clone();
            let worker_pool_state = Arc::clone(&worker_pool_state);

            if all_batch {
                // Need to pass handler map to batch_worker_loop for topic lookup
                // For now, use the first handler (common case: single topic in batch mode)
                let first_handler = handlers_arc
                    .values()
                    .next()
                    .cloned()
                    .expect("at least one handler must be registered");
                join_set.spawn(batch_worker_loop(
                    rx,
                    first_handler,
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
                    handlers,
                    executor,
                    queue_manager,
                    offset_coordinator,
                    retry_coordinator,
                    dlq_producer,
                    dlq_router,
                    worker_id,
                    token,
                    worker_pool_state,
                    handler_concurrency.clone(),
                ));
            }
        }

        logger::log("INFO", &format!("WorkerPool created n_workers={}", n_workers));
        Self {
            join_set,
            shutdown_token,
            offset_coordinator,
            dlq_producer,
            dlq_router,
            worker_pool_state,
            coordinator,
            handler_concurrency,
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

    /// Run the worker pool — waits for shutdown signal via CancellationToken.
    ///
    /// Unlike `shutdown()`, this does NOT trigger cancellation. It awaits the
    /// join set until the token is cancelled externally (e.g., by Consumer::stop).
    pub async fn run(&mut self) {
        // Wait for shutdown signal - workers exit when they see cancellation
        // (see worker_loop: "_ = shutdown_token.cancelled() => break")
        while !self.shutdown_token.is_cancelled() {
            // Check if any workers have terminated unexpectedly
            if let Some(result) = self.join_set.join_next().await {
                match result {
                    Ok(()) => {} // Worker exited normally
                    Err(e) => {
                        tracing::error!(error = ?e, "worker panicked");
                    }
                }
            }
            // Continue waiting for shutdown or other workers
        }
        // Token was cancelled - trigger full graceful shutdown including final offset commit
        self.shutdown().await;
    }

    /// Trigger graceful shutdown and await completion (EXEC-12).
    ///
    /// Uses the drain timeout from ShutdownCoordinator. If drain exceeds the
    /// timeout, forces abort of all remaining workers.
    pub async fn shutdown(&mut self) {
        logger::log("INFO", "initiating worker pool shutdown");
        self.shutdown_token.cancel();
        // LSC-03: Drain with timeout from coordinator
        let drain_timeout = self.coordinator.drain_timeout();
        match tokio::time::timeout(drain_timeout, self.join_set.shutdown()).await {
            Ok(()) => {
                logger::log("INFO", "worker pool drained gracefully");
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
        logger::log("INFO", "worker pool shutdown complete");
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

    fn dummy_handlers() -> HashMap<String, Arc<PythonHandler>> {
        use crate::python::handler::HandlerMode;
        let handler = Python::attach(|py| {
            let py_none = py.None();
            Arc::new(PythonHandler::new(
                py_none.into(),
                None,
                HandlerMode::SingleSync,
                None,
                None,
            ))
        });
        let mut map = HashMap::new();
        map.insert("test".to_string(), handler);
        map
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
            dummy_handlers(),
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
            crate::worker_pool::HandlerConcurrency::new(4),
        );
        let _ = tx;
    }
}
