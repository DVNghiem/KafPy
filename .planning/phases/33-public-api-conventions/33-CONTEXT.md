# Phase 33: Public API Conventions - Context

**Gathered:** 2026-04-20
**Status:** Ready for planning

<domain>
## Phase Boundary

Establish PascalCase naming, snake_case methods, type hints, private Rust internals, and `__all__` definitions across all public Python modules.

This is the FOUNDATION for all subsequent phases — what gets marked public here shapes every other phase.

</domain>

<decisions>
## Implementation Decisions

### __all__ Strategy
- **D-01:** `__all__` in every public Python module — not just the package `__init__.py`. Each module is self-documenting and explicitly controls its public surface.

### PyO3 Rust pub Boundary
- **D-02:** Principle of least exposure — mark Rust items `pub` ONLY when Python MUST access them. Everything else is `pub(crate)` or private. This keeps the PyO3 boundary tight and intentional.

### kafpy init Exports
- **D-03:** Full surface re-export at `import kafpy` — all public types from submodules are re-exported at the package level for discoverability.

### Naming Conventions
- **D-04:** All public Python classes use PascalCase (e.g., `ConsumerConfig`, `KafkaMessage`)
- **D-05:** All public methods use snake_case with type hints on all signatures
- **D-06:** No underscore-prefixed names in `__all__`, no raw Rust struct names exposed

### Rust Internals
- **D-07:** Rust internals are private by default — only explicitly marked `pub` items are accessible from Python

### Instance-Based Configuration
- **D-08:** No global mutable state — all runtime state held in instance objects

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

- `.planning/REQUIREMENTS.md` — API-01 through API-05 (naming, type hints, `__all__`, Rust privacy)
- `.planning/ROADMAP.md` Phase 33 — full phase goal and success criteria

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- Existing `kafpy/__init__.py` — will need to be restructured to export full surface
- Existing Rust `pub` items in `src/lib.rs` and `src/pyconsumer.rs` — will need audit for least-exposure

### Integration Points
- Phase 34 (Configuration) depends on this phase's config class structure
- Phase 35 (Handlers) depends on `KafkaMessage`, `HandlerContext`, `HandlerResult` being public
- Phase 36 (Errors) depends on exception hierarchy being established here

</code_context>

<specifics>
## Specific Ideas

No external specs — decisions captured above from requirements analysis.

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 33-public-api-conventions*
*Context gathered: 2026-04-20*
