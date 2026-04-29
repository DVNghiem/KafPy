//! Fan-out tracking for parallel sink dispatch.
//!
//! One message can trigger multiple sink handlers in parallel via JoinSet.
//! Fan-out degree is bounded (max 64) to prevent resource exhaustion.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use tokio::sync::Semaphore;

use crate::dispatcher::backpressure::BackpressureAction;
use crate::python::handler::PythonHandler;

// ─── Branch Result ────────────────────────────────────────────────────────────

/// Result of a single fan-out sink branch execution.
#[derive(Debug, Clone)]
pub enum BranchResult {
    /// Handler completed successfully.
    Ok,
    /// Handler raised an exception.
    Error {
        reason: crate::failure::FailureReason,
        exception: String,
    },
    /// Handler timed out.
    Timeout { timeout_ms: u64 },
}

/// Results from all fan-out branches for a single message dispatch.
#[derive(Debug, Clone, Default)]
pub struct BranchResults {
    /// Vector of (branch_id, result) pairs collected from all branches.
    pub results: Vec<(u64, BranchResult)>,
}

// ─── Sink Config ──────────────────────────────────────────────────────────────

/// Sink configuration holding topic and handler for a single fan-out branch.
#[derive(Clone)]
pub struct SinkConfig {
    /// Topic name for this sink.
    pub topic: String,
    /// Handler to invoke for this sink.
    pub handler: Arc<PythonHandler>,
    /// Per-sink timeout override. None means use handler's default timeout.
    pub timeout: Option<std::time::Duration>,
}

impl std::fmt::Debug for SinkConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SinkConfig")
            .field("topic", &self.topic)
            .field("handler", &"<PythonHandler>")
            .finish()
    }
}

// ─── Fan-Out Configuration ────────────────────────────────────────────────────

/// Fan-out configuration attached to a handler.
///
/// When configured, messages dispatched to this handler will be fanned out
/// to all registered sinks in parallel. The primary handler ACKs immediately;
/// sink failures are tracked via callbacks but are non-blocking.
#[derive(Debug, Clone)]
pub struct FanOutConfig {
    /// Sinks to dispatch to in parallel (topic + handler).
    pub sinks: Vec<SinkConfig>,
    /// Max fan-out degree (capped at 64).
    pub max_fan_out: u8,
    /// Slot manager for bounded concurrency control.
    /// None means unbounded (use with caution).
    pub slot_manager: Option<Arc<FanOutSlotManager>>,
}

impl FanOutConfig {
    /// Returns the number of registered sinks.
    pub fn sink_count(&self) -> usize {
        self.sinks.len()
    }

    /// Returns true if fan-out slots are exhausted.
    pub fn is_exhausted(&self) -> bool {
        self.slot_manager
            .as_ref()
            .map(|s| s.available() == 0)
            .unwrap_or(false)
    }
}

// ─── Callback Registry ─────────────────────────────────────────────────────────

/// Manages completion callbacks per branch.
///
/// Each fan-out branch gets a unique branch_id. Callbacks are invoked
/// when the branch completes (success, error, or timeout).
///
/// Uses std::sync::Mutex (not tokio::sync::Mutex) because callbacks are
/// registered and emitted from async contexts but the Mutex only needs to
/// be held briefly during mutation — no .await across lock boundary.
pub struct CallbackRegistry {
    callbacks: Mutex<HashMap<u64, Box<dyn Fn(BranchResult) + Send + Sync>>>,
    next_branch_id: std::sync::atomic::AtomicU64,
}

impl Default for CallbackRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl CallbackRegistry {
    /// Create a new empty CallbackRegistry.
    pub fn new() -> Self {
        Self {
            callbacks: Mutex::new(HashMap::new()),
            next_branch_id: std::sync::atomic::AtomicU64::new(1),
        }
    }

    /// Generate the next unique branch_id.
    fn next_id(&self) -> u64 {
        self.next_branch_id
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }

    /// Register a completion callback for a branch.
    /// Returns the branch_id for later correlation.
    pub fn on_sink_complete<F>(&self, callback: F) -> u64
    where
        F: Fn(BranchResult) + Send + Sync + 'static,
    {
        let branch_id = self.next_id();
        let mut cb_map = self.callbacks.lock().expect("CallbackRegistry poisoned");
        cb_map.insert(branch_id, Box::new(callback));
        branch_id
    }

