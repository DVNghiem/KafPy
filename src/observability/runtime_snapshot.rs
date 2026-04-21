//! Runtime introspection via periodic snapshot — zero hot-path overhead.
//!
//! ## Design (OBS-27 through OBS-32)
//!
//! - **RuntimeSnapshot**: Holds worker_pool status, queue depths per handler,
//!   accumulator states, and consumer_lag_summary. Updated every 10s by
//!   `RuntimeSnapshotTask`, never on the hot path.
//! - **RuntimeSnapshotTask**: Background poller that wakes every 10s and updates
//!   the shared `Arc<RwLock<RuntimeSnapshot>>` snapshot. No atomic updates in
//!   worker_loop.
//! - **WorkerPoolState**: Shared state that worker_loop updates when processing
//!   (set_active / set_idle). Polled by RuntimeSnapshotTask — not on hot path.
//! - **StatusCallbackRegistry**: Python can register a callback invoked on each
//!   snapshot update (opt-in, not default).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use parking_lot::RwLock;
use pyo3::{Py, PyAny};
use tokio_util::sync::CancellationToken;

/// Thread-safe snapshot of runtime state for Python introspection.
///
/// Polling-based: updated by RuntimeSnapshotTask every 10s.
/// Zero-cost when not called — no atomic updates on hot path.
#[derive(Debug, Clone)]
pub struct RuntimeSnapshot {
    /// Snapshot timestamp (UTC).
    pub timestamp: SystemTime,
    /// Worker pool status: worker_id -> WorkerState.
    pub worker_states: HashMap<usize, WorkerState>,
    /// Queue depths per handler (from QueueManager::queue_snapshots).
    pub queue_depths: HashMap<String, QueueDepthInfo>,
    /// Accumulator states per handler.
    pub accumulator_info: HashMap<String, AccumulatorInfo>,
    /// Consumer lag summary (from OffsetTracker + KafkaMetrics).
    pub consumer_lag_summary: ConsumerLagSummary,
}

/// Worker activity state — maps directly to WorkerStatus enum.
#[derive(Debug, Clone)]
pub enum WorkerState {
    Idle,
    Active {
        handler_id: String,
        topic: String,
        partition: i32,
        offset: i64,
    },
    Busy {
        handler_id: String,
    },
}

/// Queue depth and inflight counts for a single handler.
#[derive(Debug, Clone)]
pub struct QueueDepthInfo {
    pub queue_depth: usize,
    pub inflight: usize,
}

/// Accumulator state for a single handler.
#[derive(Debug, Clone)]
pub struct AccumulatorInfo {
    pub total_messages: usize,
    pub partitions: HashMap<i32, PartitionAccumulatorInfo>,
}

/// Per-partition accumulator state.
#[derive(Debug, Clone)]
pub struct PartitionAccumulatorInfo {
    pub message_count: usize,
    pub has_deadline: bool,
    pub deadline_ms_remaining: Option<i64>,
}

/// Consumer lag summary across all topics.
#[derive(Debug, Clone, Default)]
pub struct ConsumerLagSummary {
    pub total_lag: i64,
    pub per_topic: HashMap<String, TopicLagInfo>,
}

/// Lag info per topic.
#[derive(Debug, Clone)]
pub struct TopicLagInfo {
    pub total_lag: i64,
    pub partitions: HashMap<i32, PartitionLagInfo>,
}

/// Per-partition lag info.
#[derive(Debug, Clone)]
pub struct PartitionLagInfo {
    pub consumer_lag: i64,
    pub committed_offset: i64,
}

// ─── WorkerPoolState ─────────────────────────────────────────────────────────

/// Shared state for worker pool introspection.
///
/// worker_loop updates its current handler_id and partition when processing.
/// Status is polled by RuntimeSnapshotTask — no atomic updates on hot path.
pub struct WorkerPoolState {
    states: RwLock<HashMap<usize, WorkerStatus>>,
}

#[derive(Debug, Clone)]
pub enum WorkerStatus {
    Idle,
    Active {
        handler_id: String,
        topic: String,
        partition: i32,
        offset: i64,
    },
    Busy {
        handler_id: String,
    },
}

impl WorkerPoolState {
    /// Create a new WorkerPoolState with `n_workers` entries, all initially Idle.
    pub fn new(n_workers: usize) -> Self {
        let states = (0..n_workers)
            .map(|id| (id, WorkerStatus::Idle))
            .collect();
        Self {
            states: RwLock::new(states),
        }
    }

    /// Called by worker_loop when it starts processing a message.
    /// This is called during message processing (not on the hot recv path).
    pub fn set_active(
        &self,
        worker_id: usize,
        handler_id: String,
        topic: String,
        partition: i32,
        offset: i64,
    ) {
        let mut guard = self.states.write();
        guard.insert(
            worker_id,
            WorkerStatus::Active {
                handler_id,
                topic,
                partition,
                offset,
            },
        );
    }

