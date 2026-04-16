---
phase: 6
slug: dispatcher-core
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-04-16
---

# Phase 6 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `#[tokio::test]` + `#[test]` (Rust native) |
| **Config file** | None — tokio already in Cargo.toml |
| **Quick run command** | `cargo test --lib -- dispatcher` |
| **Full suite command** | `cargo test --lib && cargo test --test dispatcher_test` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --lib -- dispatcher`
- **After every plan wave:** Run `cargo test --lib && cargo test --test dispatcher_test`
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 60 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 06-01-01 | 01 | 1 | DISP-19 (DispatchError 4 variants) | T-D06-02 | Bounded channels prevent DoS | unit | `grep -n "QueueFull\|UnknownTopic\|HandlerNotRegistered\|QueueClosed" src/dispatcher/error.rs` | ✅ | ⬜ pending |
| 06-01-02 | 01 | 1 | DISP-01/02/03/04/05 (Dispatcher API) | T-D06-01 | Exact topic match, no mutation | unit | `grep -n "parking_lot::Mutex.*mpsc::Sender" src/dispatcher/mod.rs` | ✅ | ⬜ pending |
| 06-01-03 | 01 | 1 | Module export | — | N/A | unit | `grep -n "pub mod dispatcher" src/lib.rs` | ✅ | ⬜ pending |
| 06-02-01 | 02 | 2 | All DISP requirements | T-D06-02 | Bounded queue enforcement | unit | `cargo test --test dispatcher_test` | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `src/dispatcher/error.rs` — DispatchError enum with 4 variants
- [ ] `src/dispatcher/mod.rs` — Dispatcher struct with register_handler() and send()
- [ ] `src/lib.rs` — `pub mod dispatcher` export
- [ ] `tests/dispatcher_test.rs` — comprehensive unit tests

*If none: "Wave 0 infrastructure covers all phase requirements."*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| None identified | — | — | — |

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
