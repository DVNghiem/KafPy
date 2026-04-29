//! Shutdown coordination module.
//!
//! Provides 4-phase shutdown lifecycle: Running -> Draining -> Finalizing -> Done.

#[allow(clippy::module_inception)]
pub mod shutdown;

pub use shutdown::{ShutdownCoordinator, ShutdownPhase};
