# Phase 16: PyO3 Bridge - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-04-17
**Phase:** 16-pyconsumer-bridge
**Areas discussed:** committer signaling mechanism, ownership of runner in OffsetTracker, BRIDGE-03 verification

---

## Committer signaling mechanism

| Option | Description | Selected |
|--------|-------------|----------|
| Interval-only scanning (Recommended) | OffsetCommitter ticks on interval, calls all_partitions() each tick, no watch channel | ✓ |
| Watch channel per partition | Dispatcher sends TopicPartition on watch channel when message dispatched; committer reacts | |
| watch + interval hybrid | Watch triggers immediate commit cycle, interval as fallback | |

**Notes:** Interval-only is simpler and matches the existing `process_ready_partitions()` tick-based design. No watch channel sender ownership needed. BRIDGE-02 and BRIDGE-03 satisfied without it.

---

## OffsetCommitter runner ownership

| Option | Description | Selected |
|--------|-------------|----------|
| Separate runner clone (Recommended) | Pass runner_arc = Arc::clone(&runner) to OffsetCommitter separately from dispatcher | ✓ |
| Shared runner with dispatcher | Same runner used for dispatch + commit — simpler but single point of failure | |

**Notes:** Separate runner clone keeps concerns decoupled. Current `ConsumerRunner::new()` at pyconsumer.rs:86 creates one runner used for dispatcher stream. Commit operations can use the same runner.

---

## OffsetCommitter spawn location

| Option | Description | Selected |
|--------|-------------|----------|
| tokio::spawn in start() (Recommended) | Spawn OffsetCommitter::run() as independent task before pool.run().await | ✓ |
| ConsumerRunner owns committer | OffsetCommitter spawned inside ConsumerTask::spawn | |

**Notes:** pyconsumer.rs doesn't use ConsumerTask::spawn — it manually spawns dispatcher as a task and pools workers. Interval-only committer fits naturally via tokio::spawn in start().

---

## BRIDGE-03 already satisfied

**BRIDGE-03: ConsumerDispatcher passes Arc<dyn OffsetCoordinator> to WorkerPool**

Current pyconsumer.rs:126: `offset_tracker.clone()` passed to `WorkerPool::new()` ✓ — already satisfied in Phase 15. No changes needed.

---

## Deferred Ideas

None — interval-only approach sidesteps the watch channel sender ownership question entirely.
