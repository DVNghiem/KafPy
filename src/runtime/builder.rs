//! Runtime builder — assembles the full consumer runtime in the correct order.
//!
//! ## Assembly Order (must be preserved)
//!
//! 1. `ConsumerConfigBuilder` → `rust_config`
//! 2. `rust_config` + `default_retry_policy` → `ConsumerRunner`
//! 3. `runner_arc` → `OffsetTracker::new` + `set_runner`
//! 4. `runner_arc` → `ConsumerDispatcher::new`
//! 5. Collect receivers from handlers
//! 6. `handlers` → `PythonHandler::new`
//! 7. `executor`, `queue_manager`, `retry_coordinator`
//! 8. `dlq_producer`, `dlq_router`
//! 9. `ShutdownCoordinator`
//! 10. `WorkerPool::new` with all dependencies
//! 11. `RuntimeSnapshotTask::spawn`
//! 12. `OffsetCommitter::new` + spawn committer task
//! 13. Spawn dispatcher task → `dispatcher_handle`
//! 14. Return `Runtime`; caller invokes `runtime.run()`

use crate::config::ConsumerConfig;
use crate::consumer::error::ConsumerError;
use crate::consumer::{ConsumerConfigBuilder, ConsumerRunner};
use crate::coordinator::{OffsetCommitter, RetryCoordinator, ShutdownCoordinator, CommitConfig, OffsetTracker};
use crate::dispatcher::consumer_dispatcher::ConsumerDispatcher;
use crate::dispatcher::DefaultBackpressurePolicy;
use crate::dlq::produce::SharedDlqProducer;
use crate::dlq::router::DefaultDlqRouter;
use crate::dlq::DlqRouter;
use crate::observability::runtime_snapshot::RuntimeSnapshotTask;
use crate::python::handler::PythonHandler;
use crate::python::{DefaultExecutor, Executor};
use crate::worker_pool::pool::WorkerPool;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

/// Builder for assembling the full consumer runtime.
///
/// Created in `Consumer::start()` and consumed by `build()`.
pub struct RuntimeBuilder {
    config: ConsumerConfig,
    handlers: Arc<Mutex<HashMap<String, Arc<pyo3::Py<pyo3::PyAny>>>>>,
    shutdown_token: CancellationToken,
}

impl RuntimeBuilder {
    /// Creates a new runtime builder with the given consumer config and handlers.
    pub fn new(
        config: ConsumerConfig,
        handlers: Arc<Mutex<HashMap<String, Arc<pyo3::Py<pyo3::PyAny>>>>>,
        shutdown_token: CancellationToken,
    ) -> Self {
        Self {
            config,
            handlers,
            shutdown_token,
        }
    }

