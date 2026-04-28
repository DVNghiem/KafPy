---
phase: "02"
slug: rebalance-handling-failure-handling-lifecycle
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-04-29
---

# Phase 02 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust `#[test]` + `#[tokio::test]` (standard Cargo test) |
| **Config file** | None — Wave 0 installs test stubs |
| **Quick run command** | `cargo test --lib` |
| **Full suite command** | `cargo test --all` |
| **Estimated runtime** | ~60 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --lib`
- **After every plan wave:** Run `cargo test --all`
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 60 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 02-01 | 01 | 1 | CORE-06 | unit | `cargo test rebalance` | no | ⬜ pending |
| 02-01 | 01 | 1 | LIFE-05 | unit | `cargo test rebalance` | no | ⬜ pending |
| 02-02 | 02 | 1 | FAIL-01 | unit | `cargo test failure_category` | YES | ⬜ pending |
| 02-02 | 02 | 1 | FAIL-02 | unit | `cargo test classify` | YES | ⬜ pending |
| 02-03 | 03 | 1 | LIFE-01 | unit | `cargo test shutdown` | YES | ⬜ pending |
| 02-03 | 03 | 1 | LIFE-02 | unit | `cargo test shutdown` | YES | ⬜ pending |
| 02-03 | 03 | 1 | LIFE-03 | unit | `cargo test shutdown` | YES | ⬜ pending |
| 02-04 | 04 | 2 | CORE-08 | unit | `cargo test pause` | YES | ⬜ pending |
| 02-04 | 04 | 2 | OFF-04 | unit | `cargo test terminal` | YES | ⬜ pending |
| 02-05 | 05 | 2 | FAIL-05 | unit | `cargo test dlq_producer` | YES | ⬜ pending |
| 02-05 | 05 | 2 | FAIL-06 | unit | `cargo test dlq_metadata` | YES | ⬜ pending |
| 02-05 | 05 | 2 | FAIL-07 | unit | `cargo test dlq_router` | YES | ⬜ pending |
| 02-05 | 05 | 2 | MSG-03 | unit | `cargo test key_router` | YES | ⬜ pending |
| 02-06 | 06 | 2 | CONF-03 | unit | `cargo test retry_config` | YES | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `src/consumer/context.rs` — CustomConsumerContext with rebalance callbacks; covers CORE-06, LIFE-05
- [ ] `tests/test_rebalance.rs` — Rebalance unit tests; covers CORE-06, LIFE-05
- [ ] `tests/test_sigterm.rs` — SIGTERM handling integration test; covers LIFE-04
- [ ] `tests/test_retry_integration.rs` — RetryCoordinator wiring tests; covers FAIL-03, FAIL-04
- [ ] `tests/test_dlq_integration.rs` — DLQ routing end-to-end tests; covers FAIL-05, FAIL-06, FAIL-07

*Existing infrastructure: `cargo test` in Cargo.toml covers all phase requirements.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| SIGTERM triggers begin_draining() | LIFE-04 | Requires sending actual SIGTERM signal | `rust_test_process --send-sigterm` |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
