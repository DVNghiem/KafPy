# Phase 27: Shutdown Drain & Polish - Discussion Log

> **Audit trail only.** Do not use as input to planning or execution agents.

**Date:** 2026-04-18
**Phase:** 27-shutdown-drain-polish
**Areas discussed:** Shutdown drain verification, GIL verification approach, All 4 HandlerMode paths

---

## Area 1: Shutdown Drain

| Status | Notes |
|--------|-------|
| **Already implemented** | batch_worker_loop Branch 3 (lines 655-688) flushes all + invokes handler + breaks |

**Verification task only:** Confirm drain integrates correctly.

---

## Area 2: GIL Verification

| Status | Notes |
|--------|-------|
| **Already implemented** | spawn_blocking for sync, PythonAsyncFuture transient GIL for async |

**Verification task only:** Code review confirms GIL patterns are correct.

---

## Area 3: All 4 HandlerMode Paths

| Status | Notes |
|--------|-------|
| **All 4 modes implemented** | SingleSync (invoke), BatchSync (invoke_batch), SingleAsync (invoke_async), BatchAsync (invoke_batch_async) |

**Verification task only:** Confirm all 4 routes from WorkerPool::new.

---

*Phase: 27-shutdown-drain-polish*
*Discussion complete: 2026-04-18*
