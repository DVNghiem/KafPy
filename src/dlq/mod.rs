//! DLQ — dead-letter queue routing infrastructure.
//!
//! Provides DlqMetadata envelope and DlqRouter trait for routing failed
//! messages to DLQ topics with full metadata.

pub mod metadata;
pub mod router;

pub use metadata::DlqMetadata;
pub use router::{DlqRouter, DefaultDlqRouter, TopicPartition};
