//! RoutingChain — chains routers in precedence order: pattern → header → key → python → default.
//!
//! Per D-04: an explicit `default_handler: HandlerId` is REQUIRED.
//! If the chain reaches the end with Defer: panic in debug, Reject(NoDefaultHandler) in release.

use crate::routing::context::HandlerId;
use crate::routing::context::RoutingContext;
use crate::routing::decision::{RejectReason, RoutingDecision};
use crate::routing::router::Router;
use std::sync::Arc;

/// The routing chain wires together the router types in precedence order.
///
/// Routers are stored as `Arc<dyn Router>` to allow heterogeneous types in the chain.
/// Phase 22 (Python Integration) will insert `PythonRouter` at the `python_slot` position.
///
/// # Precedence order
/// 1. TopicPatternRouter (pattern)
/// 2. HeaderRouter (header)
/// 3. KeyRouter (key)
/// 4. Python slot (python_slot) — filled in Phase 22, defaults to None
/// 5. Default handler — always required; returned if all routers return Defer
///
/// # Example
/// ```ignore
/// let chain = RoutingChain::new()
///     .with_topic_router(topic_router)
///     .with_header_router(header_router)
///     .with_key_router(key_router)
///     .with_default_handler("default-handler".into());
/// ```
pub struct RoutingChain {
    topic_router: Option<Arc<dyn Router>>,
    header_router: Option<Arc<dyn Router>>,
    key_router: Option<Arc<dyn Router>>,
    // Phase 22: Python router slot — filled via with_python_router()
    python_router: Option<Arc<dyn Router>>,
    default_handler: HandlerId,
}

impl std::fmt::Debug for RoutingChain {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RoutingChain")
            .field("topic_router", &self.topic_router.as_ref().map(|_| "Some(..)"))
            .field("header_router", &self.header_router.as_ref().map(|_| "Some(..)"))
            .field("key_router", &self.key_router.as_ref().map(|_| "Some(..)"))
            .field("python_router", &self.python_router.as_ref().map(|_| "Some(..)"))
            .field("default_handler", &self.default_handler)
            .finish()
    }
}

impl RoutingChain {
    /// Creates a new empty chain. A default handler MUST be set before use.
    pub fn new() -> Self {
        Self {
            topic_router: None,
            header_router: None,
            key_router: None,
            python_router: None,
            // safety: caller MUST call with_default_handler()
            default_handler: HandlerId::new(),
        }
    }

    /// Sets the topic pattern router (first in chain).
    pub fn with_topic_router(mut self, router: impl Router + 'static) -> Self {
        self.topic_router = Some(Arc::new(router));
        self
    }

    /// Sets the header router (second in chain).
    pub fn with_header_router(mut self, router: impl Router + 'static) -> Self {
        self.header_router = Some(Arc::new(router));
        self
    }

    /// Sets the key router (third in chain).
    pub fn with_key_router(mut self, router: impl Router + 'static) -> Self {
        self.key_router = Some(Arc::new(router));
        self
    }

    /// Sets the Python router (fourth in chain, Phase 22).
    /// If not called, the slot is skipped.
    pub fn with_python_router(mut self, router: impl Router + 'static) -> Self {
        self.python_router = Some(Arc::new(router));
        self
    }

    /// Sets the default handler (REQUIRED — panic if not set before calling route()).
    ///
    /// Per D-04: every RoutingChain must have an explicit default handler.
    pub fn with_default_handler(mut self, handler: HandlerId) -> Self {
        self.default_handler = handler;
        self
    }

    /// Returns the configured default handler ID.
    pub fn default_handler(&self) -> &HandlerId {
        &self.default_handler
    }

    /// Evaluates the routing context through the chain in precedence order.
    ///
    /// Returns the first non-Defer decision, or the default handler if all return Defer.
    ///
    /// # Panic
    /// Panics in debug mode if no default handler was configured.
    /// In release mode, returns `Reject(NoDefaultHandler)`.
    pub fn route(&self, ctx: &RoutingContext) -> RoutingDecision {
        // Topic pattern router
        if let Some(ref router) = self.topic_router {
            let decision = router.route(ctx);
            if !decision.is_defer() {
                return decision;
            }
        }

        // Header router
        if let Some(ref router) = self.header_router {
            let decision = router.route(ctx);
            if !decision.is_defer() {
                return decision;
            }
        }

        // Key router
        if let Some(ref router) = self.key_router {
            let decision = router.route(ctx);
            if !decision.is_defer() {
                return decision;
            }
        }

        // Python router (Phase 22 — slot present but not filled in Phase 21)
        if let Some(ref router) = self.python_router {
            let decision = router.route(ctx);
            if !decision.is_defer() {
                return decision;
            }
        }

        // Default handler — required
        // In debug: panic if default_handler was not configured (empty string sentinel)
        // In release: return Reject(NoDefaultHandler) to avoid panicking in production
        if self.default_handler.is_empty() {
            debug_assert!(
                false,
                "RoutingChain::route() called without a default handler configured"
            );
            return RoutingDecision::Reject(RejectReason::NoDefaultHandler);
        }

        RoutingDecision::Route(self.default_handler.clone())
    }
}