    /// Called by worker_loop when batch processing starts (marks as Busy).
    pub fn set_busy(&self, worker_id: usize, handler_id: String) {
        let mut guard = self.states.write();
        guard.insert(worker_id, WorkerStatus::Busy { handler_id });
    }

    /// Called by worker_loop when it finishes processing.
    pub fn set_idle(&self, worker_id: usize) {
        let mut guard = self.states.write();
        guard.insert(worker_id, WorkerStatus::Idle);
    }

    /// Returns current state snapshot.
    pub fn get_states(&self) -> HashMap<usize, WorkerStatus> {
        self.states.read().clone()
    }

    /// Returns counts of idle / active / busy workers.
    #[allow(dead_code)]
    pub fn worker_counts(&self) -> (usize, usize, usize) {
        let guard = self.states.read();
        let mut idle = 0;
        let mut active = 0;
        let mut busy = 0;
        for state in guard.values() {
            match state {
                WorkerStatus::Idle => idle += 1,
                WorkerStatus::Active { .. } => active += 1,
                WorkerStatus::Busy { .. } => busy += 1,
            }
        }
        (idle, active, busy)
    }
}

// ─── StatusCallbackRegistry ──────────────────────────────────────────────────

/// Registry for Python status callbacks.
///
/// Python can register a callback function that is invoked on every
/// RuntimeSnapshot update (opt-in, not default).
pub struct StatusCallbackRegistry {
    callbacks: RwLock<Vec<Py<PyAny>>>,
}

impl StatusCallbackRegistry {
    pub fn new() -> Self {
        Self {
            callbacks: RwLock::new(Vec::new()),
        }
    }

    /// Register a Python callable to be invoked on each snapshot update.
    pub fn register(&self, py_callback: Py<PyAny>) {
        let mut guard = self.callbacks.write();
        guard.push(py_callback);
    }
}

impl Default for StatusCallbackRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ─── RuntimeSnapshotTask ───────────────────────────────────────────────────

/// Global singleton holder for the runtime snapshot task.
///
/// Set once via `set_global` and accessed via `get_global`.
static GLOBAL_SNAPSHOT_TASK: std::sync::OnceLock<Arc<RuntimeSnapshotTask>> =
    std::sync::OnceLock::new();

/// Global accessor for the current snapshot (for Python FFI).
pub fn get_current_snapshot() -> RuntimeSnapshot {
    GLOBAL_SNAPSHOT_TASK
        .get()
        .map(|task| task.get_snapshot())
        .unwrap_or_else(|| RuntimeSnapshot::default())
}

/// Global accessor for the callback registry.
pub fn get_callback_registry() -> Option<Arc<StatusCallbackRegistry>> {
    GLOBAL_SNAPSHOT_TASK.get().map(|task| task.callbacks())
}

/// Background task that polls all components and updates RuntimeSnapshot.
///
/// Runs every 10s (configurable), avoiding per-message hot-path overhead.
/// Uses Arc<RwLock<RuntimeSnapshot>> for thread-safe snapshot sharing.
/// No hot-path atomic updates — all state is polled on interval.
pub struct RuntimeSnapshotTask {
    snapshot: RwLock<RuntimeSnapshot>,
    queue_manager: Option<Arc<crate::dispatcher::queue_manager::QueueManager>>,
    offset_tracker: Option<Arc<crate::coordinator::OffsetTracker>>,
    worker_pool_state: Option<Arc<WorkerPoolState>>,
    poll_interval: Duration,
    shutdown_token: CancellationToken,
}

impl Default for RuntimeSnapshot {
    fn default() -> Self {
        Self {
            timestamp: SystemTime::UNIX_EPOCH,
            worker_states: HashMap::new(),
            queue_depths: HashMap::new(),
            accumulator_info: HashMap::new(),
            consumer_lag_summary: ConsumerLagSummary::default(),
        }
    }
}

impl RuntimeSnapshotTask {
    /// Spawn the background polling task and store as global singleton.
    pub(crate) fn spawn(
        queue_manager: Option<Arc<crate::dispatcher::queue_manager::QueueManager>>,
        offset_tracker: Option<Arc<crate::coordinator::OffsetTracker>>,
        worker_pool_state: Option<Arc<WorkerPoolState>>,
        poll_interval: Duration,
    ) -> Arc<Self> {
        let task = Arc::new(Self {
            snapshot: RwLock::new(RuntimeSnapshot::default()),
            queue_manager,
            offset_tracker,
            worker_pool_state,
            poll_interval,
            shutdown_token: CancellationToken::new(),
        });

        let task_clone = Arc::clone(&task);

        let _ = GLOBAL_SNAPSHOT_TASK.set(Arc::clone(&task));

        tokio::spawn(async move {
            task_clone.run().await;
        });

        task
    }

