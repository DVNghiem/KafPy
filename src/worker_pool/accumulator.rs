//! Per-partition message accumulator with fixed-window timer.

use tokio::time::Instant;

use crate::dispatcher::OwnedMessage;

/// Per-partition message accumulator with fixed-window timer.
///
/// Timer starts on first message arrival. Deadline is FIXED at first arrival +
/// max_batch_wait_ms — it does NOT reset on subsequent messages (D-02 fixed-window).
/// Each partition maintains its own timer independently.
pub struct PerPartitionBuffer {
    messages: Vec<OwnedMessage>,
    deadline: Option<Instant>,
}

impl Default for PerPartitionBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl PerPartitionBuffer {
    /// Create a new empty buffer with no deadline set.
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            deadline: None,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Start the fixed-window timer on first message arrival.
    fn start_timer(&mut self, max_wait: std::time::Duration) {
        if self.deadline.is_none() {
            self.deadline = Some(Instant::now() + max_wait);
        }
    }

    /// Returns true if deadline has expired (used for polling in select! loop).
    pub fn is_deadline_expired(&self) -> bool {
        self.deadline.map(|d| Instant::now() >= d).unwrap_or(false)
    }

    /// Returns the current deadline, if one has been set.
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    /// Add a message, starting the timer if this is the first message.
    pub fn add(&mut self, msg: OwnedMessage, max_wait: std::time::Duration) {
        if self.messages.is_empty() {
            self.start_timer(max_wait);
        }
        self.messages.push(msg);
    }

    /// Take all messages from this partition accumulator, clearing it.
    pub fn take_messages(&mut self) -> Vec<OwnedMessage> {
        std::mem::take(&mut self.messages)
    }
}
