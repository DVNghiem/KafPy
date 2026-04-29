---
phase: "08"
slug: async-timeout
validation: "01"
date: "2026-04-29"
status: "in_progress"
---

# Phase 08: Async Timeout - Validation Strategy

## Validation Architecture

### Scope
This phase implements async handler timeout with DLQ metadata and Prometheus metrics.

### Key Verification Dimensions

| # | Dimension | What to Verify | How to Verify |
|---|-----------|----------------|---------------|
| 1 | Requirement Coverage | All TMOUT-01/02/03 requirements addressed in plans | grep for TMOUT-0X in plan frontmatter |
| 2 | Task Completeness | All tasks have Files/Action/Verify/Done | scan plan tasks |
| 3 | Dependency Correctness | Wave 1 before Wave 2, no cycles | review frontmatter waves |
| 4 | Key Links | TimeoutInfo → DLQ metadata flow exists | grep pattern verification |
| 5 | Scope Sanity | Plans stay within phase goal | review plan objectives |
| 6 | Verification Derivation | must_haves map to truths in codebase | cross-check |
| 7 | Context Compliance | D-01/D-02/D-03 decisions implemented | grep verification |
| 8 | Nyquist Coverage | Validated requirements in VALIDATION.md | check coverage |
| 9 | Plans Follow Deep Work Rules | Tasks have read_first, acceptance_criteria, concrete actions | scan tasks |
| 10 | Security Gate | No security-sensitive changes in this phase | review files |
| 11 | Research Resolution | Open questions resolved in RESEARCH.md | grep "(RESOLVED)" |

### Truths (Goal-Backward Verification)

From ROADMAP.md Phase 8 goal:
1. Async handlers can be aborted after configured timeout
2. Timeout metadata appears in DLQ envelope
3. Timeout metric appears in Prometheus

### must_haves

| Truth | Artifact | Key Link |
|-------|----------|----------|
| `timeout_duration` in DLQ | `DlqMetadata.timeout_duration: Option<u64>` | set in handle_execution_failure |
| `last_processed_offset` in DLQ | `DlqMetadata.last_processed_offset: Option<u64>` | set to original_offset - 1 |
| Timeout counter metric | `TimeoutMetrics::record_timeout(topic, handler)` | counter kafpy.handler.timeout_total |
| Python API wired | `add_handler(timeout_ms=X)` flow | invoke_mode_with_timeout |

### Test Approach

1. **cargo check** — compilation
2. **cargo test --lib** — unit tests for new types (TimeoutInfo, ExecutionResult::Timeout)
3. **grep verification** — grep for field names and method names in expected locations

---
*Phase: 08-async-timeout*
*Validation strategy: 2026-04-29*