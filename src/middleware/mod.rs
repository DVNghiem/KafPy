//! Handler middleware chain for cross-cutting concerns.

//! Middleware wraps Python handler invocation to add logging, metrics, retries,
//! and other cross-cutting behavior without duplicating handler code.

pub mod chain;
pub mod logging;
pub mod metrics;
pub mod traits;

pub use chain::MiddlewareChain;
pub use logging::Logging;
pub use metrics::Metrics;
pub use traits::HandlerMiddleware;