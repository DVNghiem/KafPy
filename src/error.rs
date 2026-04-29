//! Unified error types for the KafPy crate.
//!
//! All public-facing and internal error types are re-exported here for
//! discoverability. Individual modules (`dispatcher/error.rs`, etc.) remain
//! the source of truth; this module aggregates them.
//!
//! ## Usage
//!
//! ```rust
//! use kafpy::error::{DispatchError, ConsumerError, CoordinatorError, PyError};
//! ```

pub use crate::consumer::error::ConsumerError;
pub use crate::coordinator::error::CoordinatorError;
pub use crate::dispatcher::error::DispatchError;
pub use crate::errors::PyError;
