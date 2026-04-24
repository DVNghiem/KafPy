# Phase 1: architecture document - Context

**Gathered:** 2026-04-24
**Status:** Ready for planning

<domain>
## Phase Boundary

Create a comprehensive architecture document for KafPy — a PyO3 Rust native extension providing a high-performance Kafka producer/consumer framework with idiomatic Python API.

</domain>

<decisions>
## Implementation Decisions

### Document Format
- **D-01:** Markdown with Mermaid diagrams for visual documentation
- **D-02:** MkDocs as the documentation framework

### Depth Level
- **D-03:** Detailed component specs for all modules in `src/`
- **D-04:** Each module to include: responsibilities, public API, key data structures, relationships

### Scope
- **D-05:** All modules in `src/` — comprehensive coverage (consumer/, dispatcher/, worker_pool/, routing/, observability/, offset/, shutdown/, retry/, dlq/, failure/, python/, runtime/, error.rs, config.rs, etc.)

### Audience
- **D-06:** All of the above: new contributors, API users, and maintainers
- **D-07:** Balanced coverage serving onboarding, public API contracts, and extension patterns

### Agent's Discretion
- Diagrams, layout structure, and navigation organization — agent can decide best approach

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Existing Documentation
- `.planning/codebase/ARCHITECTURE.md` — existing architecture analysis (outdated, to be refreshed)
- `.planning/codebase/STRUCTURE.md` — codebase structure and file purposes
- `.planning/codebase/CONVENTIONS.md` — Rust naming and PyO3 patterns
- `.planning/codebase/STACK.md` — technology stack and dependencies
- `.planning/PROJECT.md` — project vision, key decisions, validated requirements
- `.planning/REQUIREMENTS.md` — v2 requirements including ARCH-01, ARCH-02, ARCH-03

### Source Code
- `src/` — all Rust source modules
- `kafpy/__init__.py` — Python public API

[No external specs — requirements fully captured in decisions above]

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- Existing `.planning/codebase/` maps: ARCHITECTURE.md, STRUCTURE.md, CONVENTIONS.md, STACK.md, INTEGRATIONS.md, CONCERNS.md, TESTING.md
- Existing `src/` module structure provides the inventory for comprehensive documentation

### Established Patterns
- PyO3 module pattern: `#[pymodule] fn _kafpy` in lib.rs
- Async pattern: `future_into_py` for Python async integration
- Error pattern: `thiserror` enums converted to `PyErr` at boundary

### Integration Points
- Documentation should map to existing module structure under `src/`
- Python API lives in `kafpy/__init__.py` (public surface)
- PyO3 boundary is the Rust/Python interface surface

</code_context>

<specifics>
## Specific Ideas

- New contributors need onboarding path and coding conventions (reference CONVENTIONS.md)
- API users need clear public API contracts (PascalCase classes, snake_case methods)
- Maintainers need design rationale and extension patterns (reference PROJECT.md Key Decisions)
- Mermaid diagrams for: module hierarchy, message flow (Kafka→Python→offset commit), component relationships

[No specific external references cited during discussion]

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 01-architecture-document*
*Context gathered: 2026-04-24*
