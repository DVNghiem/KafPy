# Phase 07: Verify DUP Requirements - Context

**Gathered:** 2026-04-25
**Status:** Ready for planning
**Source:** Gap closure from v2.0 MILESTONE-AUDIT.md

<domain>
## Phase Boundary

Phase 07 (verify-dup) is a **gap closure verification phase**. It must verify that the 4 DUP requirements (DUP-01 through DUP-04) claimed complete in Phase 01 (01-extract-duplication) are actually present in the codebase, and produce a VERIFICATION.md that formally records the verification results.

If any requirement is not verified, the plan must include a fix task to complete the missing work.

</domain>

<decisions>
## Implementation Decisions

### DUP-01: handle_execution_failure()
- **Requirement:** Extract `handle_execution_failure()` helper to reduce error/DLQ branch duplication in worker_pool
- **Phase 01 Plan target:** `src/worker_pool/mod.rs` — new `ExecutionAction` enum + `handle_execution_failure()` helper
- **Phase 01 Summary claim:** Extracted, committed as `f9d2f6f`
- **Verification grep (2026-04-25):** `grep "handle_execution_failure" src/worker_pool/mod.rs` → found at line 37 as `pub(crate) async fn handle_execution_failure` ✓ PRESENT

### DUP-02: message_to_pydict()
- **Requirement:** Extract `message_to_pydict()` helper to eliminate copied Python conversion logic
- **Phase 01 Plan target:** `src/python/handler.rs` — new `message_to_pydict()` helper (30+ lines)
- **Phase 01 Summary claim:** Extracted, committed as `c0cf3c3`
- **Verification grep (2026-04-25):** `grep "message_to_pydict" src/python/handler.rs` → NOT FOUND in handler.rs
  - BUT: `grep "message_to_pydict" src/worker_pool/mod.rs` → found at line 23
  - This suggests the function was placed in the wrong file per the plan, or the plan was updated during execution
  - **ACTION REQUIRED:** Verify correct location and document discrepancy

### DUP-03: flush_partition_batch()
- **Requirement:** Extract `flush_partition_batch()` helper to consolidate batch flush boilerplate (6 repetitions)
- **Phase 01 Plan target:** `src/worker_pool/mod.rs` — new `flush_partition_batch()` helper (40+ lines)
- **Phase 01 Summary claim:** Extracted, committed as `2ca3b8b`
- **Verification grep (2026-04-25):** `grep "flush_partition_batch" src/worker_pool/mod.rs` → **NOT FOUND** ❌ MISSING
- **Gap confirmed:** DUP-03 was claimed complete but the helper is NOT in the codebase. This is the primary gap.

### DUP-04: NoopSink consolidation
- **Requirement:** Verify NoopSink duplication between worker_pool and observability — consolidate if redundant
- **Phase 01 Plan target:** Move `NoopSink` from `worker_pool/mod.rs` to `observability/metrics.rs`
- **Phase 01 Summary claim:** Moved, committed as `9c32260`
- **Verification grep (2026-04-25):**
  - `grep "struct NoopSink" src/worker_pool/mod.rs` → empty (not in worker_pool) ✓
  - `grep "struct NoopSink" src/observability/metrics.rs` → found at line 93 ✓ PRESENT

### Claude's Discretion

**On DUP-02 location discrepancy:** The plan specified `src/python/handler.rs` but `message_to_pydict` appears in `src/worker_pool/mod.rs`. This may indicate the plan was deviated during execution, or the file was later moved. The executor should verify the authoritative location and document it.

**On DUP-03 (flush_partition_batch):** The function is absent from the codebase despite the summary claiming it was committed. The plan must include a task to either (a) extract the helper now if it was lost, or (b) confirm it's under a different name, or (c) update the requirement status.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase 01 execution artifacts
- `.planning/phases/01-extract-duplication/01-01-PLAN.md` — original plan specifying extraction targets
- `.planning/phases/01-extract-duplication/01-01-SUMMARY.md` — execution summary with claims and grep results
- `.planning/REQUIREMENTS.md` — DUP-01 through DUP-04 requirement definitions

### Codebase files to verify
- `src/worker_pool/mod.rs` — contains handle_execution_failure, message_to_pydict (DUP-01, DUP-02), flush_partition_batch (DUP-03)
- `src/python/handler.rs` — originally targeted for message_to_pydict (DUP-02)
- `src/observability/metrics.rs` — NoopSink location (DUP-04)

### Verification standard
- `cargo clippy --all-targets` must pass with no new warnings
- `cargo test --lib` must pass
- All grep-based verification from 01-01-PLAN.md verification section must pass

</canonical_refs>

<specifics>
## Specific Ideas

**DUP-03 gap closure task must:**
1. Read the 6 flush sites in `batch_worker_loop` in `src/worker_pool/mod.rs`
2. Extract the repeated pattern into `flush_partition_batch()` helper
3. Replace all 6 call sites with the helper
4. Verify with `grep "flush_partition_batch" src/worker_pool/mod.rs` (should show 1 def + 6 call sites)
5. Run `cargo clippy --all-targets` and `cargo test --lib`

**DUP-02 discrepancy task must:**
1. Determine authoritative location of `message_to_pydict`
2. If in wrong file per original plan, update plan to reflect reality OR move the function
3. Document the actual call sites (grep showed 4 call sites in worker_pool/mod.rs)

</specifics>

<deferred>
## Deferred Ideas

None — Phase 07 scope is to verify and fix DUP-01 through DUP-04

</deferred>

---

*Phase: 07-verify-dup*
*Context gathered: 2026-04-25 via gap closure planning*
