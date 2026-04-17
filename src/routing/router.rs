//! router — Router trait definition.

use crate::routing::context::RoutingContext;
use crate::routing::decision::RoutingDecision;

pub trait Router: Send + Sync {
    fn route(&self, ctx: &RoutingContext) -> RoutingDecision;
}