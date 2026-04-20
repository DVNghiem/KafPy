//! topic_pattern — TopicPatternRouter with glob/regex support.

use crate::routing::context::RoutingContext;
use crate::routing::decision::RoutingDecision;
use crate::routing::router::Router;
use glob::Pattern;
use regex::Regex;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatternType {
    Glob,
    Regex,
}

#[derive(Debug, Clone)]
pub struct TopicRule {
    pub pattern: String,
    pub pattern_type: PatternType,
    pub handler_id: crate::routing::HandlerId,
}

#[derive(Debug, Clone)]
pub struct TopicPatternRouter {
    rules: Arc<[TopicRule]>,
    compiled: Arc<[CompiledPattern]>,
}

#[derive(Debug, Clone)]
enum CompiledPattern {
    Glob(Pattern),
    Regex(Regex),
}

impl TopicPatternRouter {
    pub fn new(rules: Vec<TopicRule>) -> Result<Self, PatternError> {
        let mut compiled = Vec::with_capacity(rules.len());
        for rule in &rules {
            let cp = match rule.pattern_type {
                PatternType::Glob => {
                    CompiledPattern::Glob(Pattern::new(&rule.pattern).map_err(|e| {
                        PatternError::InvalidGlob(rule.pattern.clone(), e.to_string())
                    })?)
                }
                PatternType::Regex => {
                    CompiledPattern::Regex(Regex::new(&rule.pattern).map_err(|e| {
                        PatternError::InvalidRegex(rule.pattern.clone(), e.to_string())
                    })?)
                }
            };
            compiled.push(cp);
        }
        Ok(Self {
            rules: rules.into(),
            compiled: compiled.into(),
        })
    }

    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }
}

impl Router for TopicPatternRouter {
    fn route(&self, ctx: &RoutingContext) -> RoutingDecision {
        for (rule, compiled) in self.rules.iter().zip(self.compiled.iter()) {
            let matches = match compiled {
                CompiledPattern::Glob(p) => p.matches(ctx.topic),
                CompiledPattern::Regex(r) => r.is_match(ctx.topic),
            };
            if matches {
                return RoutingDecision::Route(rule.handler_id.clone());
            }
        }
        RoutingDecision::Defer
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PatternError {
    #[error("invalid glob pattern '{0}': {1}")]
    InvalidGlob(String, String),
    #[error("invalid regex pattern '{0}': {1}")]
    InvalidRegex(String, String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glob_router(patterns: &[(&str, &str)]) -> TopicPatternRouter {
        let rules: Vec<TopicRule> = patterns
            .iter()
            .map(|(p, h)| TopicRule {
                pattern: (*p).into(),
                pattern_type: PatternType::Glob,
                handler_id: (*h).into(),
            })
            .collect();
        TopicPatternRouter::new(rules).unwrap()
    }

    fn regex_router(patterns: &[(&str, &str)]) -> TopicPatternRouter {
        let rules: Vec<TopicRule> = patterns
            .iter()
            .map(|(p, h)| TopicRule {
                pattern: (*p).into(),
                pattern_type: PatternType::Regex,
                handler_id: (*h).into(),
            })
            .collect();
        TopicPatternRouter::new(rules).unwrap()
    }

    fn ctx(topic: &str) -> RoutingContext<'_> {
        RoutingContext {
            topic,
            partition: 0,
            offset: 0,
            key: None,
            payload: None,
            headers: &[],
        }
    }

    #[test]
    fn glob_exact_match() {
        let router = glob_router(&[("events", "handler1")]);
        assert!(
            matches!(router.route(&ctx("events")), RoutingDecision::Route(id) if id.as_str() == "handler1")
        );
    }

    #[test]
    fn glob_wildcard_match() {
        let router = glob_router(&[("events.*", "handler1")]);
        assert!(
            matches!(router.route(&ctx("events.us")), RoutingDecision::Route(id) if id.as_str() == "handler1")
        );
        assert!(
            matches!(router.route(&ctx("events.eu")), RoutingDecision::Route(id) if id.as_str() == "handler1")
        );
    }

    #[test]
    fn glob_no_match_defer() {
        let router = glob_router(&[("events.*", "handler1")]);
        assert!(matches!(
            router.route(&ctx("other")),
            RoutingDecision::Defer
        ));
    }

    #[test]
    fn glob_first_match_wins() {
        let router = glob_router(&[("events.*", "handler1"), ("events.us", "handler2")]);
        assert!(
            matches!(router.route(&ctx("events.us")), RoutingDecision::Route(id) if id.as_str() == "handler1")
        );
    }

    #[test]
    fn regex_match() {
        let router = regex_router(&[(r"^events\..+$", "handler1")]);
        assert!(
            matches!(router.route(&ctx("events.us")), RoutingDecision::Route(id) if id.as_str() == "handler1")
        );
        assert!(matches!(
            router.route(&ctx("events")),
            RoutingDecision::Defer
        ));
    }

    #[test]
    fn invalid_glob_returns_error() {
        let result = TopicPatternRouter::new(vec![TopicRule {
            pattern: "[".into(),
            pattern_type: PatternType::Glob,
            handler_id: "h".into(),
        }]);
        assert!(result.is_err());
    }

    #[test]
    fn invalid_regex_returns_error() {
        let result = TopicPatternRouter::new(vec![TopicRule {
            pattern: "(?".into(),
            pattern_type: PatternType::Regex,
            handler_id: "h".into(),
        }]);
        assert!(result.is_err());
    }
}
