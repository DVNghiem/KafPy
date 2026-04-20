//! Shutdown coordination module.
//!
//! Provides 4-phase shutdown lifecycle: Running -> Draining -> Finalizing -> Done.

pub mod shutdown;

pub use shutdown::{ShutdownCoordinator, ShutdownPhase};
