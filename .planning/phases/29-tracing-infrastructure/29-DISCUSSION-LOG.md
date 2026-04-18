# Phase 29: Tracing Infrastructure - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-04-18
**Phase:** 29-tracing-infrastructure
**Areas discussed:** Tracing layer init, span wrapping, span context propagation, async patterns, ObservabilityConfig, naming convention

---

## Area 1: OBS Requirements Alignment

| Option | Description | Selected |
|--------|-------------|----------|
| OBS-11 through OBS-19 | All Phase 29 requirements fully specified in REQUIREMENTS.md | ✓ |

**User's choice:** All OBS requirements adopted as specified
**Notes:** No gray areas requiring discussion — all decisions pre-specified in roadmap requirements.

---

## Area 2: Async Span Pattern (OBS-17)

| Option | Description | Selected |
|--------|-------------|----------|
| #[instrument] macro | Cleaner, less boilerplate | |
| span.in_scope() | More explicit control for complex async control flow | ✓ |

**User's choice:** span.in_scope() (recommended — explicit scope easier to verify in worker_loop)
**Notes:** worker_loop has complex async/await patterns; explicit scope provides clearer control.

---

*Phase: 29-tracing-infrastructure*
*Discussion complete: 2026-04-18*
