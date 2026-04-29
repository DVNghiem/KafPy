//! Shutdown coordinator for orchestrating graceful consumer shutdown.
//!
//! Manages a 4-phase shutdown lifecycle: Running -> Draining -> Finalizing -> Done.
//! Each phase transition is explicit and enforced; invalid transitions panic.

use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::info;

/// Phase of the shutdown lifecycle.
///
/// Transitions are strictly ordered: Running -> Draining -> Finalizing -> Done.
/// Invalid transitions cause a panic to prevent impossible states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownPhase {
    /// Consumer is running normally, accepting and processing messages.
    Running,
    /// Shutdown has been initiated; components are draining in-flight work.
    Draining,
    /// Drain complete; final offsets are being committed.
    Finalizing,
    /// Shutdown complete; consumer is ready to drop.
    Done,
}

impl std::fmt::Display for ShutdownPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShutdownPhase::Running => write!(f, "Running"),
            ShutdownPhase::Draining => write!(f, "Draining"),
            ShutdownPhase::Finalizing => write!(f, "Finalizing"),
            ShutdownPhase::Done => write!(f, "Done"),
        }
    }
}

/// Coordinates graceful shutdown across all consumer components.
///
/// ## Shutdown Order (prevents circular wait deadlock)
///
/// 1. **Dispatcher stop** — stop new dispatches first
/// 2. **Worker drain** — wait for in-flight work with timeout
/// 3. **Offset finalize** — commit final safe offsets
/// 4. **Consumer drop** — rd_kafka_consumer_close() called automatically
///
/// ## Phase Transitions
///
/// ```
/// Running -> Draining -> Finalizing -> Done
/// ```
///
/// Each transition is validated; invalid transitions panic.
pub struct ShutdownCoordinator {
    phase: parking_lot::Mutex<ShutdownPhase>,
    drain_timeout: Duration,
    /// Cancellation token for the dispatcher — cancelled to stop new dispatches.
    dispatcher_cancel: CancellationToken,
    /// Cancellation token for workers — cancelled to stop accepting new work.
    worker_cancel: CancellationToken,
    /// Cancellation token for the committer — cancelled to enter finalization.
    committer_cancel: CancellationToken,
}

impl ShutdownCoordinator {
    /// Creates a new coordinator in the `Running` phase.
    ///
    /// # Arguments
    ///
    /// * `drain_timeout_secs` — seconds to wait for worker drain before force-abort
    pub fn new(drain_timeout_secs: u64) -> Self {
        let drain_timeout = Duration::from_secs(drain_timeout_secs);
        tracing::debug!(
            drain_timeout_secs = drain_timeout_secs,
            "ShutdownCoordinator created"
        );
        Self {
            phase: parking_lot::Mutex::new(ShutdownPhase::Running),
            drain_timeout,
            dispatcher_cancel: CancellationToken::new(),
            worker_cancel: CancellationToken::new(),
            committer_cancel: CancellationToken::new(),
        }
    }

    /// Returns the current shutdown phase (read-only).
    pub fn phase(&self) -> ShutdownPhase {
        *self.phase.lock()
    }

    /// Initiates the draining phase, transitioning `Running` -> `Draining`.
    ///
    /// Returns the three cancellation tokens for dispatcher, workers, and committer.
    /// Components receive these tokens and use them to interrupt their loops.
    ///
    /// # Panics
    ///
    /// Panics if the current phase is not `Running`.
    pub fn begin_draining(&self) -> (CancellationToken, CancellationToken, CancellationToken) {
        {
            let mut phase = self.phase.lock();
            assert_eq!(
                *phase,
                ShutdownPhase::Running,
                "begin_draining() called in non-Running phase"
            );
            *phase = ShutdownPhase::Draining;
        }
        info!(
            phase = %self.phase(),
            drain_timeout_secs = self.drain_timeout.as_secs(),
            "shutdown initiated, signaling components to drain"
        );
        (
            self.dispatcher_cancel.clone(),
            self.worker_cancel.clone(),
            self.committer_cancel.clone(),
        )
    }

