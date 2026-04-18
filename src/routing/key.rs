//! key — KeyRouter for key-based routing.

use crate::routing::context::RoutingContext;
use crate::routing::decision::RoutingDecision;
use crate::routing::router::Router;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyMatchMode {
    Exact,
    Prefix,
    PrefixStr,
    ExactStr,
}

#[derive(Debug, Clone)]
pub struct KeyRule {
    pub match_mode: KeyMatchMode,
    pub pattern: Vec<u8>,
    pub handler_id: crate::routing::HandlerId,
}

impl KeyRule {
    pub fn exact_bytes(pattern: impl Into<Vec<u8>>, handler_id: crate::routing::HandlerId) -> Self {
        Self {
            match_mode: KeyMatchMode::Exact,
            pattern: pattern.into(),
            handler_id,
        }
    }

    pub fn prefix_bytes(
        pattern: impl Into<Vec<u8>>,
        handler_id: crate::routing::HandlerId,
    ) -> Self {
        Self {
            match_mode: KeyMatchMode::Prefix,
            pattern: pattern.into(),
            handler_id,
        }
    }

    pub fn exact_str(pattern: impl Into<String>, handler_id: crate::routing::HandlerId) -> Self {
        Self {
            match_mode: KeyMatchMode::ExactStr,
            pattern: pattern.into().into_bytes(),
            handler_id,
        }
    }

    pub fn prefix_str(pattern: impl Into<String>, handler_id: crate::routing::HandlerId) -> Self {
        Self {
            match_mode: KeyMatchMode::PrefixStr,
            pattern: pattern.into().into_bytes(),
            handler_id,
        }
    }
}

#[derive(Debug, Clone)]
pub struct KeyRouter {
    rules: Arc<[KeyRule]>,
}

impl KeyRouter {
    pub fn new(rules: Vec<KeyRule>) -> Self {
        Self {
            rules: rules.into(),
        }
    }

    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }
}

impl Router for KeyRouter {
    fn route(&self, ctx: &RoutingContext) -> RoutingDecision {
        let Some(key) = ctx.key else {
            return RoutingDecision::Defer;
        };

        for rule in self.rules.iter() {
            let matches = match rule.match_mode {
                KeyMatchMode::Exact => key == rule.pattern.as_slice(),
                KeyMatchMode::Prefix => key.starts_with(&rule.pattern),
                KeyMatchMode::ExactStr => std::str::from_utf8(key)
                    .ok()
                    .is_some_and(|s| s.as_bytes() == rule.pattern.as_slice()),
                KeyMatchMode::PrefixStr => std::str::from_utf8(key).ok().is_some_and(|s| {
                    let pat = std::str::from_utf8(&rule.pattern).unwrap_or("");
                    s.starts_with(pat)
                }),
            };
            if matches {
                return RoutingDecision::Route(rule.handler_id.clone());
            }
        }
        RoutingDecision::Defer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(key: Option<&[u8]>) -> RoutingContext<'_> {
        RoutingContext {
            topic: "test",
            partition: 0,
            offset: 0,
            key,
            payload: None,
            headers: &[],
        }
    }

    #[test]
    fn exact_bytes_match() {
        let router = KeyRouter::new(vec![KeyRule::exact_bytes(b"order-123", "h".into())]);
        assert!(
            matches!(router.route(&ctx(Some(b"order-123"))), RoutingDecision::Route(id) if id == "h")
        );
    }

    #[test]
    fn exact_bytes_no_match() {
        let router = KeyRouter::new(vec![KeyRule::exact_bytes(b"order-123", "h".into())]);
        assert!(matches!(
            router.route(&ctx(Some(b"order-456"))),
            RoutingDecision::Defer
        ));
    }

    #[test]
    fn prefix_bytes_match() {
        let router = KeyRouter::new(vec![KeyRule::prefix_bytes(b"order-", "h".into())]);
        assert!(
            matches!(router.route(&ctx(Some(b"order-123"))), RoutingDecision::Route(id) if id == "h")
        );
        assert!(
            matches!(router.route(&ctx(Some(b"order-"))), RoutingDecision::Route(id) if id == "h")
        );
    }

    #[test]
    fn prefix_bytes_no_match() {
        let router = KeyRouter::new(vec![KeyRule::prefix_bytes(b"order-", "h".into())]);
        assert!(matches!(
            router.route(&ctx(Some(b"event-123"))),
            RoutingDecision::Defer
        ));
    }

    #[test]
    fn exact_str_match() {
        let router = KeyRouter::new(vec![KeyRule::exact_str("order-123", "h".into())]);
        assert!(
            matches!(router.route(&ctx(Some(b"order-123"))), RoutingDecision::Route(id) if id == "h")
        );
    }

    #[test]
    fn prefix_str_match() {
        let router = KeyRouter::new(vec![KeyRule::prefix_str("order-", "h".into())]);
        assert!(
            matches!(router.route(&ctx(Some(b"order-123"))), RoutingDecision::Route(id) if id == "h")
        );
    }

    #[test]
    fn no_key_defer() {
        let router = KeyRouter::new(vec![KeyRule::exact_bytes(b"order-123", "h".into())]);
        assert!(matches!(router.route(&ctx(None)), RoutingDecision::Defer));
    }

    #[test]
    fn invalid_utf8_key_defers_str_matching() {
        let router = KeyRouter::new(vec![KeyRule::prefix_bytes(b"\xff", "h".into())]);
        assert!(
            matches!(router.route(&ctx(Some(&[0xff]))), RoutingDecision::Route(id) if id == "h")
        );
    }
}