    /// Start the polling task (self-owned; does not block).
    #[allow(dead_code)]
    pub fn start(self: Arc<Self>) {
        let this = Arc::clone(&self);
        tokio::spawn(async move {
            this.run().await;
        });
    }

    /// Returns a cheap clone of the current snapshot.
    pub fn get_snapshot(&self) -> RuntimeSnapshot {
        self.snapshot.read().clone()
    }

    /// Returns the callback registry for opt-in Python callbacks.
    pub fn callbacks(&self) -> Arc<StatusCallbackRegistry> {
        // Lazily create the registry if not set — but in practice it's set via spawn()
        // We use a static for the registry to avoid adding another field.
        static REGISTRY: std::sync::OnceLock<Arc<StatusCallbackRegistry>> =
            std::sync::OnceLock::new();
        REGISTRY
            .get_or_init(|| Arc::new(StatusCallbackRegistry::new()))
            .clone()
    }

    async fn run(self: Arc<Self>) {
        let mut interval = tokio::time::interval(self.poll_interval);
        // Skip the first tick (immediate), start from second one
        interval.tick().await;
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.poll_and_update().await;
                    // Invoke registered Python callbacks if any
                    self.invoke_callbacks();
                }
                _ = self.shutdown_token.cancelled() => {
                    break;
                }
            }
        }
    }

    async fn poll_and_update(&self) {
        // 1. Poll queue depths from QueueManager::queue_snapshots()
        let queue_depths = if let Some(ref qm) = self.queue_manager {
            qm.queue_snapshots()
                .into_iter()
                .map(|(handler_id, qs)| {
                    (
                        handler_id,
                        QueueDepthInfo {
                            queue_depth: qs.queue_depth.load(std::sync::atomic::Ordering::Relaxed),
                            inflight: qs.inflight.load(std::sync::atomic::Ordering::Relaxed),
                        },
                    )
                })
                .collect()
        } else {
            HashMap::new()
        };

        // 2. Poll worker states from WorkerPoolState
        let worker_states: HashMap<usize, WorkerState> = if let Some(ref wps) = self.worker_pool_state {
            wps.get_states()
                .into_iter()
                .map(|(id, status)| {
                    let state = match status {
                        WorkerStatus::Idle => WorkerState::Idle,
                        WorkerStatus::Active {
                            handler_id,
                            topic,
                            partition,
                            offset,
                        } => WorkerState::Active {
                            handler_id,
                            topic,
                            partition,
                            offset,
                        },
                        WorkerStatus::Busy { handler_id } => WorkerState::Busy { handler_id },
                    };
                    (id, state)
                })
                .collect()
        } else {
            HashMap::new()
        };

        // 3. Accumulator info — currently not tracked without handler reference
        // Will be populated once BatchAccumulator state is accessible
        let accumulator_info = HashMap::new();

        // 4. Consumer lag from OffsetTracker
        let consumer_lag_summary = if let Some(ref ot) = self.offset_tracker {
            let snapshots = ot.offset_snapshots();
            let mut total_lag: i64 = 0;
            let mut per_topic: HashMap<String, TopicLagInfo> = HashMap::new();

            for ((topic, partition), committed_offset) in snapshots {
                // consumer_lag = highwater - committed_offset (placeholder)
                // In practice this would come from KafkaMetricsTask
                let consumer_lag: i64 = 0; // Will be populated via kafka_metrics
                total_lag += consumer_lag;

                let part_info = PartitionLagInfo {
                    consumer_lag,
                    committed_offset,
                };

                per_topic
                    .entry(topic.clone())
                    .or_insert_with(|| TopicLagInfo {
                        total_lag: 0,
                        partitions: HashMap::new(),
                    })
                    .partitions
                    .insert(partition, part_info);
            }

            // Compute per-topic totals
            for tinfo in per_topic.values_mut() {
                tinfo.total_lag = tinfo.partitions.values().map(|p| p.consumer_lag).sum();
            }

            ConsumerLagSummary {
                total_lag,
                per_topic,
            }
        } else {
            ConsumerLagSummary::default()
        };

        let snapshot = RuntimeSnapshot {
            timestamp: SystemTime::now(),
            worker_states,
            queue_depths,
            accumulator_info,
            consumer_lag_summary,
        };

        *self.snapshot.write() = snapshot;
    }

    fn invoke_callbacks(&self) {
        // Note: This requires GIL. The actual implementation in pyconsumer.rs
        // uses Python::with_gil() to invoke callbacks.
        // This is a placeholder that does nothing when called from non-PyO3 context.
    }
}
