---
phase: 16-fix-warning-and-dead-code-not-use-allow-deadcode-must-fix-or
verified: 2026-05-01T00:00:00Z
status: gaps_found
score: 0/7 must-haves verified
overrides_applied: 0
gaps:
  - truth: "cargo build 2>&1 | grep -E 'error|warning:' shows zero warnings"
    status: failed
    reason: "6 warnings remain in cargo build output"
    artifacts:
      - path: "src/worker_pool/fan_in_loop.rs"
        issue: "fan_in_worker_loop function is never used"
      - path: "src/worker_pool/streaming_loop.rs"
        issue: "StreamingState enum and streaming_worker_loop are never used"
      - path: "src/runtime/builder.rs"
        issue: "fan_in_handlers field is never read"
    missing:
      - "Fan-in and streaming functions are called by Phase 11/12 integration code that does not exist yet"
  - truth: "cargo clippy --all-targets 2>&1 shows zero warnings"
    status: failed
    reason: "26 clippy warnings remain"
    artifacts: []
    missing:
      - "Multiple clippy warnings include: dead_code for unused items, too-many-arguments, useless conversion, etc."
  - truth: "cargo test 2>&1 passes"
    status: failed
    reason: "Test compilation fails due to Python/PyO3 environment issues (undefined symbols: PyExc_TypeError, PyUnicode_Type, etc.)"
    artifacts: []
    missing:
      - "Known environment issue: Python development libraries not properly configured in this environment"
  - truth: "No #[allow(dead_code)] or #[allow(unused)] attributes present in src/"
    status: failed
    reason: "81 #[allow(dead_code)] attributes remain across multiple files in src/"
    artifacts:
      - path: "src/consumer/context.rs"
        issue: "2 #[allow(dead_code)] attributes"
      - path: "src/worker_pool/pool.rs"
        issue: "2 #[allow(dead_code)] attributes"
      - path: "src/retry/retry_coordinator.rs"
        issue: "5 #[allow(dead_code)] attributes"
      - path: "src/dlq/produce.rs"
        issue: "1 #[allow(dead_code)] attribute"
      - path: "src/routing/topic_pattern.rs"
        issue: "5 #[allow(dead_code)] attributes"
      - path: "src/routing/header.rs"
        issue: "3 #[allow(dead_code)] attributes"
      - path: "src/routing/key.rs"
        issue: "2 #[allow(dead_code)] attributes"
    missing:
      - "The plan only covered 6 files, but many more files have #[allow(dead_code)] attributes"
  - truth: "No dead code (unused functions, constants, fields) remains"
    status: failed
    reason: "Dead code (unused functions/constants/fields) remains - these are addressed by #[allow(dead_code)] or are expected until Phase 11/12 integrates them"
    artifacts: []
    missing:
      - "fan_in_worker_loop, streaming_worker_loop, StreamingState, MAX_RECOVERY_ATTEMPTS are unused but will be used in Phase 11/12"
  - truth: "No deprecated API calls remain"
    status: verified
    reason: "Python::assume_gil_acquired() replaced with Python::assume_attached() in pyconsumer.rs at lines 289 and 334"
    artifacts:
      - path: "src/pyconsumer.rs"
        issue: ""
  - truth: "No pub function uses pub(crate) types incorrectly"
    status: verified
    reason: "fan_in_worker_loop and streaming_worker_loop changed from pub to pub(crate)"
    artifacts:
      - path: "src/worker_pool/fan_in_loop.rs"
        issue: ""
      - path: "src/worker_pool/streaming_loop.rs"
        issue: ""
deferred: []
---

# Phase 16: Fix Warnings and Dead Code Verification Report

**Phase Goal:** Remove all `#[allow(dead_code)]` attributes and fix all compiler warnings. The warnings must be fixed at the root cause, not suppressed.

**Verified:** 2026-05-01
**Status:** gaps_found
**Re-verification:** No - initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | cargo build shows zero warnings | FAILED | 6 warnings in cargo build output |
| 2 | cargo clippy shows zero warnings | FAILED | 26 clippy warnings |
| 3 | cargo test passes | FAILED | Linker errors due to Python/PyO3 environment issue (known pre-existing issue) |
| 4 | No #[allow(dead_code)] or #[allow(unused)] in src/ | FAILED | 81 #[allow(dead_code)] attributes remain |
| 5 | No dead code remains | FAILED | Unused functions/constants that will be used in Phase 11/12 |
| 6 | No deprecated API calls remain | VERIFIED | assume_gil_acquired replaced with assume_attached at lines 289 and 334 |
| 7 | No pub function uses pub(crate) types | VERIFIED | fan_in_worker_loop and streaming_worker_loop are now pub(crate) |

