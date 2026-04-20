---
phase: '37'
phase-slug: documentation-packaging
status: draft
nyquist_compliant: false
wave_0_complete: false
created: '2026-04-20'
---

# Phase 37 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | pytest |
| **Config file** | pytest.ini (if created) or pyproject.toml [pytest] section |
| **Quick run command** | `pytest tests/test_docs.py -x -v` (if tests created) |
| **Full suite command** | `pytest tests/ -x -v` |
| **Estimated runtime** | ~30 seconds |

*Note: Phase 37 is primarily documentation and packaging — many verifications are file-existence checks and manual build tests.*

---

## Sampling Rate

- **After every task commit:** Quick file-existence check
- **After every plan wave:** Run pytest tests/ -x -v if tests exist
- **Before `/gsd-verify-work`:** BUILD-04 requires manual maturin develop test

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 37-01-01 | 01 | 1 | DOC-01 | file-existence | `ls README.md` | ❌ W0 | ⬜ pending |
| 37-01-02 | 01 | 1 | DOC-02 | file-existence | `ls docs/` | ❌ W0 | ⬜ pending |
| 37-01-03 | 01 | 1 | DOC-03 | file-existence | `ls kafpy/guides/` | ❌ W0 | ⬜ pending |
| 37-01-04 | 01 | 1 | DOC-04 | grep | `grep -r "def " kafpy/*.py \| wc -l` | ❌ W0 | ⬜ pending |
| 37-01-05 | 01 | 1 | DOC-05 | file-existence | `grep -l "app.run()" README.md` | ❌ W0 | ⬜ pending |
| 37-01-06 | 01 | 1 | DOC-06 | file-existence | `ls docs/guides/` | ❌ W0 | ⬜ pending |
| 37-01-07 | 01 | 1 | BUILD-01 | file-existence | `grep -l "maturin" pyproject.toml` | ❌ W0 | ⬜ pending |
| 37-01-08 | 01 | 1 | BUILD-02 | file-existence | `grep -l "kafpy-python" Cargo.toml` | ❌ W0 | ⬜ pending |
| 37-01-09 | 01 | 1 | BUILD-03 | grep | `grep "__all__" kafpy/__init__.py` | ❌ W0 | ⬜ pending |
| 37-01-10 | 01 | 1 | BUILD-04 | manual | maturin develop (requires librdkafka) | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `docs/` directory — created empty for quickstart guide and API reference
- [ ] `docs/guides/` directory — for handler-patterns.md and other guides
- [ ] `kafpy/guides/__init__.py` — Python package structure for guides
- [ ] `kafpy/guides/getting-started.md` — placeholder content
- [ ] `kafpy/guides/handler-patterns.md` — placeholder content
- [ ] `kafpy/guides/configuration.md` — placeholder content
- [ ] `kafpy/guides/error-handling.md` — placeholder content
- [ ] `README.md` update with quickstart and feature list (if file already exists but is incomplete)
- [ ] Framework install: none required (documentation phase)

*If none: "Existing infrastructure covers all phase requirements."*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| maturin develop works | BUILD-04 | Requires librdkafka installed on target machine; automated testing environment may not have it | Run `maturin develop` in project root and verify no errors |
| docs/ API reference auto-generation | DOC-02 | Requires pdoc or similar; environment-specific | Run `pdoc kafpy` and verify HTML output exists |

*If none: "All phase behaviors have automated verification."*

---

## Validation Sign-Off

- [ ] All tasks have automated verify or Wave 0 dependencies
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter (when coverage complete)

**Approval:** pending