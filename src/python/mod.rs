//! Python execution lane.
//!
//! ## Core types
//!
//! - [`ExecutionResult`] — normalized outcome (Ok/Error/Rejected)
//! - [`ExecutionContext`] — message metadata for trace context
//! - [`Executor`] trait + [`ExecutorOutcome`] — pluggable post-execution policy
//! - [`DefaultExecutor`] — fire-and-forget, always acks
//!
//! ## Phase structure
//!
//! - Phase 9-01: ExecutionResult, ExecutionContext, Executor, ExecutorOutcome, DefaultExecutor, placeholders
//! - Phase 9-02: PythonHandler (spawn_blocking invoke) [pending]
//! - Phase 10: WorkerPool [Phase 10]

pub mod async_bridge;
pub mod context;
pub mod execution_result;
pub mod executor;
pub mod handler;

pub use context::ExecutionContext;
pub use execution_result::ExecutionResult;
pub use executor::{
    AsyncHandler, DefaultExecutor, Executor, ExecutorOutcome, OffsetAck, RetryExecutor,
};
pub use handler::PythonHandler;
