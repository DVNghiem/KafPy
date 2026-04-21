//! header — HeaderRouter for header-based routing.

use crate::routing::context::RoutingContext;
use crate::routing::decision::RoutingDecision;
use crate::routing::router::Router;
use glob::Pattern;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct HeaderRule {
    pub key: String,
    pub value_pattern: Option<String>,
    pub handler_id: crate::routing::HandlerId,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct HeaderRouter {
    rules: Arc<[HeaderRule]>,
}

impl HeaderRouter {
    #[allow(dead_code)]
    pub fn new(rules: Vec<HeaderRule>) -> Self {
        Self {
            rules: rules.into(),
        }
    }

    #[allow(dead_code)]
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }
}

impl Router for HeaderRouter {
    fn route(&self, ctx: &RoutingContext) -> RoutingDecision {
        for rule in self.rules.iter() {
            if !ctx.has_header(&rule.key) {
                continue;
            }
            let Some(ref value_pattern) = rule.value_pattern else {
                return RoutingDecision::Route(rule.handler_id.clone());
            };
            if let Some(value) = ctx.get_header(&rule.key) {
                if let Ok(pattern) = Pattern::new(value_pattern) {
                    if let Some(value_str) = std::str::from_utf8(value).ok() {
                        if pattern.matches(value_str) {
                            return RoutingDecision::Route(rule.handler_id.clone());
                        }
                    }
                }
            }
        }
        RoutingDecision::Defer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_with_headers(headers: &[(&str, Option<&[u8]>)]) -> RoutingContext<'static> {
        let owned: Vec<(String, Option<Vec<u8>>)> = headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.map(|b| b.to_vec())))
            .collect();
        RoutingContext {
            topic: "test",
            partition: 0,
            offset: 0,
            key: None,
            payload: None,
            headers: Box::leak(owned.into_boxed_slice()),
        }
    }

    fn rule(key: &str, value_pattern: Option<&str>, handler: &str) -> HeaderRule {
        HeaderRule {
            key: key.into(),
            value_pattern: value_pattern.map(|s| s.into()),
            handler_id: handler.into(),
        }
    }

    #[test]
    fn presence_match() {
        let router = HeaderRouter::new(vec![rule("x-trace", None, "handler1")]);
        let ctx = ctx_with_headers(&[("x-trace", Some(b"abc"))]);
        assert!(matches!(router.route(&ctx), RoutingDecision::Route(id) if id.as_str() == "handler1"));
    }

    #[test]
    fn presence_no_match_when_missing() {
        let router = HeaderRouter::new(vec![rule("x-trace", None, "handler1")]);
        let ctx = ctx_with_headers(&[("other", Some(b"abc"))]);
        assert!(matches!(router.route(&ctx), RoutingDecision::Defer));
    }

    #[test]
    fn value_pattern_match() {
        let router = HeaderRouter::new(vec![rule(
            "content-type",
            Some("application/json*"),
            "handler1",
        )]);
        let ctx = ctx_with_headers(&[("content-type", Some(b"application/json; charset=utf-8"))]);
        assert!(matches!(router.route(&ctx), RoutingDecision::Route(id) if id.as_str() == "handler1"));
    }

    #[test]
    fn value_pattern_no_match() {
        let router = HeaderRouter::new(vec![rule(
            "content-type",
            Some("application/json*"),
            "handler1",
        )]);
        let ctx = ctx_with_headers(&[("content-type", Some(b"text/plain"))]);
        assert!(matches!(router.route(&ctx), RoutingDecision::Defer));
    }

    #[test]
    fn null_header_value_defers() {
        let router = HeaderRouter::new(vec![rule("x-trace", Some("*"), "handler1")]);
        let ctx = ctx_with_headers(&[("x-trace", None)]);
        assert!(matches!(router.route(&ctx), RoutingDecision::Defer));
    }

    #[test]
    fn first_match_wins() {
        let router = HeaderRouter::new(vec![
            rule("x-a", None, "handler1"),
            rule("x-a", None, "handler2"),
        ]);
        let ctx = ctx_with_headers(&[("x-a", Some(b"1"))]);
        assert!(matches!(router.route(&ctx), RoutingDecision::Route(id) if id.as_str() == "handler1"));
    }
}
