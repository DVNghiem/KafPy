---
phase: "03"
slug: python-handler-api
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-04-29
---

# Phase 03 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust `#[test]` + `#[tokio::test]` (standard Cargo test) |
| **Config file** | None — Wave 0 installs test stubs |
| **Quick run command** | `cargo build --lib 2>&1 \| head -20` |
| **Full suite command** | `cargo test --lib 2>&1 \| tail -20` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo build --lib`
- **After every plan wave:** Run `cargo test --lib`
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 03.1-01 | 01 | 1 | PY-04 | unit | `cargo build --lib` | no | ⬜ pending |
| 03.1-02 | 01 | 1 | PY-04 | unit | `cargo build --lib` | no | ⬜ pending |
| 03.1-03 | 01 | 1 | PY-04 | unit | `cargo build --lib` | no | ⬜ pending |
| 03.1-04 | 01 | 1 | PY-06 | unit | `cargo build --lib` | no | ⬜ pending |
| 03.1-05 | 01 | 1 | PY-06 | unit | `cargo build --lib` | no | ⬜ pending |
| 03.1-06 | 01 | 1 | PY-06 | unit | `cargo build --lib` | no | ⬜ pending |
| 03.1-07 | 01 | 1 | PY-06 | unit | `cargo build --lib` | no | ⬜ pending |
| 03.2-01 | 02 | 2 | CONF-05 | unit | `cargo build --lib` | no | ⬜ pending |
| 03.2-02 | 02 | 2 | CONF-05 | unit | `cargo build --lib` | no | ⬜ pending |
| 03.2-03 | 02 | 2 | PY-01 | unit | `cargo build --lib` | no | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `src/observability/tracing.rs` — `inject_trace_context()` stub replaced with W3C parser
- [ ] `src/python/context.rs` — ExecutionContext with trace_id, span_id fields
- [ ] `src/worker_pool/concurrency.rs` — HandlerConcurrency with Arc&lt;Semaphore&gt;
- [ ] `kafpy/runtime.py` — concurrency parameter added to handler() decorator
- [ ] `kafpy/consumer.py` — __enter__/__exit__ context manager

*Existing infrastructure: `cargo test` in Cargo.toml covers all phase requirements.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Context manager calls stop() | CONF-05 | Requires full consumer lifecycle | `python -c "from kafpy import Consumer; c = Consumer(); c.start()"` |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
