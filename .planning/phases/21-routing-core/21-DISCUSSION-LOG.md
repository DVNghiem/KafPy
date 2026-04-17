# Phase 21: Routing Core - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-04-17
**Phase:** 21-routing-core
**Areas discussed:** TopicPattern matching, Header access API, RoutingRule config API, Default handler behavior

---

## TopicPattern matching — glob vs regex

| Option | Description | Selected |
|--------|-------------|----------|
| Glob only | orders.*, payments.# — simpler, no external deps, covers 95% of Kafka routing use cases | |
| Regex only | Full regex power, but adds regex crate dep and DoS attack surface | |
| Both — glob and regex | RoutingRule has 'pattern_type: Glob \| Regex' field, user picks per rule. Most flexible but more complex. | ✓ |

**User's choice:** Both — glob and regex (user picks per rule via `pattern_type` field)

---

## Header access API — borrowed vs owned

| Option | Description | Selected |
|--------|-------------|----------|
| Zero-copy borrowed headers | RoutingContext borrows rdkafka Headers directly — no allocation, no copies. Requires RoutingContext to hold a message reference (not OwnedMessage). | ✓ |
| Owned HashMap | Copy headers into a owned HashMap — simpler API, consistent with OwnedMessage approach, slight allocation overhead. RoutingContext can be fully owned/self-contained. | |

**User's choice:** Zero-copy borrowed headers ("follow big tech")

---

## RoutingRule config API

| Option | Description | Selected |
|--------|-------------|----------|
| ConsumerConfigBuilder chain | config.routing_rule(pattern, header, key).to_handler("handler-id").priority(1) — consistent with topics() API | ✓ |
| Separate RoutingRuleBuilder | RoutingRule::builder().pattern(...).header(...).key(...).build() — more explicit, more verbose | |
| RoutingRule struct with From impl | From<[(&str, &str, HandlerId)]> for RoutingRule — minimal API, uses Rust idioms | |

**User's choice:** ConsumerConfigBuilder chain ("follow big tech")

---

## Default handler behavior

| Option | Description | Selected |
|--------|-------------|----------|
| Error — no default handler configured | Return Reject(Reason::NoMatchingHandler) when Defer reaches end of chain with no default. Fail explicitly. | |
| Silently drop the message | At end of chain with no default, silently drop (Drop is implicit). Simple, but may hide misconfiguration. | |
| Register default handler explicitly | RoutingChain has a required default_handler: HandlerId field. If Defer reaches end and no default set, panic in debug, error in release. | ✓ |

**User's choice:** Register default handler explicitly ("follow big tech")

---

## Claude's Discretion

All 4 decisions were user-driven. No areas delegated to Claude discretion.

## Deferred Ideas

- Schema-based routing (Avro/Protobuf) — deferred to post-v1.5
- Content-based routing (payload parsing) — Python fallback only per v1.5 design
- Multi-handler fan-out — single handler per message, deferred
