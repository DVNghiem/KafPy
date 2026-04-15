//! Kafka consumer core module.
//!
//! Provides a clean, PyO3-free Rust consumer built on `rdkafka` and Tokio.
//!
//! ## Core types
//!
//! - [`ConsumerConfig`] / [`ConsumerConfigBuilder`] — configuration with sensible defaults
//! - [`OwnedMessage`] — owned message envelope; safe to process, route, or store
//! - [`ConsumerRunner`] — async message loop; converts borrowed messages to owned
//! - [`ConsumerTask`] — consumer runner spawned as a Tokio task
//! - [`ConsumerError`] — all error variants
//!
//! ## Usage
//!
//! ```
//! use kafpy::consumer::{ConsumerConfigBuilder, ConsumerTask};
//!
//! let config = ConsumerConfigBuilder::new()
//!     .brokers("localhost:9092")
//!     .group_id("my-group")
//!     .topics(["events"])
//!     .build()
//!     .unwrap();
//!
//! let runner = kafpy::consumer::ConsumerRunner::new(config).unwrap();
//!
//! let task = ConsumerTask::spawn(runner, |msg| async move {
//!     println!("{:?}", msg);
//!     Ok(())
//! }).unwrap();
//! ```

pub mod config;
pub mod error;
pub mod message;
pub mod runner;

pub use config::{
    AutoOffsetReset, ConsumerConfig, ConsumerConfigBuilder, PartitionAssignmentStrategy,
};
pub use error::ConsumerError;
pub use message::{MessageRef, MessageTimestamp, OwnedMessage};
pub use runner::{ConsumerRunner, ConsumerStream, ConsumerTask};
