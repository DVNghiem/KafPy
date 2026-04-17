# Phase 13 Verification — ConsumerRunner::store_offset + enable_auto_offset_store

**Phase:** 13-consumer-runner-store-offset
**Verified:** 2026-04-17
**Requirement IDs:** COMMIT-01, COMMIT-02, CONFIG-01, CONFIG-02

---

## Requirement Cross-Reference

| ID | Requirement | Status | Evidence |
|----|-------------|--------|----------|
| **COMMIT-01** | `ConsumerRunner::store_offset(topic, partition, offset)` — rdkafka `store_offset()` for two-phase manual offset management | SATISFIED | `src/consumer/runner.rs:131-146` — async method using `spawn_blocking` to call `consumer.store_offset()` |
| **COMMIT-02** | `store_offset` + `commit()` two-phase guard — only call `commit()` when `highest_contiguous > committed_offset` | INCOMPLETE | Phase 13 only provides `store_offset`. The two-phase guard logic lives in `OffsetCommitter::commit_partition()` (see `src/coordinator/commit_task.rs:181-200`), which currently calls `commit()` directly without calling `store_offset` first. The comment at line 189 explicitly states "Phase 13 will add runner.store_offset". The guard pattern using `highest_contiguous()` is implemented in `OffsetTracker` (phase 11), but the connection in `commit_partition` is not yet wired. |
| **CONFIG-01** | `ConsumerConfig` sets `enable.auto.commit=false` (manual commit) | SATISFIED | `src/consumer/config.rs:236` — `"enable.auto.commit", self.enable_auto_commit.to_string()` — field defaults to `false` at line 98 |
| **CONFIG-02** | `ConsumerConfig` sets `enable.auto.offset.store=false` (manual store) | SATISFIED | `src/consumer/config.rs:237-240` — `"enable.auto.offset.store", self.enable_auto_offset_store.to_string()` — field defaults to `false` at line 99 |

---

## must_haves Verification

### Truths

| Truth | Satisfied? | Evidence |
|-------|-----------|----------|
| "ConsumerRunner::store_offset persists offset to rdkafka in-memory state" | YES | `runner.rs:131-146` calls `consumer.store_offset(&topic, partition, offset)` via `spawn_blocking` |
| "store_offset is called before commit in the two-phase guard" | NO | `commit_partition()` at `commit_task.rs:181-200` does NOT call `store_offset` before `commit`. Comment at line 189 says "Phase 13 will add runner.store_offset". Only `commit()` is called directly. |
| "enable_auto_offset_store=false disables automatic rdkafka offset writes" | YES | `config.rs:237-240` sets `enable.auto.offset.store` from `enable_auto_offset_store` field which defaults to `false` |
| "enable_auto_commit=false disables automatic rdkafka commit" | YES | `config.rs:236` sets `enable.auto.commit` from `enable_auto_offset_store` field which defaults to `false` (already existed pre-phase-13) |

### Artifacts

| Artifact | Path | Satisfied? | Evidence |
|----------|------|-----------|----------|
| `ConsumerRunner::store_offset` method | `src/consumer/runner.rs` | YES | Lines 131-146 |
| `ConsumerConfig::enable_auto_offset_store` field | `src/consumer/config.rs` | YES | Line 24 |
| `ConsumerConfigBuilder::enable_auto_offset_store` builder method | `src/consumer/config.rs` | YES | Lines 141-144 |
| `enable.auto.offset.store=false` in rdkafka config | `src/consumer/config.rs` | YES | Lines 237-240 |

---

## Build & Quality Checks

| Check | Command | Result |
|-------|---------|--------|
| Compilation | `cargo build` | PASS (14 pre-existing warnings, 0 errors) |
| Formatting | `cargo fmt --check` | PASS |
| Clippy | `cargo clippy --lib -- -D warnings` | FAIL — 22 pre-existing errors in `dispatcher/mod.rs`, `coordinator/offset_tracker.rs`, `producer/mod.rs`, and `tests/` (none in `runner.rs` or `config.rs`) |

> The clippy failures are all in files not touched by phase 13. They are pre-existing issues introduced before this phase.

---

## Key Finding

**COMMIT-02 is not fully implemented.** Phase 13 delivers `store_offset` but the two-phase guard (`store_offset` called before `commit` when `highest_contiguous > committed_offset`) has NOT been wired into `OffsetCommitter::commit_partition`. The summary at `13-01-SUMMARY.md` claims "Plan executed exactly as written" and "Phase 14 ready to proceed" — but this is inaccurate given COMMIT-02 is the explicit requirement for the two-phase guard, which is not yet in place.

The connection from `OffsetTracker::highest_contiguous()` → `runner.store_offset()` → `runner.commit()` in `commit_partition` was deferred from phase 13.

---

## Files Reviewed

- `/home/nghiem/project/KafPy/.planning/phases/13-consumer-runner-store-offset/13-PLAN.md`
- `/home/nghiem/project/KafPy/.planning/phases/13-consumer-runner-store-offset/13-01-SUMMARY.md`
- `/home/nghiem/project/KafPy/.planning/REQUIREMENTS.md`
- `/home/nghiem/project/KafPy/src/consumer/runner.rs`
- `/home/nghiem/project/KafPy/src/consumer/config.rs`
- `/home/nghiem/project/KafPy/src/coordinator/commit_task.rs`
