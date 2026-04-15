//! Domain error types for Python-facing operations.
//!
//! Note: The pure-Rust consumer uses `consumer::error::ConsumerError` internally.
//! This module is for errors that surface at the PyO3 boundary.

use thiserror::Error;

/// Errors that cross the PyO3 boundary (Python-callable functions).
#[derive(Error, Debug)]
pub enum PyError {
    #[error("consumer error: {0}")]
    Consumer(String),

    #[error("producer error: {0}")]
    Producer(String),

    #[error("configuration error: {0}")]
    Config(String),
}