impl Default for RoutingChain {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::context::RoutingContext;
    use crate::routing::decision::RoutingDecision;
    use crate::routing::router::Router;

    struct MockRouter {
        decision: RoutingDecision,
    }
    impl Router for MockRouter {
        fn route(&self, _ctx: &RoutingContext) -> RoutingDecision {
            self.decision.clone()
        }
    }

    fn ctx() -> RoutingContext<'static> {
        RoutingContext {
            topic: "test",
            partition: 0,
            offset: 0,
            key: None,
            payload: None,
            headers: &[],
        }
    }

    #[test]
    fn first_non_defer_wins() {
        let chain = RoutingChain::new()
            .with_topic_router(MockRouter { decision: RoutingDecision::Drop })
            .with_header_router(MockRouter { decision: RoutingDecision::Route("header-handler".into()) })
            .with_key_router(MockRouter { decision: RoutingDecision::Route("key-handler".into()) })
            .with_default_handler("default".into());

        // TopicRouter returns Drop — non-Defer, chain stops
        assert!(matches!(chain.route(&ctx()), RoutingDecision::Drop));
    }

    #[test]
    fn defer_continues_chain() {
        let chain = RoutingChain::new()
            .with_topic_router(MockRouter { decision: RoutingDecision::Defer })
            .with_header_router(MockRouter { decision: RoutingDecision::Route("header-handler".into()) })
            .with_key_router(MockRouter { decision: RoutingDecision::Route("key-handler".into()) })
            .with_default_handler("default".into());

        assert!(matches!(chain.route(&ctx()), RoutingDecision::Route(id) if id == "header-handler"));
    }

    #[test]
    fn all_defer_returns_default() {
        let chain = RoutingChain::new()
            .with_topic_router(MockRouter { decision: RoutingDecision::Defer })
            .with_header_router(MockRouter { decision: RoutingDecision::Defer })
            .with_key_router(MockRouter { decision: RoutingDecision::Defer })
            .with_default_handler("my-default".into());

        assert!(matches!(chain.route(&ctx()), RoutingDecision::Route(id) if id == "my-default"));
    }

    #[test]
    fn skips_none_routers() {
        // No topic or header router, only key + default
        let chain = RoutingChain::new()
            .with_key_router(MockRouter { decision: RoutingDecision::Route("key-handler".into()) })
            .with_default_handler("default".into());

        assert!(matches!(chain.route(&ctx()), RoutingDecision::Route(id) if id == "key-handler"));
    }

    #[test]
    fn order_pattern_header_key() {
        // Verify precedence: pattern → header → key
        use std::sync::Mutex;
        let calls: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let calls2 = Arc::clone(&calls);

        struct TracingRouter(&'static str, RoutingDecision, Arc<Mutex<Vec<String>>>);
        impl Router for TracingRouter {
            fn route(&self, _ctx: &RoutingContext) -> RoutingDecision {
                self.2.lock().unwrap().push(self.0.into());
                self.1.clone()
            }
        }

        let chain = RoutingChain::new()
            .with_topic_router(TracingRouter("pattern", RoutingDecision::Defer, Arc::clone(&calls2)))
            .with_header_router(TracingRouter("header", RoutingDecision::Defer, Arc::clone(&calls2)))
            .with_key_router(TracingRouter("key", RoutingDecision::Route("final".into()), Arc::clone(&calls2)))
            .with_default_handler("default".into());

        chain.route(&ctx());
        let order: Vec<String> = Arc::try_unwrap(calls2).unwrap().into_inner().unwrap();
        let order: Vec<_> = order.iter().map(|s| s.as_str()).collect();
        assert_eq!(order, vec!["pattern", "header", "key"]);
    }
}
