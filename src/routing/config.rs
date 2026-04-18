//! RoutingRule and RoutingRuleBuilder — configuration types for routing rules.
//!
//! Per D-03: follows the ConsumerConfigBuilder chain API shape.
//! API shape: `config.routing_rule(pattern, header, key).to_handler("handler-id").priority(1)`
//! Pattern type (glob/regex) is set via `pattern_type(PatternType)`.

use crate::routing::header::HeaderRule;
use crate::routing::key::{KeyMatchMode, KeyRule};
use crate::routing::topic_pattern::{PatternType as Pt, TopicRule};

pub use crate::routing::topic_pattern::PatternType;

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

/// Builder for routing rules. Follows the ConsumerConfigBuilder chain pattern.
///
/// # Example
/// ```ignore
/// let rule = RoutingRuleBuilder::new()
///     .topic_pattern("events.*", PatternType::Glob)
///     .header_key("content-type", Some("application/json*"))
///     .to_handler("json-handler")
///     .priority(1)
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct RoutingRuleBuilder {
    priority: i32,
    topic_pattern: Option<String>,
    pattern_type: Pt,
    header_key: Option<String>,
    header_value_pattern: Option<String>,
    key_match_mode: Option<KeyMatchMode>,
    key_pattern: Option<Vec<u8>>,
    handler_id: Option<crate::routing::HandlerId>,
}

impl RoutingRuleBuilder {
    /// Creates a new builder.
    pub fn new() -> Self {
        Self {
            priority: 0,
            topic_pattern: None,
            pattern_type: Pt::Glob,
            header_key: None,
            header_value_pattern: None,
            key_match_mode: None,
            key_pattern: None,
            handler_id: None,
        }
    }

    /// Sets the topic glob/regex pattern.
    pub fn topic_pattern(mut self, pattern: impl Into<String>, pattern_type: Pt) -> Self {
        self.topic_pattern = Some(pattern.into());
        self.pattern_type = pattern_type;
        self
    }

    /// Sets the header key to match, optionally with a glob value pattern.
    /// If value_pattern is None, any header value matches (presence only).
    pub fn header_key(mut self, key: impl Into<String>, value_pattern: Option<&str>) -> Self {
        self.header_key = Some(key.into());
        self.header_value_pattern = value_pattern.map(|s| s.into());
        self
    }

    /// Sets the key match mode and pattern (binary).
    pub fn key_match(mut self, mode: KeyMatchMode, pattern: impl Into<Vec<u8>>) -> Self {
        self.key_match_mode = Some(mode);
        self.key_pattern = Some(pattern.into());
        self
    }

    /// Sets the target handler ID (REQUIRED before build).
    pub fn to_handler(mut self, handler_id: impl Into<crate::routing::HandlerId>) -> Self {
        self.handler_id = Some(handler_id.into());
        self
    }

    /// Sets the priority (lower = evaluated first). Default is 0.
    pub fn priority(mut self, priority: i32) -> Self {
        self.priority = priority;
        self
    }

    /// Builds the RoutingRule, or returns an error if handler_id is not set.
    pub fn build(self) -> Result<RoutingRule, RoutingRuleBuildError> {
        let handler_id = self
            .handler_id
            .ok_or(RoutingRuleBuildError::MissingHandler)?;

        let topic_rule = self.topic_pattern.map(|pattern| TopicRule {
            pattern,
            pattern_type: self.pattern_type.clone(),
            handler_id: handler_id.clone(),
        });

        let header_rule = self.header_key.map(|key| HeaderRule {
            key,
            value_pattern: self.header_value_pattern,
            handler_id: handler_id.clone(),
        });

        let key_rule = match (self.key_match_mode, self.key_pattern) {
            (Some(mode), Some(pattern)) => Some(KeyRule {
                match_mode: mode,
                pattern,
                handler_id: handler_id.clone(),
            }),
            _ => None,
        };

        Ok(RoutingRule {
            priority: self.priority,
            topic_rule,
            header_rule,
            key_rule,
            handler_id,
        })
    }
}

impl Default for RoutingRuleBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Error type for RoutingRuleBuilder::build().
#[derive(Debug, thiserror::Error)]
pub enum RoutingRuleBuildError {
    #[error("to_handler() must be called before build()")]
    MissingHandler,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topic_rule_build() {
        let rule = RoutingRuleBuilder::new()
            .topic_pattern("events.*", Pt::Glob)
            .to_handler("handler1")
            .priority(1)
            .build()
            .unwrap();

        assert_eq!(rule.priority, 1);
        assert!(rule.topic_rule.is_some());
        assert!(rule.header_rule.is_none());
        assert!(rule.key_rule.is_none());
        assert_eq!(rule.handler_id, "handler1");
    }

    #[test]
    fn header_rule_build() {
        let rule = RoutingRuleBuilder::new()
            .header_key("content-type", Some("application/json*"))
            .to_handler("handler2")
            .build()
            .unwrap();

        assert_eq!(rule.priority, 0);
        assert!(rule.topic_rule.is_none());
        assert!(rule.header_rule.is_some());
        assert_eq!(rule.handler_id, "handler2");
    }

    #[test]
    fn key_rule_build() {
        let rule = RoutingRuleBuilder::new()
            .key_match(KeyMatchMode::Prefix, b"order-")
            .to_handler("handler3")
            .build()
            .unwrap();

        assert!(rule.key_rule.is_some());
        assert_eq!(rule.handler_id, "handler3");
    }

    #[test]
    fn combined_rules() {
        let rule = RoutingRuleBuilder::new()
            .topic_pattern("events.*", Pt::Glob)
            .header_key("content-type", Some("application/json*"))
            .key_match(KeyMatchMode::Prefix, b"order-")
            .to_handler("combined")
            .priority(5)
            .build()
            .unwrap();

        assert_eq!(rule.priority, 5);
        assert!(rule.topic_rule.is_some());
        assert!(rule.header_rule.is_some());
        assert!(rule.key_rule.is_some());
    }

    #[test]
    fn missing_handler_error() {
        let result = RoutingRuleBuilder::new()
            .topic_pattern("events.*", Pt::Glob)
            .build();
        assert!(matches!(result, Err(RoutingRuleBuildError::MissingHandler)));
    }
}
