# Routing

KafPy runtime supports an internal precedence-based routing chain in Rust:

1. `TopicPatternRouter`
2. `HeaderRouter`
3. `KeyRouter`
4. `PythonRouter` (optional callback)
5. Default fallback handler

## Important status

- The Python package currently does **not** expose a stable public API like `RoutingConfig`, `TopicPatternRouter`, `HeaderRouter`, or `KeyRouter` classes for end-user construction.
- Routing chain construction happens inside the Rust runtime (`RoutingChain::from_rules(...)`) when routing rules are provided to the internal Rust config layer.
- If no routing chain is configured, dispatch is by registered handler key/topic in normal consumer flow.

## Current behavior summary

| Decision | Meaning |
|---|---|
| `Route(handler_id)` | Dispatch to a specific handler |
| `Drop` | Drop message from routing path |
| `Reject(reason)` | Reject from routing path (handled through dispatcher failure path) |
| `Defer` | Continue to next router; if all defer, fallback handler is used |

## Python callback router

When enabled, Python routing callback output is parsed as:

- `route:<handler_id>`
- `drop`
- `reject:<reason>`
- `defer`

Callback failures are converted to reject decisions with explicit reason text.