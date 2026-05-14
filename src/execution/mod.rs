//! Callback execution lane.
//!
//! ## Core types
//!
//! - [`ExecutionResult`] — normalized outcome (Ok/Error/Rejected)
//! - [`ExecutionContext`] — message metadata for trace context
//!
pub mod async_bridge;
pub mod batch;
pub mod callback;
pub mod context;
pub mod execution_result;
pub mod fan_out;
pub mod logger;
pub mod streaming;

pub use batch::BatchAccumulator;
pub use callback::PythonHandler;
pub use context::ExecutionContext;
pub use execution_result::ExecutionResult;
