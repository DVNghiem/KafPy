//! Offset commit coordinator module.
//!
//! Provides per-topic-partition offset tracking with highest-contiguous-offset
//! algorithm for at-least-once delivery guarantees.

pub mod commit_task;
pub mod error;
pub mod offset_coordinator;
pub mod offset_tracker;

pub use commit_task::{CommitConfig, TopicPartition};
pub use error::CoordinatorError;
pub use offset_coordinator::OffsetCoordinator;
pub use offset_tracker::{OffsetTracker, PartitionState};
