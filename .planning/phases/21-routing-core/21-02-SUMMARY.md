# Phase 21 Plan 02 Summary: Three Concrete Router Types

**Plan:** 21-02
**Phase:** 21-routing-core
**Completed:** 2026-04-17

## Objective

Implement three concrete Router types: TopicPatternRouter (glob/regex), HeaderRouter (header key presence or value match), KeyRouter (key prefix or exact match).

## What Was Built

### TopicPatternRouter (`src/routing/topic_pattern.rs`)
- `PatternType` enum: `Glob` | `Regex`
- `TopicRule` struct: `pattern: String`, `pattern_type: PatternType`, `handler_id: HandlerId`
- Compiles glob/regex at construction, returns `PatternError` on invalid patterns
- Returns `Route(handler_id)` on first match, `Defer` on no match

### HeaderRouter (`src/routing/header.rs`)
- `HeaderRule` struct: `key: String`, `value_pattern: Option<String>`, `handler_id: HandlerId`
- Checks header key presence via `ctx.has_header()`
- Optional glob value pattern via `ctx.get_header()`
- Returns `Route` on first match, `Defer` otherwise

### KeyRouter (`src/routing/key.rs`)
- `KeyMatchMode` enum: `Exact` | `Prefix` | `PrefixStr` | `ExactStr`
- `KeyRule` with constructors: `exact_bytes`, `prefix_bytes`, `exact_str`, `prefix_str`
- Binary and UTF-8 string matching
- Returns `Route` on first match, `Defer` when no key present

### Supporting Infrastructure
- `Router` trait: `fn route(&self, ctx: &RoutingContext) -> RoutingDecision`
- `RoutingDecision` enum: `Route(HandlerId)`, `Drop`, `Reject(RejectReason)`, `Defer`
- `RejectReason` enum: `NoMatch`, `NoDefaultHandler`, `Explicit(String)`
- `HandlerId` type alias: `String`
- `RoutingContext` zero-copy struct borrowing from `OwnedMessage`

## Commits

- `672edc6`: feat(21-02): implement three concrete Router types

## Verification

- `cargo check --lib` passes (0 errors)
- PyO3 linking errors are pre-existing (not introduced by this plan)

## Status

COMPLETE - All three router types implemented and compile successfully.