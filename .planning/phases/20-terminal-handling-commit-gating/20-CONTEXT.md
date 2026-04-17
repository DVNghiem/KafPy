# Phase 20: Terminal Handling & Commit Gating - Context

**Gathered:** 2026-04-17
**Status:** Ready for planning

<domain>
## Phase Boundary

Terminal state tracking in OffsetTracker, commit eligibility gating (terminal messages don't block commit), graceful shutdown DLQ flush.
</domain>

<decisions>
## Implementation Decisions

### D-01: Commit Gating — Per-Partition Blocking
- `PartitionState` tracks `has_terminal: bool` — set when any message for that TP is terminal
- `should_commit(topic, partition)` returns `false` if `has_terminal=true` for that partition
- **Other partitions still commit** — terminal on TP-0 does NOT block TP-1, TP-2
- Rationale: Big tech / standard Kafka consumer behavior — poison pill on one partition doesn't halt all progress

### D-02: Graceful Shutdown — Flush All Failed
- `graceful_shutdown()` drains all in-flight failed messages (retryable AND terminal) to DLQ before final commit
- `flush_failed_to_dlq()` called on: (a) normal graceful shutdown, (b) max retry exhaustion during normal processing
- `WorkerPool::shutdown()` calls `flush_failed_to_dlq()` before `graceful_shutdown()`
- Rationale: Shutdown should drain all outstanding failed work, not just terminal

### D-03: Terminal Tracking — Set Once, Never Clear
- `has_terminal=true` set on first terminal message for a TP — never cleared
- Persists for consumer lifetime — cleared only on consumer restart
- Rationale: Simpler, more robust. Once a partition has seen a terminal message, that partition needs manual intervention (DLQ inspection/replay) to recover normal operation.

### D-04: Commit Gating vs DLQ Routing
- DLQ routing (Phase 19): fire-and-forget async produce — does NOT affect commit
- Commit gating (Phase 20): `has_terminal` flag blocks commit for that partition
- These are separate mechanisms: DLQ routing is async/fire-and-forget, commit gating prevents offset advance

### D-05: `has_terminal` Setting Point
- Set in `worker_loop`: when `ExecutionResult` is `Error` or `Rejected` with `FailureCategory::Terminal`
- Also set when `should_dlq=true` from `RetryCoordinator` (non-retryable/terminal after max attempts)
- `mark_failed(topic, partition, offset, reason)` in OffsetTracker sets `has_terminal=true` if `reason.category() == Terminal`

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase 17 Artifacts
- `src/failure/reason.rs` — FailureReason, FailureCategory (Terminal classification)
- `src/coordinator/offset_tracker.rs` — PartitionState, OffsetTracker pattern

### Phase 18 Artifacts
- `src/coordinator/retry_coordinator.rs` — RetryCoordinator (signals should_dlq)

### Phase 19 Artifacts
- `src/dlq/router.rs` — DlqRouter trait
- `src/dlq/produce.rs` — SharedDlqProducer (fire-and-forget produce)
- `src/worker_pool/mod.rs` — worker_loop (DLQ routing stubs at lines 106-116, 157-167)

### Key Files
- `src/worker_pool/mod.rs` — WorkerPool::shutdown signature
- `src/coordinator/offset_tracker.rs` — should_commit method
</canonical_refs>

<specifics>
## Specific Ideas

- `graceful_shutdown_flush()`: iterates all partitions, for each partition with `has_failed=true`, produces all failed messages to DLQ, then calls `offset_coordinator.commit_partition`
- `flush_failed_to_dlq()` signature: `fn flush_failed_to_dlq(&self, dlq_router: &Arc<DlqRouter>, dlq_producer: &Arc<SharedDlqProducer>)`
- Clear distinction: `flush_failed_to_dlq` (drains failed messages to DLQ) vs `graceful_shutdown` (final offset commit after flush)
</specifics>

<deferred>
## Deferred Ideas

- Manual DLQ replay mechanism — re-process messages from DLQ with original handler
- Per-partition DLQ topic override — custom routing per topic/handler
</deferred>

---

*Phase: 20-terminal-handling-commit-gating*
*Context gathered: 2026-04-17*
