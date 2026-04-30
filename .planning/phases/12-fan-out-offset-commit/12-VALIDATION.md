---
phase: 12
slug: fan-out-offset-commit
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-04-29
---

# Phase 12 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust `#[tokio::test]`, `#[test]` |
| **Config file** | `Cargo.toml` |
| **Quick run command** | `cargo test -p kafpy --lib -- worker_pool::fan_out` |
| **Full suite command** | `cargo test -p kafpy` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p kafpy --lib -- worker_pool::fan_out`
- **After every plan wave:** Run `cargo test -p kafpy`
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 60 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 12-01 | 01 | 1 | FANOUT-04 | — | N/A | unit | `cargo test -p kafpy --lib -- fan_out::tests` | ✅ | ⬜ pending |
| 12-02 | 01 | 1 | FANOUT-04 | — | N/A | unit | `cargo test -p kafpy --lib -- fan_out::tests` | ✅ | ⬜ pending |
| 12-03 | 01 | 1 | FANOUT-05 | — | N/A | unit | `cargo test -p kafpy --lib -- context` | ✅ | ⬜ pending |
| 12-04 | 01 | 1 | FANOUT-05 | — | N/A | unit | `cargo test -p kafpy --lib -- dlq` | ✅ | ⬜ pending |
| 12-05 | 01 | 1 | FANOUT-04, FANOUT-05 | — | N/A | unit | `cargo test -p kafpy --lib -- worker::tests` | ✅ | ⬜ pending |
| 12-06 | 01 | 1 | FANOUT-04 | — | N/A | unit | `cargo test -p kafpy --lib -- handler::tests` | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `src/failure/classifier.rs` — needs `BranchErrorCategory` enum for three-category classification
- [ ] `src/worker_pool/fan_out.rs` — needs `wait_all()`, `record_branch_result()`, `BranchResults`
- [ ] `src/python/context.rs` — needs `branch_id`, `fan_out_id` fields
- [ ] `src/dlq/metadata.rs` — needs `branch_id`, `fan_out_id` fields
- [ ] `src/worker_pool/worker.rs` — needs to await `wait_all()` before `record_ack()` and call DLQ routing per branch

*If none: "Existing infrastructure covers all phase requirements."*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| DLQ routing to actual Kafka topic | FANOUT-05 | Requires live Kafka + DLQ setup | Subscribe to DLQ topic, produce a timeout/error branch result, verify message appears |

*If none: "All phase behaviors have automated verification."*

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
