//! Coordinator module — thin re-export layer.
//!
//! This module re-exports from offset/, shutdown/, and retry/ submodules.
//! No new behavior here; all logic lives in the responsibility-split modules.

pub mod error; // kept in place

// Re-export from offset/ submodule
pub use crate::offset::commit_task::{CommitConfig, OffsetCommitter, TopicPartition};
pub use crate::offset::offset_coordinator::OffsetCoordinator;
pub use crate::offset::offset_tracker::OffsetTracker;

// Re-export from shutdown/ submodule
pub use crate::shutdown::{ShutdownCoordinator, ShutdownPhase};

// Re-export from retry/ submodule
pub use crate::retry::{RetryCoordinator, RetryPolicy, RetryState};

// Re-export error types at coordinator/ level for backward compatibility
pub use thiserror::Error;