    /// Transitions to the finalization phase: `Draining` -> `Finalizing`.
    ///
    /// Called after workers have drained and before final offsets are committed.
    ///
    /// # Panics
    ///
    /// Panics if the current phase is not `Draining`.
    pub fn begin_finalizing(&self) {
        {
            let mut phase = self.phase.lock();
            assert_eq!(
                *phase,
                ShutdownPhase::Draining,
                "begin_finalizing() called in non-Draining phase"
            );
            *phase = ShutdownPhase::Finalizing;
        }
        info!(
            phase = %self.phase(),
            "drain complete, entering finalization phase"
        );
    }

    /// Marks shutdown as complete: `Finalizing` -> `Done`.
    ///
    /// Called after all final offsets have been committed.
    ///
    /// # Panics
    ///
    /// Panics if the current phase is not `Finalizing`.
    pub fn set_done(&self) {
        {
            let mut phase = self.phase.lock();
            assert_eq!(
                *phase,
                ShutdownPhase::Finalizing,
                "set_done() called in non-Finalizing phase"
            );
            *phase = ShutdownPhase::Done;
        }
        info!(
            phase = %self.phase(),
            "shutdown complete"
        );
    }

    /// Returns true if shutdown has completed (phase is `Done`).
    pub fn is_done(&self) -> bool {
        *self.phase.lock() == ShutdownPhase::Done
    }

    /// Returns the configured drain timeout duration.
    pub fn drain_timeout(&self) -> Duration {
        self.drain_timeout
    }

    /// Returns a reference to the committer cancellation token.
    #[allow(dead_code)]
    pub(crate) fn committer_cancel_token(&self) -> CancellationToken {
        self.committer_cancel.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_starts_in_running() {
        let coord = ShutdownCoordinator::new(30);
        assert_eq!(coord.phase(), ShutdownPhase::Running);
        assert_eq!(coord.drain_timeout(), Duration::from_secs(30));
    }

    #[test]
    fn begin_draining_transitions_to_draining() {
        let coord = ShutdownCoordinator::new(30);
        let (disp, work, comm) = coord.begin_draining();
        assert_eq!(coord.phase(), ShutdownPhase::Draining);
        // Tokens are distinct clones
        assert!(!disp.is_cancelled());
        assert!(!work.is_cancelled());
        assert!(!comm.is_cancelled());
    }

    #[test]
    fn begin_draining_then_finalizing() {
        let coord = ShutdownCoordinator::new(30);
        coord.begin_draining();
        coord.begin_finalizing();
        assert_eq!(coord.phase(), ShutdownPhase::Finalizing);
    }

    #[test]
    fn finalizing_then_done() {
        let coord = ShutdownCoordinator::new(30);
        coord.begin_draining();
        coord.begin_finalizing();
        coord.set_done();
        assert_eq!(coord.phase(), ShutdownPhase::Done);
        assert!(coord.is_done());
    }

    #[test]
    fn is_done_false_until_done() {
        let coord = ShutdownCoordinator::new(30);
        assert!(!coord.is_done());
        coord.begin_draining();
        assert!(!coord.is_done());
        coord.begin_finalizing();
        assert!(!coord.is_done());
        coord.set_done();
        assert!(coord.is_done());
    }

    #[test]
    #[should_panic(expected = "begin_draining() called in non-Running phase")]
    fn double_begin_draining_panics() {
        let coord = ShutdownCoordinator::new(30);
        coord.begin_draining();
        coord.begin_draining();
    }

    #[test]
    #[should_panic(expected = "begin_finalizing() called in non-Draining phase")]
    fn finalize_without_drain_panics() {
        let coord = ShutdownCoordinator::new(30);
        coord.begin_finalizing();
    }

    #[test]
    #[should_panic(expected = "set_done() called in non-Finalizing phase")]
    fn done_without_finalize_panics() {
        let coord = ShutdownCoordinator::new(30);
        coord.begin_draining();
        coord.set_done();
    }

    #[test]
    fn shutdown_phase_display() {
        assert_eq!(ShutdownPhase::Running.to_string(), "Running");
        assert_eq!(ShutdownPhase::Draining.to_string(), "Draining");
        assert_eq!(ShutdownPhase::Finalizing.to_string(), "Finalizing");
        assert_eq!(ShutdownPhase::Done.to_string(), "Done");
    }

    #[test]
    fn custom_drain_timeout() {
        let coord = ShutdownCoordinator::new(60);
        assert_eq!(coord.drain_timeout(), Duration::from_secs(60));
    }
}