    /// Assembles the full runtime and returns a `Runtime` handle.
    ///
    /// Assembly order is fixed — see module-level doc for invariants.
    pub async fn build(self) -> Result<Runtime, ConsumerError> {
        // 1. Build pure-Rust config from the Python-facing config
        let rust_config = ConsumerConfigBuilder::new()
            .brokers(&self.config.brokers)
            .group_id(&self.config.group_id)
            .topics(self.config.topics.iter().map(|s| s.as_str()))
            .auto_offset_reset(match self.config.auto_offset_reset.as_str() {
                "earliest" => crate::consumer::AutoOffsetReset::Earliest,
                _ => crate::consumer::AutoOffsetReset::Latest,
            })
            .enable_auto_commit(self.config.enable_auto_commit)
            .session_timeout(std::time::Duration::from_millis(
                self.config.session_timeout_ms as u64,
            ))
            .heartbeat_interval(std::time::Duration::from_millis(
                self.config.heartbeat_interval_ms as u64,
            ))
            .max_poll_interval(std::time::Duration::from_millis(
                self.config.max_poll_interval_ms as u64,
            ))
            .build()?;

        let default_retry_policy = rust_config.default_retry_policy.clone();

        // 2. Create ConsumerRunner
        let runner = ConsumerRunner::new(rust_config.clone(), None)?;

        // 3. Create OffsetTracker and wire ConsumerRunner
        let runner_arc: Arc<ConsumerRunner> = Arc::new(runner);
        let offset_tracker: Arc<OffsetTracker> = Arc::new(OffsetTracker::new());
        offset_tracker.set_runner(Arc::clone(&runner_arc));

        // 4. Create ConsumerDispatcher
        let dispatcher = ConsumerDispatcher::new((*runner_arc).clone());

        // 5. Collect receivers from all registered handlers
        let receivers: Vec<_> = {
            let handlers_guard = self.handlers.lock().unwrap();
            handlers_guard
                .keys()
                .map(|topic| dispatcher.register_handler(topic.clone(), 100, None))
                .collect()
        };

        // 6. Build PythonHandler from the registered callbacks
        let handler_arc: Arc<PythonHandler> = {
            let handlers_guard = self.handlers.lock().unwrap();
            let first_handler = handlers_guard
                .values()
                .next()
                .expect("at least one handler must be registered");
            Arc::new(PythonHandler::new(
                first_handler.clone(),
                Some(default_retry_policy),
                crate::python::handler::HandlerMode::SingleSync,
                None,
            ))
        };

        // 7. Create executor, queue_manager, retry_coordinator
        let executor_arc: Arc<dyn Executor> = Arc::new(DefaultExecutor::default());
        let queue_manager_arc = dispatcher.queue_manager();
        let retry_coordinator: Arc<RetryCoordinator> =
            Arc::new(RetryCoordinator::new(&rust_config));

        // 8. Create DLQ producer and router
        let dlq_producer: Arc<SharedDlqProducer> =
            Arc::new(SharedDlqProducer::new(&rust_config).expect("Failed to create DLQ producer"));
        let dlq_router: Arc<dyn DlqRouter> =
            Arc::new(DefaultDlqRouter::new(rust_config.dlq_topic_prefix.clone()));

        let n_workers = 4; // EXEC-08: configurable

        // 9. Create shutdown coordinator with configured drain timeout
        let coordinator: Arc<ShutdownCoordinator> =
            Arc::new(ShutdownCoordinator::new(rust_config.drain_timeout_secs));

        // 10. Create WorkerPool
        let pool = WorkerPool::new(
            n_workers,
            receivers,
            handler_arc,
            executor_arc,
            queue_manager_arc.clone(),
            offset_tracker.clone(),
            retry_coordinator,
            dlq_producer,
            dlq_router,
            self.shutdown_token.clone(),
            Arc::clone(&coordinator),
        );

        // 11. Spawn RuntimeSnapshotTask for introspection
        let _snapshot_task = RuntimeSnapshotTask::spawn(
            Some(queue_manager_arc.clone()),
            Some(offset_tracker.clone()),
            Some(Arc::clone(&pool.worker_pool_state)),
            std::time::Duration::from_secs(10),
        );

        // 12. Create OffsetCommitter and spawn committer task
        let committer = OffsetCommitter::new(
            Arc::clone(&runner_arc),
            Arc::clone(&offset_tracker),
            CommitConfig::default(),
            Arc::clone(&coordinator),
        );
        let (tx, rx) = watch::channel(crate::coordinator::TopicPartition::new("", 0));
        let committer_handle = tokio::spawn(async move {
            committer.run(rx).await;
        });
        drop(tx);

        // 13. Spawn dispatcher task
        let dispatcher_handle = tokio::spawn(async move {
            dispatcher
                .run(&DefaultBackpressurePolicy)
                .await;
        });

        // 14. Return Runtime (caller must invoke run())
        Ok(Runtime {
            pool,
            dispatcher_handle,
            committer_handle,
            coordinator,
        })
    }
}

/// Handle to the assembled runtime.
///
/// Call `run()` to start the pool and wait for shutdown.
pub struct Runtime {
    /// The worker pool — must be polled to process messages.
    pub pool: WorkerPool,
    /// Handle to the dispatcher task — kept alive while pool runs.
    pub dispatcher_handle: tokio::task::JoinHandle<()>,
    /// Handle to the offset committer task.
    pub committer_handle: tokio::task::JoinHandle<()>,
    /// Shutdown coordinator for drain signaling.
    #[allow(dead_code)]
    pub coordinator: Arc<ShutdownCoordinator>,
}

impl Runtime {
    /// Runs the worker pool and waits for shutdown.
    ///
    /// Blocks until `pool.run()` completes (i.e., all workers have exited after
    /// shutdown signal). Keeps `dispatcher_handle` and `committer_handle` alive
    /// for the duration.
    pub async fn run(self) {
        self.pool.run().await;
        // Await the dispatcher and committer to ensure clean shutdown
        let _ = self.dispatcher_handle.await;
        let _ = self.committer_handle.await;
    }
}