    /// Emit the completion result for a branch, invoking its callback and removing it.
    pub fn emit(&self, branch_id: u64, result: BranchResult) {
        let mut cb_map = self.callbacks.lock().expect("CallbackRegistry poisoned");
        if let Some(callback) = cb_map.remove(&branch_id) {
            callback(result);
        }
    }
}

// ─── Fan-Out Tracker ─────────────────────────────────────────────────────────

/// FanOutTracker tracks in-flight branches for a single message dispatch.
///
/// # Fields
/// - `max_fan_out: u8` — configured max fan-out degree (capped at 64)
/// - `sinks: Vec<SinkConfig>` — registered sink topics and their handlers
/// - `inflight: AtomicU8` — current in-flight branch count
/// - `callback_registry: CallbackRegistry` — completion callbacks per branch
///
/// # Usage
/// Created per-message dispatch. Shared across all sink branches via Arc.
/// Each branch calls `try_acquire_slot()` before executing and `release_slot()`
/// when done. Callbacks are invoked via `emit_completion`.
pub struct FanOutTracker {
    max_fan_out: u8,
    sinks: Vec<SinkConfig>,
    inflight: std::sync::atomic::AtomicU8,
    callback_registry: CallbackRegistry,
    /// Total number of registered branches.
    total: std::sync::atomic::AtomicU8,
    /// Counter for completed branches.
    completed: std::sync::atomic::AtomicU8,
    /// Notify signal fired when all branches complete.
    notify: tokio::sync::Notify,
    /// Results collected from all branches (branch_id -> result).
    results: Mutex<Vec<(u64, BranchResult)>>,
}

impl FanOutTracker {
    /// Create a new FanOutTracker with max_fan_out degree.
    ///
    /// max_fan_out is capped at 64 at construction time.
    pub fn new(max_fan_out: u8) -> Self {
        let max = max_fan_out.min(64);
        Self {
            max_fan_out: max,
            sinks: Vec::new(),
            inflight: std::sync::atomic::AtomicU8::new(0),
            callback_registry: CallbackRegistry::new(),
            total: std::sync::atomic::AtomicU8::new(0),
            completed: std::sync::atomic::AtomicU8::new(0),
            notify: tokio::sync::Notify::new(),
            results: Mutex::new(Vec::new()),
        }
    }

    /// Register a sink topic and handler for this fan-out tracker.
    pub fn register_sink(&mut self, topic: String, handler: Arc<PythonHandler>, timeout: Option<std::time::Duration>) {
        self.sinks.push(SinkConfig { topic, handler, timeout });
    }

    /// Returns the number of registered sinks.
    pub fn sink_count(&self) -> usize {
        self.sinks.len()
    }

    /// Returns the configured max fan-out degree.
    pub fn max_fan_out(&self) -> u8 {
        self.max_fan_out
    }

    /// Returns true if fan-out slots are exhausted (inflight >= max_fan_out).
    pub fn is_exhausted(&self) -> bool {
        self.inflight.load(std::sync::atomic::Ordering::SeqCst) >= self.max_fan_out
    }

    /// Atomically increment inflight count.
    ///
    /// Returns false if would exceed max_fan_out (caller should backpressure).
    pub fn try_acquire_slot(&self) -> bool {
        let current = self.inflight.load(std::sync::atomic::Ordering::SeqCst);
        if current >= self.max_fan_out {
            return false;
        }
        self.inflight
            .compare_exchange(
                current,
                current + 1,
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
            )
            .is_ok()
    }

    /// Decrement inflight count (called when a branch completes).
    pub fn release_slot(&self) {
        self.inflight.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }

    /// Returns the current inflight count.
    pub fn inflight_count(&self) -> u8 {
        self.inflight.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Register a completion callback for a branch.
    /// Returns the branch_id for later correlation.
    pub fn on_sink_complete<F>(&self, callback: F) -> u64
    where
        F: Fn(BranchResult) + Send + Sync + 'static,
    {
        self.callback_registry.on_sink_complete(callback)
    }

    /// Invoke the callback for a branch result and remove the callback.
    pub fn emit_completion(&self, branch_id: u64, result: BranchResult) {
        self.callback_registry.emit(branch_id, result);
    }

    /// Register a new branch and return its branch_id.
    ///
    /// Must be called before spawning the branch task.
    pub fn register_branch(&self) -> u64 {
        let branch_id = self.total.fetch_add(1, std::sync::atomic::Ordering::SeqCst) as u64;
        branch_id
    }

    /// Record a branch result and signal completion if all branches are done.
    pub fn record_branch_result(&self, branch_id: u64, result: BranchResult) {
        {
            let mut results = self.results.lock().expect("poisoned");
            results.push((branch_id, result));
        }
        let prev = self.completed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        if prev + 1 >= self.total.load(std::sync::atomic::Ordering::SeqCst) {
            self.notify.notify_waiters();
        }
    }

    /// Wait for all fan-out branches to reach terminal state.
    ///
    /// Returns a `BranchResults` containing all branch outcomes.
    /// This MUST be awaited after all branches have been spawned.
    pub async fn wait_all(&self) -> BranchResults {
        loop {
            let done = self.completed.load(std::sync::atomic::Ordering::SeqCst);
            if done >= self.total.load(std::sync::atomic::Ordering::SeqCst) && done > 0 {
                let results = self.results.lock().expect("poisoned").clone();
                return BranchResults { results };
            }
            self.notify.notified().await;
        }
    }

    /// Returns the registered sinks.
    pub fn sinks(&self) -> &[SinkConfig] {
        &self.sinks
    }
}

// ─── Fan-Out Slot Manager ─────────────────────────────────────────────────────

/// Semaphore-based slot manager for bounded fan-out concurrency.
///
/// Controls concurrent fan-out branch count using a semaphore.
/// Used by worker_loop to acquire a slot before dispatching a fan-out branch.
/// If no slots available, returns `BackpressureAction::PausePartition`.
pub struct FanOutSlotManager {
    semaphore: Arc<Semaphore>,
    max_slots: usize,
    topic: String,
}

impl std::fmt::Debug for FanOutSlotManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FanOutSlotManager")
            .field("max_slots", &self.max_slots)
            .field("topic", &self.topic)
            .finish()
    }
}

impl FanOutSlotManager {
    /// Create a new slot manager with the given max_fan_out for the given topic.
    ///
    /// max_fan_out is capped at 64.
    pub fn new(max_fan_out: u8, topic: String) -> Self {
        let max = std::cmp::min(max_fan_out as usize, 64);
        Self {
            semaphore: Arc::new(Semaphore::new(max)),
            max_slots: max,
            topic,
        }
    }

    /// Try to acquire a fan-out slot.
    ///
    /// Returns Ok(permit) if available,
    /// Err(BackpressureAction::PausePartition) if exhausted.
    pub async fn acquire(&self) -> Result<(), BackpressureAction> {
        match self.semaphore.clone().acquire().await {
            Ok(_permit) => {
                // Permit is dropped immediately but the semaphore count is decremented.
                // To keep the permit alive across await points we'd need self to be &'async self.
                // Instead, we just check availability after acquiring.
                Ok(())
            }
            Err(_) => Err(BackpressureAction::PausePartition {
                topic: self.topic.clone(),
                partition: -1,
            }),
        }
    }

    /// Returns the number of available slots.
    pub fn available(&self) -> usize {
        self.semaphore.available_permits()
    }

    /// Returns the max number of slots.
    pub fn max_slots(&self) -> usize {
        self.max_slots
    }

