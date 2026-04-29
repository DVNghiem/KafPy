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
use crate::coordinator::{CommitConfig, OffsetCommitter, OffsetTracker, RetryCoordinator, ShutdownCoordinator};
use crate::dispatcher::consumer_dispatcher::ConsumerDispatcher;
use crate::dispatcher::DefaultBackpressurePolicy;
use crate::dlq::produce::SharedDlqProducer;
use crate::dlq::router::DefaultDlqRouter;
use crate::dlq::DlqRouter;
use crate::observability::runtime_snapshot::RuntimeSnapshotTask;
use crate::pyconsumer::HandlerMetadata;
use crate::python::handler::PythonHandler;
use crate::python::logger;
use crate::python::{DefaultExecutor, Executor};
use crate::worker_pool::concurrency::HandlerConcurrency;
use crate::worker_pool::pool::WorkerPool;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

/// Builder for assembling the full consumer runtime.
///
/// Created in `Consumer::start()` and consumed by `build()`.
pub struct RuntimeBuilder {
    config: ConsumerConfig,
    handlers: Arc<Mutex<HashMap<String, HandlerMetadata>>>,
    shutdown_token: CancellationToken,
}

impl RuntimeBuilder {
    /// Creates a new runtime builder with the given consumer config and handlers.
    pub fn new(
        config: ConsumerConfig,
        handlers: Arc<Mutex<HashMap<String, HandlerMetadata>>>,
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
        let mut config_builder = ConsumerConfigBuilder::new()
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
            ));

        // Wire retry policy from PyO3 config if provided, otherwise use default
        if let Some(ref py_retry) = self.config.default_retry_policy {
            config_builder = config_builder.default_retry_policy(py_retry.to_rust());
        }

        // Wire DLQ topic prefix from PyO3 config if provided
        if let Some(ref prefix) = self.config.dlq_topic_prefix {
            config_builder = config_builder.dlq_topic_prefix(prefix);
        }

        // Wire drain timeout from PyO3 config if provided
        if let Some(secs) = self.config.drain_timeout_secs {
            config_builder = config_builder.drain_timeout(secs);
        }

        let rust_config = config_builder.build()?;

        let default_retry_policy = rust_config.default_retry_policy.clone();

        // 2. Create DLQ producer and router (needed for ConsumerRunner with CustomConsumerContext)
        let dlq_producer: Arc<SharedDlqProducer> =
            Arc::new(SharedDlqProducer::new(&rust_config).expect("Failed to create DLQ producer"));
        let dlq_router: Arc<dyn DlqRouter> =
            Arc::new(DefaultDlqRouter::new(rust_config.dlq_topic_prefix.clone()));

        // 3. Create OffsetTracker first (needed for CustomConsumerContext)
        let offset_tracker: Arc<OffsetTracker> = Arc::new(OffsetTracker::new());

        // 4. Create ConsumerRunner with CustomConsumerContext
        let runner = ConsumerRunner::new(
            rust_config.clone(),
            None,
            Arc::clone(&offset_tracker),
            Arc::clone(&dlq_router),
            Arc::clone(&dlq_producer),
        )?;

        // 5. Wire ConsumerRunner into OffsetTracker
        let runner_arc: Arc<ConsumerRunner> = Arc::new(runner);
        offset_tracker.set_runner(Arc::clone(&runner_arc));

        // 4. Create ConsumerDispatcher
        let dispatcher = ConsumerDispatcher::new((*runner_arc).clone());

        // 5. Collect receivers from all registered handlers
        let all_handlers: Vec<(String, HandlerMetadata)> = {
            let handlers_guard = self.handlers.lock().unwrap();
            handlers_guard
                .iter()
                .map(|(topic, meta)| (topic.clone(), meta.clone()))
                .collect()
        };
        logger::log("INFO", &format!(
            "registering handlers count={} topics={:?}",
            all_handlers.len(),
            all_handlers.iter().map(|(t, _)| t).collect::<Vec<_>>()
        ));
        let receivers: Vec<_> = all_handlers
            .iter()
            .map(|(topic, _)| dispatcher.register_handler(topic.clone(), 100, None))
            .collect();

        // 6. Build per-topic PythonHandler map from registered HandlerMetadata
        // Each topic gets its own handler with individual config (timeout, mode, batch).
        let default_handler_timeout = self.config.handler_timeout_ms.map(Duration::from_millis);
        let handler_map: HashMap<String, Arc<PythonHandler>> = {
            let handlers_guard = self.handlers.lock().unwrap();
            handlers_guard
                .iter()
                .map(|(topic, meta)| {
                    // Resolve timeout: per-handler > global config > None
                    let timeout = meta
                        .timeout_ms
                        .map(Duration::from_millis)
                        .or(default_handler_timeout);

                    // Resolve batch config
                    let batch_policy = meta.batch_max_size.map(|max_size| {
                        crate::python::handler::BatchPolicy {
                            max_batch_size: max_size,
                            max_batch_wait_ms: meta.batch_max_wait_ms.unwrap_or(1000),
                        }
                    });

                    let handler = Arc::new(PythonHandler::new(
                        meta.callback.clone(),
                        Some(default_retry_policy.clone()),
                        meta.mode.clone(),
                        batch_policy,
                        timeout,
                        topic.clone(),
                    ));
                    (topic.clone(), handler)
                })
                .collect()
        };

        // 7. Create executor, queue_manager, retry_coordinator
        let executor_arc: Arc<dyn Executor> = Arc::new(DefaultExecutor);
        let queue_manager_arc = dispatcher.queue_manager();
        let retry_coordinator: Arc<RetryCoordinator> =
            Arc::new(RetryCoordinator::new(&rust_config));

        // Read num_workers from PyO3 config if provided, otherwise default to 4
        let n_workers = self.config.num_workers.unwrap_or(4) as usize;

        // 9. Create shutdown coordinator with configured drain timeout
        let coordinator: Arc<ShutdownCoordinator> =
            Arc::new(ShutdownCoordinator::new(rust_config.drain_timeout_secs));

        // Create HandlerConcurrency with default limit of 4 per handler
        let handler_concurrency = HandlerConcurrency::new(4);

        // 10. Create WorkerPool with per-topic handler map
        let pool = WorkerPool::new(
            n_workers,
            receivers,
            handler_map,
            executor_arc,
            queue_manager_arc.clone(),
            offset_tracker.clone(),
            retry_coordinator,
            dlq_producer,
            dlq_router,
            self.shutdown_token.clone(),
            Arc::clone(&coordinator),
            handler_concurrency,
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
    pub async fn run(mut self) {
        self.pool.run().await;
        // Await the dispatcher and committer to ensure clean shutdown
        let _ = self.dispatcher_handle.await;
        let _ = self.committer_handle.await;
    }

    /// Runs the worker pool with SIGTERM handling.
    ///
    /// Spawns a task that listens for SIGTERM. When received, initiates
    /// graceful shutdown via ShutdownCoordinator.begin_draining().
    /// Then runs the pool and waits for shutdown.
    pub async fn run_with_sigterm(self) {
        use tokio::signal::unix::{signal, SignalKind};

        let coordinator = Arc::clone(&self.coordinator);

        tokio::spawn(async move {
            let mut sigterm = signal(SignalKind::terminate()).unwrap();
            sigterm.recv().await;
            tracing::info!(
                drain_timeout_secs = coordinator.drain_timeout().as_secs(),
                "received SIGTERM, initiating graceful shutdown"
            );
            let _ = coordinator.begin_draining();
        });

        self.run().await;
    }
}
