//! Python execution lane.
//!
//! ## Core types
//!
//! - [`ExecutionResult`] — normalized outcome (Ok/Error/Rejected)
//! - [`ExecutionContext`] — message metadata for trace context
//! - [`Executor`] trait + [`ExecutorOutcome`] — pluggable post-execution policy
//! - [`DefaultExecutor`] — fire-and-forget, always acks
//!
pub mod async_bridge;
pub mod batch;
pub mod context;
pub mod execution_result;
pub mod executor;
pub mod fan_out_bridge;
pub mod handler;
pub mod logger;
pub mod streaming;

pub use batch::BatchAccumulator;
pub use context::ExecutionContext;
pub use execution_result::ExecutionResult;
pub use executor::{DefaultExecutor, Executor, ExecutorOutcome};
pub use handler::PythonHandler;
