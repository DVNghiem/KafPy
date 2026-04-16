---
phase: 10
slug: worker-pool
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-04-16
---

# Phase 10 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust (cargo test) |
| **Config file** | none — standard Rust test infrastructure |
| **Quick run command** | `cargo test --lib` |
| **Full suite command** | `cargo test --lib && cargo test --test '*' 2>/dev/null || true` |
| **Estimated runtime** | ~5 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --lib`
- **After every plan wave:** Run full suite
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 10 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 10-01-01 | 01 | 1 | EXEC-08 | unit | `cargo test --lib 2>&1 \| grep -E "test.*worker\|test.*pool"` | ✅ | ⬜ pending |
| 10-01-02 | 01 | 1 | EXEC-09 | unit | `cargo test --lib 2>&1 \| grep -E "test.*worker"` | ✅ | ⬜ pending |
| 10-01-03 | 01 | 1 | EXEC-10 | integration | `cargo build --lib 2>&1 \| grep -E "spawn_blocking"` | ✅ | ⬜ pending |
| 10-01-04 | 01 | 1 | EXEC-11 | unit | `cargo test --lib 2>&1 \| grep -E "test.*log\|test.*shutdown"` | ✅ | ⬜ pending |
| 10-01-05 | 01 | 1 | EXEC-12 | unit | `cargo test --lib 2>&1 \| grep -E "test.*graceful\|test.*cancel"` | ✅ | ⬜ pending |
| 10-01-06 | 01 | 1 | EXEC-13 | unit | `cargo test --lib 2>&1 \| grep -E "test.*ack"` | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- `tests/worker_pool_test.rs` — unit tests for WorkerPool (spawn, shutdown, message routing)
- `tests/dispatcher_test.rs` — existing (9 tests passing)
- No additional framework installation needed.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Worker graceful shutdown completion | EXEC-12 | Requires integration test with real tokio runtime | Run consumer with messages, send stop signal, verify all in-flight complete |
| Python callback invocation via spawn_blocking | EXEC-10 | Requires Python FFI setup | Verify by running Python consumer with instrumented callback |

*If none: "All phase behaviors have automated verification."*

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 10s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
