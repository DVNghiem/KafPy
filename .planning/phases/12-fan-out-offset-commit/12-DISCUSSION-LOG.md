# Phase 12: Fan-Out Offset Commit - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-04-29
**Phase:** 12-fan-out-offset-commit
**Areas discussed:** Offset commit trigger, Per-sink timeout, Error classification

---

## Offset Commit Trigger

| Option | Description | Selected |
|--------|-------------|----------|
| All must succeed | Offset commits only if every branch completes successfully | |
| All must finish (recommended) | Offset commits when all branches complete, regardless of outcome. Failures logged but don't block commit. | ✓ |
| Let me think more | User wants more time before deciding | |

**User's choice:** All must finish (recommended)
**Notes:** Followed big tech recommendation. Aligns with Phase 11's non-blocking partial success philosophy. DLQ handles failures separately. Big tech pattern (AWS Step Functions, Temporal, Google Pub/Sub fan-out).

---

## Per-Sink Timeout

| Option | Description | Selected |
|--------|-------------|----------|
| Per-sink (recommended) | Each branch gets its own timeout scope via invoke_mode_with_timeout | ✓ |
| Shared timeout | Single shared deadline for entire fan-out operation | |
| You decide | Defer to Claude's judgment | |

**User's choice:** Per-sink (recommended)
**Notes:** Independent timeout scopes prevent one slow branch from cancelling faster branches.

---

## Error Classification (DLQ Routing)

| Option | Description | Selected |
|--------|-------------|----------|
| 3 categories (recommended) | Timeout, HandlerError, SystemError — matched to existing BranchResult enum | ✓ |
| Exception-based | Tag errors with exception type (ValueError, TypeError, etc.) | |
| Not in scope | Defer error classification to future phase | |

**User's choice:** 3 categories (recommended)
**Notes:** Leverages existing BranchResult::Error { reason, exception } and BranchResult::Timeout { timeout_ms }. Already has the structure needed.

---

## Deferred Ideas

None — discussion stayed within phase scope.