**Score:** 2/7 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `src/pyconsumer.rs` | assume_gil_acquired -> assume_attached | VERIFIED | Lines 289 and 334 use assume_attached() |
| `src/worker_pool/streaming_loop.rs` | pub -> pub(crate), unused vars fixed | PARTIAL | pub(crate) corrected, but streaming_worker_loop generates dead_code warning |
| `src/worker_pool/worker.rs` | Unused vars prefixed, fields ignored | NOT VERIFIED | Cannot fully verify - clippy has many warnings |
| `src/worker_pool/fan_in_loop.rs` | pub -> pub(crate) | VERIFIED | fan_in_worker_loop is pub(crate) at line 35 |
| `src/dispatcher/queue_manager.rs` | Dead code removed | NOT VERIFIED | grep shows dead code removed but clippy warnings remain |
| `src/observability/metrics.rs` | Dead code removed | NOT VERIFIED | grep shows record_branch_duration removed but clippy warnings remain |

### Key Link Verification

No key links defined in PLAN frontmatter. N/A.

### Data-Flow Trace (Level 4)

N/A - This phase is about compiler warnings and dead code, not data flow.

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Build count | `cargo build 2>&1 \| grep -c "warning:"` | 6 | FAIL |
| Clippy count | `cargo clippy --all-targets 2>&1 \| grep -c "warning:"` | 26 | FAIL |
| Allow dead_code count | `grep -rn "allow(dead_code)" src/ 2>/dev/null \| wc -l` | 81 | FAIL |
| Deprecated API check | `grep "assume_gil_acquired" src/ -r` | no matches | PASS |

### Requirements Coverage

N/A - No requirements declared in this plan.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| src/worker_pool/fan_in_loop.rs | 35 | dead_code: fan_in_worker_loop unused | Warning | Expected until Phase 11/12 integration |
| src/worker_pool/streaming_loop.rs | 21 | dead_code: StreamingState unused | Warning | Expected until Phase 11/12 integration |
| src/worker_pool/streaming_loop.rs | 33 | dead_code: MAX_RECOVERY_ATTEMPTS unused | Warning | Expected until Phase 11/12 integration |
| src/worker_pool/streaming_loop.rs | 49 | dead_code: streaming_worker_loop unused | Warning | Expected until Phase 11/12 integration |
| src/runtime/builder.rs | 53 | dead_code: fan_in_handlers unused | Warning | Expected until Phase 11/12 integration |
| src/consumer/context.rs | 44,46 | #[allow(dead_code)] | Info | Suppression attributes |
| src/worker_pool/pool.rs | 43,47 | #[allow(dead_code)] | Info | Suppression attributes |
| src/retry/retry_coordinator.rs | multiple | #[allow(dead_code)] | Info | Multiple suppression attributes |
| src/dlq/produce.rs | 37 | #[allow(dead_code)] | Info | Suppression attribute |
| src/routing/topic_pattern.rs | multiple | #[allow(dead_code)] | Info | Multiple suppression attributes |
| src/routing/header.rs | multiple | #[allow(dead_code)] | Info | Multiple suppression attributes |
| src/routing/key.rs | 61,67 | #[allow(dead_code)] | Info | Suppression attributes |

### Human Verification Required

None - all verification can be done programmatically.

## Gaps Summary

The phase made partial progress on fixing warnings and dead code:

**Fixed:**
1. Deprecated API: Python::assume_gil_acquired() -> assume_attached() in pyconsumer.rs
2. Visibility: pub -> pub(crate) on fan_in_worker_loop and streaming_worker_loop
3. Dead code removed from the 6 files in scope: STREAMING_BUFFER_CAPACITY, paused_partitions field, pause_partition/resume_partition/is_partition_paused methods from queue_manager.rs; record_branch_duration from metrics.rs

**Not Fixed (out of scope or environment issues):**
1. 81 #[allow(dead_code)] attributes remain in files not covered by this plan (consumer/context.rs, worker_pool/pool.rs, retry/retry_coordinator.rs, dlq/produce.rs, routing/*)
2. 6 cargo build warnings remain (dead_code for items that will be used in Phase 11/12)
3. 26 clippy warnings remain (dead_code + style warnings like too-many-arguments, useless conversion)
4. cargo test fails due to Python/PyO3 linker issues (pre-existing environment issue)

**Key Issue:** The PLAN frontmatter `must_haves` specified the goal as "zero warnings" and "no #[allow(dead_code)]", but the plan only covered 6 specific files. Many more files have these issues outside the plan's scope.

---

_Verified: 2026-05-01_
_Verifier: Claude (gsd-verifier)_