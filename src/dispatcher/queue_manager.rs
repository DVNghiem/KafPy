//! QueueManager owns all handler queues and metadata with atomic counters.
//!
//! Since Tokio `mpsc` channels do not expose a `len()` method, we track queue depth
//! and in-flight counts manually via `AtomicUsize` counters.
//!
//! - `queue_depth`: messages currently buffered in the bounded channel (sender-side).
//! - `inflight`: messages dispatched to the handler but not yet acknowledged via `ack()`.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::sync::mpsc;

pub use crate::consumer::OwnedMessage;
use crate::dispatcher::error::DispatchError;

/// Metadata for a registered handler — tracks queue depth and inflight counts.
pub(crate) struct HandlerMetadata {
    /// Bounded channel capacity for this handler.
    pub capacity: usize,
    /// Messages currently buffered in the channel.
    pub(crate) queue_depth: AtomicUsize,
    /// Messages dispatched but not yet acknowledged.
    pub(crate) inflight: AtomicUsize,
    /// Optional semaphore for concurrency limiting (acquired before dispatch).
    pub(crate) semaphore: Option<Arc<Semaphore>>,
}

impl HandlerMetadata {
    /// Creates a new metadata entry with zero counters and optional semaphore.
    pub fn new(capacity: usize, semaphore: Option<Arc<Semaphore>>) -> Self {
        Self {
            capacity,
            queue_depth: AtomicUsize::new(0),
            inflight: AtomicUsize::new(0),
            semaphore,
        }
    }

    /// Tries to acquire a semaphore permit before dispatch.
    /// Returns `true` if permit acquired (or no semaphore), `false` if no permit available.
    pub fn try_acquire_semaphore(&self) -> bool {
        match &self.semaphore {
            Some(sem) => sem.try_acquire().is_ok(),
            None => true,
        }
    }

    /// Returns the current queue depth.
    pub fn get_queue_depth(&self) -> usize {
        self.queue_depth.load(Ordering::Relaxed)
    }

    /// Returns the current inflight count.
    pub fn get_inflight(&self) -> usize {
        self.inflight.load(Ordering::Relaxed)
    }

    /// Increments queue_depth by 1 (called when message is buffered).
    fn inc_queue_depth(&self) {
        self.queue_depth.fetch_add(1, Ordering::Relaxed);
    }

    /// Increments inflight by 1 (called when message is dispatched).
    fn inc_inflight(&self) {
        self.inflight.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrements both queue_depth and inflight by `count`.
    /// Uses saturating semantics so the counter never underflows below 0.
    fn ack(&self, count: usize) {
        // Saturating subtract: loop until we successfully subtract without underflow.
        loop {
            let current = self.queue_depth.load(Ordering::Relaxed);
            let new = current.saturating_sub(count);
            if self
                .queue_depth
                .compare_exchange(current, new, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                break;
            }
        }
        loop {
            let current = self.inflight.load(Ordering::Relaxed);
            let new = current.saturating_sub(count);
            if self
                .inflight
                .compare_exchange(current, new, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                break;
            }
        }
    }
}

/// Pairs a handler's sender with its metadata.
pub(crate) struct HandlerEntry {
    pub sender: mpsc::Sender<OwnedMessage>,
    pub(crate) metadata: HandlerMetadata,
}

/// Owns all handler queues and their atomic metadata counters.
pub(crate) struct QueueManager {
    pub(crate) handlers: parking_lot::Mutex<std::collections::HashMap<String, HandlerEntry>>,
}

impl QueueManager {
    /// Creates a new empty QueueManager.
    pub fn new() -> Self {
        Self {
            handlers: parking_lot::Mutex::new(std::collections::HashMap::new()),
        }
    }

    /// Registers a handler for `topic` with a bounded channel of `capacity`.
    ///
    /// Returns the `mpsc::Receiver` to the caller (Python/PyO3 layer).
    /// Thread-safe.
    pub fn register_handler(
        &self,
        topic: impl Into<String>,
        capacity: usize,
    ) -> mpsc::Receiver<OwnedMessage> {
        self.register_handler_with_semaphore(topic, capacity, None)
    }

    /// Registers a handler with optional semaphore for concurrency limiting.
    pub(crate) fn register_handler_with_semaphore(
        &self,
        topic: impl Into<String>,
        capacity: usize,
        semaphore: Option<Arc<Semaphore>>,
    ) -> mpsc::Receiver<OwnedMessage> {
        let (tx, rx) = mpsc::channel(capacity);
        let metadata = HandlerMetadata::new(capacity, semaphore);
        let entry = HandlerEntry { sender: tx, metadata };
        self.handlers.lock().insert(topic.into(), entry);
        rx
    }

    /// Returns the capacity for `topic`, or `None` if not registered.
    pub fn get_capacity(&self, topic: &str) -> Option<usize> {
        self.handlers
            .lock()
            .get(topic)
            .map(|entry| entry.metadata.capacity)
    }

    /// Returns the current queue depth for `topic`, or `None` if not registered.
    pub fn get_queue_depth(&self, topic: &str) -> Option<usize> {
        self.handlers
            .lock()
            .get(topic)
            .map(|entry| entry.metadata.get_queue_depth())
    }

    /// Returns the current inflight count for `topic`, or `None` if not registered.
    pub fn get_inflight(&self, topic: &str) -> Option<usize> {
        self.handlers
            .lock()
            .get(topic)
            .map(|entry| entry.metadata.get_inflight())
    }

    /// Called by the Python/PyO3 layer after processing `count` messages.
    /// Decrements both `queue_depth` and `inflight` by `count`.
    /// Idempotent — no-op if `topic` is not registered.
    pub fn ack(&self, topic: &str, count: usize) {
        if let Some(entry) = self.handlers.lock().get(topic) {
            entry.metadata.ack(count);
        }
    }

    /// Internal: sends `message` to the handler registered for `message.topic`.
    ///
    /// Increments `queue_depth` (message buffered) and `inflight` (dispatched).
    /// Returns `DispatchOutcome` on success, `DispatchError` on failure.
    pub(crate) fn send_to_handler(
        &self,
        message: OwnedMessage,
    ) -> Result<crate::dispatcher::DispatchOutcome, DispatchError> {
        let topic = message.topic.clone();
        let partition = message.partition;
        let offset = message.offset;

        let guard = self.handlers.lock();
        let entry = guard
            .get(&topic)
            .ok_or_else(|| DispatchError::HandlerNotRegistered(topic.clone()))?;

        match entry.sender.try_send(message) {
            Ok(()) => {
                // Message is now buffered in the channel.
                entry.metadata.inc_queue_depth();
                // Message is dispatched to handler for processing.
                entry.metadata.inc_inflight();
                Ok(crate::dispatcher::DispatchOutcome {
                    topic,
                    partition,
                    offset,
                    queue_depth: entry.metadata.get_queue_depth(),
                })
            }
            Err(TrySendError::Full(_)) => Err(DispatchError::QueueFull(topic)),
            Err(TrySendError::Closed(_)) => Err(DispatchError::QueueClosed(topic)),
        }
    }
}

use tokio::sync::mpsc::error::TrySendError;
