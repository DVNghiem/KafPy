//! Routing infrastructure — zero-copy routing context, decision enum, and router trait.

pub mod callback_router;
pub mod chain;
pub mod config;
pub mod context;
pub mod decision;
pub mod header;
pub mod key;
pub mod router;
pub mod topic_pattern;

pub use context::HandlerId;