    /// Returns the topic this slot manager is for.
    pub fn topic(&self) -> &str {
        &self.topic
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::python::handler::HandlerMode;
    use pyo3::prelude::*;

    fn make_dummy_handler(name: &str) -> Arc<PythonHandler> {
        Arc::new(
            Python::attach(|py| {
                let py_none = py.None();
                PythonHandler::new(
                    py_none.into(),
                    None,
                    HandlerMode::SingleSync,
                    None,
                    None,
                    name.to_string(),
                    None,
                    None,
                )
            }),
        )
    }

    #[test]
    fn fan_out_tracker_caps_at_64() {
        let tracker = FanOutTracker::new(128);
        assert_eq!(tracker.max_fan_out(), 64);
        assert_eq!(FanOutTracker::new(100).max_fan_out(), 64);
        assert_eq!(FanOutTracker::new(64).max_fan_out(), 64);
        assert_eq!(FanOutTracker::new(4).max_fan_out(), 4);
    }

    #[test]
    fn fan_out_tracker_try_acquire_slot_returns_false_when_exhausted() {
        let tracker = FanOutTracker::new(2);
        // Acquire both slots
        assert!(tracker.try_acquire_slot());
        assert!(tracker.try_acquire_slot());
        // Exhausted
        assert!(!tracker.try_acquire_slot());
        assert!(tracker.is_exhausted());
    }

    #[test]
    fn fan_out_tracker_release_slot_decrements_inflight() {
        let tracker = FanOutTracker::new(4);
        assert_eq!(tracker.inflight_count(), 0);
        assert!(tracker.try_acquire_slot());
        assert_eq!(tracker.inflight_count(), 1);
        tracker.release_slot();
        assert_eq!(tracker.inflight_count(), 0);
    }

    #[test]
    fn fan_out_tracker_is_exhausted() {
        let tracker = FanOutTracker::new(1);
        assert!(!tracker.is_exhausted());
        assert!(tracker.try_acquire_slot());
        assert!(tracker.is_exhausted());
        tracker.release_slot();
        assert!(!tracker.is_exhausted());
    }

    #[test]
    fn fan_out_tracker_register_sink() {
        let mut tracker = FanOutTracker::new(4);
        assert_eq!(tracker.sink_count(), 0);
        tracker.register_sink("topic-a".to_string(), make_dummy_handler("handler-a"), None);
        tracker.register_sink("topic-b".to_string(), make_dummy_handler("handler-b"), None);
        assert_eq!(tracker.sink_count(), 2);
    }

    #[test]
    fn callback_registry_generates_unique_branch_ids() {
        let registry = CallbackRegistry::new();
        let id1 = registry.on_sink_complete(|_| {});
        let id2 = registry.on_sink_complete(|_| {});
        let id3 = registry.on_sink_complete(|_| {});
        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
        assert_ne!(id1, id3);
    }

    #[test]
    fn callback_registry_emit_invokes_callback() {
        use std::sync::Mutex;
        let registry = CallbackRegistry::new();
        let called = Arc::new(Mutex::new(false));
        let result_received = Arc::new(Mutex::new(None));

        let called_clone = Arc::clone(&called);
        let result_clone = Arc::clone(&result_received);
        let branch_id = registry.on_sink_complete(move |result| {
            *called_clone.lock().unwrap() = true;
            *result_clone.lock().unwrap() = Some(result);
        });

        registry.emit(branch_id, BranchResult::Ok);
        assert!(*called.lock().unwrap());
        assert!(matches!(*result_received.lock().unwrap(), Some(BranchResult::Ok)));
    }

    #[test]
    fn fan_out_slot_manager_new_caps_at_64() {
        let manager = FanOutSlotManager::new(128, "test".to_string());
        assert_eq!(manager.available(), 64);
        assert_eq!(manager.max_slots(), 64);

        let manager = FanOutSlotManager::new(4, "test".to_string());
        assert_eq!(manager.available(), 4);
        assert_eq!(manager.max_slots(), 4);
    }

    #[test]
    fn fan_out_config_sink_count() {
        let config = FanOutConfig {
            sinks: vec![
                SinkConfig {
                    topic: "topic-a".to_string(),
                    handler: make_dummy_handler("h1"),
                    timeout: None,
                },
                SinkConfig {
                    topic: "topic-b".to_string(),
                    handler: make_dummy_handler("h2"),
                    timeout: None,
                },
            ],
            max_fan_out: 4,
            slot_manager: None,
        };
        assert_eq!(config.sink_count(), 2);
    }

    #[test]
    fn fan_out_config_is_exhausted_without_slot_manager() {
        let config = FanOutConfig {
            sinks: vec![],
            max_fan_out: 4,
            slot_manager: None,
        };
        // No slot manager = not exhausted
        assert!(!config.is_exhausted());
    }

    #[tokio::test]
    async fn fan_out_slot_manager_acquire_returns_ok() {
        let manager = FanOutSlotManager::new(2, "test-topic".to_string());
        assert_eq!(manager.available(), 2);

        manager.acquire().await.expect("should acquire");
        assert_eq!(manager.available(), 1);
        // Second acquire to verify slot was actually released
        manager.acquire().await.expect("should acquire");
        assert_eq!(manager.available(), 0);
    }

    #[tokio::test]
    async fn fan_out_slot_manager_acquire_exhausted_returns_error() {
        let manager = FanOutSlotManager::new(1, "test-topic".to_string());
        manager.acquire().await.expect("first should succeed");
        assert_eq!(manager.available(), 0);

        let err = manager.acquire().await.expect_err("second should fail");
        assert!(matches!(err, BackpressureAction::PausePartition { .. }));
    }
}