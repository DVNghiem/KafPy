# Phase 21: Routing Core - Context

**Gathered:** 2026-04-17
**Status:** Ready for planning

<domain>
## Phase Boundary

Users can configure topic/header/key routing rules with zero-copy routing context, evaluated via a precedence chain in Rust. Routing lives entirely in Rust (Python routing deferred to Phase 22).

</domain>

<decisions>
## Implementation Decisions

### TopicPattern matching — D-01
- **Both glob and regex** — `RoutingRule` has a `pattern_type: PatternType` enum (`Glob | Regex`) field
- User picks per rule when configuring `RoutingRule`
- Glob covers 95% of cases; regex available for complex cases
- Pattern type determines how `TopicPatternRouter` evaluates the pattern string

### Header access API — D-02
- **Zero-copy borrowed headers** — `RoutingContext` holds a borrowed reference to the message headers
- `RoutingContext` borrows from `OwnedMessage` rather than owning a copy
- Requires `RoutingContext` to carry a lifetime parameter tied to the message
- No heap allocation during routing path; headers iterated in-place from rdkafka borrowed API

### RoutingRule config API — D-03
- **ConsumerConfigBuilder chain** — follows the same builder pattern as `topics()` API
- API shape: `config.routing_rule(pattern, header, key).to_handler("handler-id").priority(1)`
- Consistent with existing `ConsumerConfigBuilder` ergonomics
- `RoutingRuleBuilder` exposes `pattern_type(PatternType)` to switch between glob/regex

### Default handler behavior — D-04
- **Explicit default handler required** — `RoutingChain` has a required `default_handler: HandlerId` field
- If `Defer` reaches end of chain with no default configured: panic in debug, return `Reject(NoDefaultHandler)` in release
- Every `RoutingChain` must be constructed with an explicit default handler
- This prevents silent message drops from routing misconfiguration

### RoutingDecision::Route — D-05
- `Route(handler_id: HandlerId)` — handler ID is a string type alias `HandlerId = String`
- Enables `From<RoutingDecision>` for `Option<HandlerId>` — chain stops on `Route`, continues on `Defer`

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase Context
- `.planning/ROADMAP.md` — Phase 21 success criteria (8 items)
- `.planning/REQUIREMENTS.md` — ROUTER-01 through ROUTER-07, CONFIG-01, CONFIG-02
- `.planning/STATE.md` — Prior decisions: pattern→header→key→python→default precedence, zero-copy requirement

### Codebase References
- `src/dispatcher/mod.rs` — `ConsumerDispatcher` is where router will be injected (Phase 23); study `send_with_policy_and_signal` pattern
- `src/consumer/config.rs` — `ConsumerConfigBuilder` builder pattern to follow for RoutingRuleBuilder API
- `src/dlq/router.rs` — `DlqRouter` trait pattern (simple trait with `route` method) as reference for `Router` trait design
- `src/kafka_message.rs` — `KafkaMessage` headers field `Vec<(String, Option<Vec<u8>>)>` as reference for header structure

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `DlqRouter` trait (`src/dlq/router.rs`) — simple `route(&self, metadata: &T) -> R` trait pattern to follow for `Router` trait
- `ConsumerConfigBuilder` — builder chain pattern to replicate for `RoutingRuleBuilder`
- `TopicPartition` newtype pattern from `DlqRouter` — use for handler IDs

### Established Patterns
- Trait + implementation pair: `DlqRouter` trait + `DefaultDlqRouter` struct — replicate for `Router` trait + specific router implementations
- Builder pattern for config structs — consistent with `ConsumerConfigBuilder`
- Owned types with `Send + Sync` bounds — all types crossing thread boundaries must be `Send + Sync`

### Integration Points
- `ConsumerDispatcher` (`src/dispatcher/mod.rs`) will call `Router::route` before queue selection — DISPATCH-01 in Phase 23
- `RoutingChain` must be constructed and passed to `ConsumerDispatcher` at construction time
- `RoutingContext` must be constructible from `&OwnedMessage` with zero-copy header access

</code_context>

<specifics>
## Specific Ideas

No specific "I want it like X" references — open to standard Rust/Kafka routing approaches.

</specifics>

<deferred>
## Deferred Ideas

- Schema-based routing (Avro/Protobuf) — deferred to post-v1.5
- Content-based routing (payload parsing) — Python fallback only per v1.5 design
- Multi-handler fan-out — single handler per message, deferred

</deferred>

---

*Phase: 21-routing-core*
*Context gathered: 2026-04-17*
