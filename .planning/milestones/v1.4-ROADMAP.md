# Roadmap: KafPy — v1.4 Failure Handling & DLQ

## Milestones

- **v1.0 Core Consumer Refactor** — Phases 1-5 (shipped 2026-04-15)
- **v1.1 Dispatcher Layer** — Phases 6-8 (shipped 2026-04-16)
- **v1.2 Python Execution Lane** — Phases 9-10 (shipped 2026-04-16)
- **v1.3 Offset Commit Coordinator** — Phases 11-16 (shipped 2026-04-17)
- **v1.4 Failure Handling & DLQ** — Phases 17-20 (in progress)

## Phases

- [x] **Phase 17: Failure Classification** — Structured failure type taxonomy, FailureReason enum, classify method on ExecutionResult (plans 17-01, 17-02 complete)
- [x] **Phase 18: RetryPolicy & Retry Scheduling** — RetryPolicy config, exponential backoff with jitter, retry scheduling (no commit advance) (completed 2026-04-17)
- [x] **Phase 19: DLQ Routing** — DLQ topic naming, DLQ metadata envelope, routing from exhausted/non-retryable failures (completed 2026-04-17)
- [x] **Phase 20: Terminal Handling & Commit Gating** — Terminal state tracking, commit eligibility gating, graceful shutdown DLQ flush (completed 2026-04-17)

---

## Phase Details

### Phase 17: Failure Classification
**Goal**: Structured failure type taxonomy — retryable vs terminal vs non-retryable — with FailureReason enum and classify method on ExecutionResult
**Depends on**: Nothing (first phase of milestone)
**Requirements**: FAIL-01, FAIL-02, FAIL-03, FAIL-04
**Success Criteria** (what must be TRUE):
  1. `FailureReason` enum covers: Retryable (network timeout, transient), Terminal (poison message, deserialization), NonRetryable (validation, business logic)
  2. `ExecutionResult::Error` and `ExecutionResult::Rejected` carry `FailureReason`
  3. `FailureClassifier` trait: `classify(&self, error: &PyErr, context: &ExecutionContext) -> FailureReason`
  4. Python handler sets failure reason via `ExecutionResult::Error(reason, ...)` or `ExecutionResult::Rejected(reason, ...)`
**Plans**: TBD

### Phase 18: RetryPolicy & Retry Scheduling
**Goal**: RetryPolicy config (max attempts, backoff), exponential backoff with jitter, retry scheduling that does NOT advance commit position
**Depends on**: Phase 17
**Requirements**: RETRY-01, RETRY-02, RETRY-03, RETRY-04, RETRY-05, RETRY-06
**Success Criteria** (what must be TRUE):
  1. `RetryPolicy` struct: `max_attempts: usize`, `base_delay: Duration`, `max_delay: Duration`, `jitter_factor: f64`
  2. `RetrySchedule`: computes next retry timestamp with exponential backoff + jitter
  3. Retried messages are NOT acknowledged — `record_ack` called only on final success
  4. `OffsetTracker::mark_failed` called on each retry attempt (for commit gating)
  5. After `max_attempts`, message moves to DLQ path (not retry queue)
  6. Config option `default_retry_policy` in `ConsumerConfig`
**Plans**: TBD

### Phase 19: DLQ Routing
**Goal**: DLQ topic naming convention, DLQ metadata envelope (original topic/partition/offset, timestamp, attempt count, reason), routing for exhausted/non-retryable failures
**Depends on**: Phase 18
**Requirements**: DLQ-01, DLQ-02, DLQ-03, DLQ-04, DLQ-05, DLQ-06
**Success Criteria** (what must be TRUE):
  1. DLQ topic naming: `{original_topic}.DLQ` with partition preserved
  2. `DlqMetadata` struct: original_topic, original_partition, original_offset, failure_reason, attempt_count, first_failure_timestamp, last_failure_timestamp
  3. `DlqRouter` trait with `route(&self, metadata: &DlqMetadata) -> TopicPartition`
  4. Default implementation: `{topic}.DLQ`, partition preserved
  5. On `max_attempts` exceeded OR non-retryable: produce to DLQ topic with metadata headers
  6. Extensible: future support for dead-letter topic per handler or custom routing
**Plans**: TBD

### Phase 20: Terminal Handling & Commit Gating
**Goal**: Terminal state tracking in OffsetTracker, commit eligibility gating (terminal messages don't block commit), graceful shutdown DLQ flush
**Depends on**: Phase 19
**Requirements**: TERM-01, TERM-02, TERM-03, TERM-04
**Success Criteria** (what must be TRUE):
  1. `PartitionState` tracks `has_terminal: bool` — set when any message for that TP is terminal
  2. `should_commit` returns false if `has_terminal` is true (terminal blocks commit)
  3. `graceful_shutdown` flushes DLQ for all pending terminal messages before committing
  4. `WorkerPool::shutdown` calls `flush_terminal_to_dlq()` before `graceful_shutdown`
**Plans**: TBD

## Progress

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 17. Failure Classification | 2/2 | Complete | 2026-04-17 |
| 18. RetryPolicy & Retry Scheduling | 2/2 | Complete    | 2026-04-17 |
| 19. DLQ Routing | 2/2 | Complete    | 2026-04-17 |
| 20. Terminal Handling & Commit Gating | 2/2 | Complete    | 2026-04-17 |

---

## Coverage Map

| Requirement | Phase |
|-------------|-------|
| FAIL-01 | Phase 17 |
| FAIL-02 | Phase 17 |
| FAIL-03 | Phase 17 |
| FAIL-04 | Phase 17 |
| RETRY-01 | Phase 18 |
| RETRY-02 | Phase 18 |
| RETRY-03 | Phase 18 |
| RETRY-04 | Phase 18 |
| RETRY-05 | Phase 18 |
| RETRY-06 | Phase 18 |
| DLQ-01 | Phase 19 |
| DLQ-02 | Phase 19 |
| DLQ-03 | Phase 19 |
| DLQ-04 | Phase 19 |
| DLQ-05 | Phase 19 |
| DLQ-06 | Phase 19 |
| TERM-01 | Phase 20 |
| TERM-02 | Phase 20 |
| TERM-03 | Phase 20 |
| TERM-04 | Phase 20 |

**Total:** 20/20 requirements mapped