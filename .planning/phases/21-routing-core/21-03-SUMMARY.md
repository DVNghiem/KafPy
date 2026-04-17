# Phase 21 Plan 03 Summary: RoutingChain and RoutingRule Configuration

**Plan:** 21-03
**Phase:** 21-routing-core
**Completed:** 2026-04-17
**Commits:** ebba406, aca5286, 0e0b7a4

## One-liner

RoutingChain evaluates routers in precedence order with explicit default handler; RoutingRuleBuilder config API integrated into ConsumerConfig.

## Tasks Completed

| # | Name | Commit | Files |
|---|------|--------|-------|
| 1 | Implement RoutingChain | ebba406 | src/routing/chain.rs |
| 2 | Create RoutingRule/RoutingRuleBuilder | aca5286 | src/routing/config.rs |
| 3 | Integrate into ConsumerConfig | 0e0b7a4 | src/consumer/config.rs, src/routing/mod.rs, src/lib.rs |

## What Was Built

### RoutingChain (src/routing/chain.rs)
- `pub struct RoutingChain` with topic/header/key/python router slots as `Option<Arc<dyn Router>>`
- Builder methods: `with_topic_router`, `with_header_router`, `with_key_router`, `with_python_router`, `with_default_handler`
- `route(&self, ctx: &RoutingContext) -> RoutingDecision` evaluates in precedence order: pattern → header → key → python → default
- Returns first non-Defer decision; falls back to default_handler if all return Defer
- Per D-04: panic in debug / `Reject(NoDefaultHandler)` in release if no default configured
- Manual `Debug` impl (dyn Router doesn't implement Debug)
- 5 tests: first_non_defer_wins, defer_continues_chain, all_defer_returns_default, skips_none_routers, order_pattern_header_key

### RoutingRuleBuilder (src/routing/config.rs)
- `pub struct RoutingRuleBuilder` with chain API: `topic_pattern(PatternType)`, `header_key(Option<&str>)`, `key_match(KeyMatchMode, Vec<u8>)`, `to_handler()`, `priority()`
- `build() -> Result<RoutingRule, RoutingRuleBuildError>` — `MissingHandler` error if `to_handler` not called
- `pub struct RoutingRule` with `priority`, `topic_rule: Option<TopicRule>`, `header_rule: Option<HeaderRule>`, `key_rule: Option<KeyRule>`, `handler_id`
- `PatternType` (Glob, Regex) re-exported from routing module
- 5 tests: topic_rule_build, header_rule_build, key_rule_build, combined_rules, missing_handler_error

### ConsumerConfig Integration
- `ConsumerConfig.routing_rules: Vec<RoutingRule>`
- `ConsumerConfigBuilder.routing_rules: Vec<RoutingRule>` + `routing_rule(RoutingRule) -> Self`
- `lib.rs` re-exports: `RoutingRule`, `RoutingRuleBuilder`, `PatternType`, `RoutingRuleBuildError`
- `routing/mod.rs` re-exports `PatternType` from topic_pattern

## Key Decisions

1. **RoutingChain Debug impl**: Manual impl since `dyn Router` doesn't implement `Debug`; slots shown as `Option<"Some(..)">`
2. **TracingRouter in tests**: Uses `Arc<Mutex<Vec<String>>>` + `Arc::try_unwrap` instead of raw pointer for `Send + Sync` compliance
3. **PatternType re-export**: Re-exported from both `topic_pattern` → `config` → `routing` → `lib.rs` to make it accessible via `routing::config::PatternType`

## Verification

- `cargo check --lib` passes (Finished in 0.49s)
- `cargo check --tests` passes (Finished in 0.46s)
- Note: `cargo test --lib` linking fails due to Python dev headers unavailable at link time (cdylib + extension-module configuration). Test code compiles correctly.

## Files Created/Modified

| File | Change |
|------|--------|
| src/routing/chain.rs | Created (261 lines) |
| src/routing/config.rs | Created (220 lines) |
| src/consumer/config.rs | Modified (+9 lines: field, builder field, constructor, builder method, build pass-through) |
| src/routing/mod.rs | Modified (+1 re-export) |
| src/lib.rs | Modified (+1 re-export line) |
