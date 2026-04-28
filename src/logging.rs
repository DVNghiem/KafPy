//! Logging module — KafPy uses Python's logging module exclusively.
//!
//! All log messages are emitted through Python's `logging` module. Users should
//! configure logging in their Python code:
//!
//! ```python
//! import logging
//! logging.basicConfig(level=logging.INFO, format='%(asctime)s - %(name)s - %(levelname)s - %(message)s')
//! ```
//!
//! There is no Rust-side logging configuration. This is a no-op placeholder.

/// Initialize logging system - no-op, Python logging is configured in Python code.
pub fn init() {}

