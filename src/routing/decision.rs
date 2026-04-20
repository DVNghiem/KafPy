//! RoutingDecision — outcome of a routing evaluation.

use crate::routing::HandlerId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RejectReason {
    NoMatch,
    NoDefaultHandler,
    Explicit(String),
}

impl std::fmt::Display for RejectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RejectReason::NoMatch => write!(f, "no matching routing rule"),
            RejectReason::NoDefaultHandler => write!(f, "no default handler configured"),
            RejectReason::Explicit(s) => write!(f, "rejected: {s}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutingDecision {
    /// Route the message to the handler identified by [`HandlerId`](crate::routing::HandlerId).
    /// The HandlerId is an internal routing key, not the Kafka topic name.
    Route(HandlerId),
    Drop,
    Reject(RejectReason),
    Defer,
}

impl RoutingDecision {
    pub fn as_route(&self) -> Option<&HandlerId> {
        match self {
            RoutingDecision::Route(id) => Some(id),
            _ => None,
        }
    }

    pub fn is_drop(&self) -> bool {
        matches!(self, RoutingDecision::Drop)
    }

    pub fn is_defer(&self) -> bool {
        matches!(self, RoutingDecision::Defer)
    }
}

impl Default for RoutingDecision {
    fn default() -> Self {
        RoutingDecision::Defer
    }
}
