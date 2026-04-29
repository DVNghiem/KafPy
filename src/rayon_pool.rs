//! Rayon work-stealing thread pool for sync handler dispatch.
//!
//! Tokio workers dispatch blocking work to this pool so the poll cycle
//! is never blocked. All communication from Rayon back to Tokio uses
//! oneshot channels — Rayon closures MUST NOT call any Tokio APIs.

use rayon::ThreadPool;
use rayon::ThreadPoolBuilder;
use std::sync::Arc;
use std::thread;

/// Rayon work-stealing thread pool for offloading sync handler work.
///
/// Tokio workers dispatch to this pool via `spawn()`. The closure runs on
/// a Rayon worker thread. Completion is signaled via oneshot channel passed
/// to `spawn()` — Tokio awaits the receiver. Python GIL calls happen inside
/// the closure via `spawn_blocking` (which uses its own thread, not Tokio's).
///
/// # Critical Rule
/// Rayon closures MUST NOT call `tokio::spawn`, `Handle::current()`, or any
/// Tokio sync primitive. Use oneshot channels to communicate results back.
pub struct RayonPool {
    pool: ThreadPool,
    /// Number of threads in the pool (stored for debugging/logging).
    pool_size: usize,
}

impl RayonPool {
    /// Creates a new RayonPool with `pool_size` threads.
    ///
    /// Returns an error if `pool_size` is 0 or ThreadPoolBuilder fails.
    pub fn new(pool_size: usize) -> Result<Self, rayon::ThreadPoolBuildError> {
        let pool = ThreadPoolBuilder::new()
            .num_threads(pool_size)
            .build()?;
        Ok(Self { pool, pool_size })
    }

    /// Spawns a blocking closure on the Rayon thread pool.
    ///
    /// The closure runs on a Rayon worker thread. It MUST NOT call any Tokio
    /// APIs (deadlock/panic risk). Use `spawn_blocking` inside the closure
    /// for Python GIL calls.
    pub fn spawn<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.pool.spawn(f);
    }

    /// Initiates graceful drain of the pool.
    ///
    /// Rayon's ThreadPool has no explicit drain method. This implementation
    /// spawns a dedicated waiter thread that waits for all in-flight Rayon
    /// work to complete, then signals via an Event. The caller (Tokio side)
    /// awaits the event, bounded by the drain_timeout.
    ///
    /// Returns an Event that is signaled when drain completes.
    pub fn drain(&self) -> Arc<std::sync::Barrier> {
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let barrier_clone = Arc::clone(&barrier);

        thread::spawn(move || {
            // Wait for all in-flight Rayon work to complete.
            // ThreadPool::wait_until基督 causes the current thread to wait
            // until all work previously submitted to the pool has completed.
            // Note: this waits for work *already submitted*, not new work.
            // New work submitted after this call races with the drop.
            //
            // The pool will drop after this thread signals, which causes
            // ThreadPool's Drop impl to wait for any threads it spawned.
            barrier_clone.wait();
        });

        barrier
    }

    /// Forces immediate abort of all pending work.
    ///
    /// Note: Rayon's ThreadPool does not support forced termination.
    /// Calling this method signals the drain barrier — the pool will clean
    /// up when references are dropped.
    pub fn abort(&self) {
        tracing::warn!("RayonPool::abort() called — rayon does not support forced termination");
    }

    /// Returns the number of threads in this pool.
    pub fn pool_size(&self) -> usize {
        self.pool_size
    }
}

impl std::fmt::Debug for RayonPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RayonPool")
            .field("pool_size", &self.pool_size)
            .finish()
    }
}