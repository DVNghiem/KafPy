//! Explicit state machine enums for worker_pool loops.
//!
//! Replaces implicit state (Option, bool) with compiler-verified exhaustive enums.

use crate::dispatcher::OwnedMessage;

/// Worker state machine for worker_loop.
///
/// Replaces implicit `active_message: Option<OwnedMessage>` state.
/// The enum makes illegal states unrepresentable: you cannot be "processing"
/// without a message, and you cannot "retry" without a message.
#[derive(Debug, Clone)]
pub enum WorkerState {
    /// Worker is idle, waiting for a message from the channel.
    Idle,
    /// Worker is actively processing a message.
    Processing(OwnedMessage),
}

impl WorkerState {
    /// Returns true if worker is currently processing a message.
    pub fn is_processing(&self) -> bool {
        matches!(self, WorkerState::Processing(_))
    }

    /// Returns the message if currently processing, None otherwise.
    pub fn message(&self) -> Option<&OwnedMessage> {
        match self {
            WorkerState::Idle => None,
            WorkerState::Processing(msg) => Some(msg),
        }
    }
}

/// Batch worker state machine for batch_worker_loop.
///
/// Replaces implicit `backpressure_active: bool` flag.
/// The enum makes backpressure state explicit and allows future states
/// (e.g., Draining, Flushing) to be added without bool gymnastics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchState {
    /// Normal operation: accumulating messages, flushing on batch full/deadline.
    Normal,
    /// Backpressure active: flushed accumulator, blocking on capacity before accepting more.
    Backpressure,
}

impl BatchState {
    /// Returns true if currently in backpressure state.
    pub fn is_backpressure(&self) -> bool {
        matches!(self, BatchState::Backpressure)
    }
}
