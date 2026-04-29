---
phase: 7
slug: thread-pool
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-04-29
---

# Phase 7 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | pytest (Rust: cargo test + pytest for Python bindings) |
| **Config file** | none — Wave 0 installs |
| **Quick run command** | `cargo test --lib && pytest tests/ -x -q` |
| **Full suite command** | `cargo test && pytest tests/ -q` |
| **Estimated runtime** | ~60 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --lib`
- **After every plan wave:** Run `cargo test && pytest tests/ -q`
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 60 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 7-01-01 | 01 | 1 | SYNC-01 | T-7-01 | N/A | unit | `cargo test --lib rayon` | ✅ | ⬜ pending |
| 7-01-02 | 01 | 1 | SYNC-02 | — | N/A | unit | `cargo test --lib config` | ✅ | ⬜ pending |
| 7-02-01 | 02 | 1 | SYNC-03 | T-7-02 | N/A | integration | `cargo test --lib shutdown` | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `tests/test_rayon_pool.py` — stubs for SYNC-01 (Rayon pool spawn)
- [ ] `tests/test_consumer_config.py` — stubs for SYNC-02 (rayon_pool_size config)
- [ ] `tests/conftest.py` — shared fixtures (if needed for Python tests)
- [ ] Framework install: `pip install pytest` — if not present

*If none: "Existing infrastructure covers all phase requirements."*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Poll cycle not blocked >100ms | SYNC-01 | Requires load testing with Kafka | 1. Start consumer with sync handler that sleeps 200ms 2. Verify consumer.lag() stays < 10 during handler execution |
| Rayon threads never call Tokio | SYNC-01 | Static analysis + code review | grep for "tokio::" in rayon pool files, verify no Handle::current() in rayon closures |

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