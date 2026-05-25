//! Runtime assembly module.
//!
//! Provides [`RuntimeBuilder`] for composing the full consumer runtime from pure-Rust
//! components. This module is PyO3-free — it can be tested without Python.

pub mod builder;

pub use builder::RuntimeBuilder;
