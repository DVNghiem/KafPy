//! RoutingRule — configuration type for routing rules.

use crate::routing::header::HeaderRule;
use crate::routing::key::KeyRule;
use crate::routing::topic_pattern::TopicRule;

/// A fully-built routing rule with a target handler and priority.
#[derive(Debug, Clone)]
pub struct RoutingRule {
    /// Priority for rule evaluation (lower = evaluated first).
    pub priority: i32,
    /// Topic pattern rule (optional).
    pub topic_rule: Option<TopicRule>,
    /// Header rule (optional).
    pub header_rule: Option<HeaderRule>,
    /// Key rule (optional).
    pub key_rule: Option<KeyRule>,
    /// Target handler ID for this rule.
    pub handler_id: crate::routing::HandlerId,
}
