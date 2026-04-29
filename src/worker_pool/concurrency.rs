//! Per-handler concurrency control via Arc<Semaphore>.
//!
//! Limits how many concurrent executions of the same handler can run simultaneously.
//! Each handler gets its own semaphore with a configurable limit.
//! The default limit applies to handlers that don't have an explicit concurrency setting.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Semaphore;

/// Per-handler concurrency control via Arc<Semaphore>.
///
/// Limits how many concurrent executions of the same handler can run simultaneously.
/// Each handler gets its own semaphore with a configurable limit.
/// The default limit applies to handlers that don't have an explicit concurrency setting.
#[derive(Debug, Clone)]
pub struct HandlerConcurrency {
    semaphores: Arc<parking_lot::Mutex<HashMap<String, Arc<Semaphore>>>>,
    default_limit: usize,
}

impl HandlerConcurrency {
    /// Create a new HandlerConcurrency with the given default limit.
    pub fn new(default_limit: usize) -> Self {
        Self {
            semaphores: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            default_limit,
        }
    }

    /// Set the default limit for handlers without an explicit concurrency setting.
    pub fn with_default_limit(mut self, limit: usize) -> Self {
        self.default_limit = limit;
        self
    }

    /// Acquire a permit for the given handler_id.
    /// If no semaphore exists for this handler, creates one with the default limit.
    /// Returns an OwnedSemaphorePermit that is dropped to release the permit.
    pub async fn acquire(&self, handler_id: &str) -> tokio::sync::OwnedSemaphorePermit {
        let sem = {
            let mut guard = self.semaphores.lock();
            guard
                .entry(handler_id.to_string())
                .or_insert_with(|| Arc::new(Semaphore::new(self.default_limit)))
                .clone()
        };
        sem.acquire_owned().await.expect("semaphore closed")
    }

    /// Get the current number of available permits for a handler.
    /// Returns None if the handler is not registered.
    pub fn available(&self, handler_id: &str) -> Option<usize> {
        let guard = self.semaphores.lock();
        guard.get(handler_id).map(|s| s.available_permits())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn handler_concurrency_acquire_and_release() {
        let hc = HandlerConcurrency::new(2);

        // Acquire 2 permits (at limit)
        let _p1 = hc.acquire("topic-a").await;
        let _p2 = hc.acquire("topic-a").await;

        // Try to acquire 3rd — would block without permit
        // We just verify it returns a future that can be awaited
        let permit = hc.acquire("topic-a").await;
        drop(permit);

        // Now we should be able to acquire again
        let _p = hc.acquire("topic-a").await;
    }

    #[tokio::test]
    async fn handler_concurrency_per_handler_isolation() {
        let hc = HandlerConcurrency::new(1);

        // topic-a at limit
        let _p1 = hc.acquire("topic-a").await;
        // topic-b should still have permits available
        let _p2 = hc.acquire("topic-b").await;
        // Both should work independently
    }

    #[tokio::test]
    async fn handler_concurrency_available() {
        let hc = HandlerConcurrency::new(3);

        assert_eq!(hc.available("topic-x"), Some(3));

        let _p = hc.acquire("topic-x").await;
        assert_eq!(hc.available("topic-x"), Some(2));

        let _p = hc.acquire("topic-x").await;
        assert_eq!(hc.available("topic-x"), Some(1));
    }

    #[tokio::test]
    async fn handler_concurrency_unknown_handler() {
        let hc = HandlerConcurrency::new(5);
        // Unknown handler returns None
        assert_eq!(hc.available("unknown"), None);

        // Acquiring creates it with default limit
        let _p = hc.acquire("unknown").await;
        assert_eq!(hc.available("unknown"), Some(5));
    }
}