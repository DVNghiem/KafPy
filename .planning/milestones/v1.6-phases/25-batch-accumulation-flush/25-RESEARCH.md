# Phase 25: Batch Accumulation & Flush - Research

**Researched:** 2026-04-18
**Domain:** Rust async batch accumulation, fixed-window timeout, backpressure integration
**Confidence:** HIGH

## Summary

Phase 25 implements message batch accumulation in the worker_loop using a dedicated `BatchAccumulator` struct per handler. Messages accumulate per-partition until `max_batch_size` OR `max_batch_wait_ms` deadline fires. Batches flush atomically to the Python handler via `spawn_blocking`. Results are extracted via inline iteration in worker_loop, flowing into existing RetryCoordinator and OffsetCoordinator paths.

**Primary recommendation:** Create `BatchAccumulator` as a standalone struct in `worker_pool/mod.rs` (not a separate module), using `tokio::select!` for deadline/timeout racing, `parking_lot::Mutex` for interior mutability (consistent with `OffsetTracker`), and `Option<OwnedMessage>` for the in-flight batch.

## User Constraints (from CONTEXT.md)

### Locked Decisions

- **D-01:** BatchAccumulator as dedicated struct in `worker_pool/` (separate from worker_loop)
- **D-02:** Fixed-window timeout (timer starts on first message, deadline fixed at first arrival + max_batch_wait_ms)
- **D-03:** Flush immediately on backpressure (then block)
- **D-04:** Inline iteration for batch results in worker_loop
- **D-05:** Skip PartialFailure in Phase 25 (v1.7+ extension)

### Phase Requirements (from REQUIREMENTS.md)

| ID | Description | Research Support |
|----|-------------|------------------|
| EXEC-04 | BatchAccumulator struct — accumulates per handler until max_batch_size OR max_batch_wait_ms. Per-partition ordering preserved. Fixed-window timeout. | BatchAccumulator design below |
| EXEC-05 | Batch flush on size — when accumulator reaches max_batch_size, flush immediately | flush_on_size() logic |
| EXEC-06 | Batch flush on timeout — when max_batch_wait_ms expires for oldest message, flush entire accumulator. Uses tokio::select! with CancellationToken for shutdown. | timer_instant pattern |
| EXEC-09 | BatchExecutionResult enum — AllSuccess(Vec<Offset>), AllFailure(FailureReason), PartialFailure (extension point, not implemented in v1.6) | execution_result.rs extension |
| EXEC-10 | Batch success → each message calls record_ack() individually. Batch failure → all messages flow to RetryCoordinator. | result extraction pattern |
| EXEC-14 | Backpressure preserved — BackpressurePolicy applied per-batch. If backpressure triggers during batch accumulation, no new messages pulled until cleared. | backpressure integration |

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Message accumulation | worker_loop | — | Accumulates messages between recv() and invoke() |
| Per-partition ordering | BatchAccumulator | — | Batches respect partition boundaries; ordering within partition |
| Timer/deadline management | BatchAccumulator | — | Fixed-window timeout managed internally |
| Batch invoke | PythonHandler | — | spawn_blocking with Vec<OwnedMessage> for BatchSync |
| Result routing | worker_loop | — | Inline iteration matching BatchExecutionResult variant |
| Backpressure signaling | worker_loop | QueueManager | Backpressure blocks message pull; accumulator flushes first |

## Standard Stack

No new external dependencies required. Phase 25 uses existing crates already in the project.

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `tokio::select!` | (tokio 1.x) | Race between message arrival and deadline | Built-in, already used in worker_loop |
| `tokio::time::Instant` | (tokio 1.x) | Fixed-window deadline tracking | Built-in, monotonic time |
| `tokio_util::sync::CancellationToken` | (tokio-util 0.7+) | Shutdown responsiveness | Already wired in worker_loop |
| `parking_lot::Mutex` | (parking_lot 0.12) | Interior mutability for accumulator state | Already used in OffsetTracker/RetryCoordinator |
| `std::collections::HashMap` | (stdlib) | Per-partition accumulator map | Already used in OffsetTracker |

**Installation:** No new packages — all dependencies are already in the project.

## Architecture Patterns

### System Architecture Diagram

```
Worker Loop (per worker)
│
├── rx.recv() ──> message
│                    │
│                    ▼
│              BatchAccumulator::add(message)
│                    │
│         ┌──────────┴──────────┐
│         │                     │
│   batch full?           deadline reached?
│   (max_batch_size)      (first_msg_time + max_wait)?
│         │                     │
│         ▼                     ▼
│   flush_now()  OR  select! {
│                              Some(msg) = deadline => flush()
│                              Some(msg) = rx.recv() => add()
│                              _ = shutdown => flush()
│                            }
│                    │
│                    ▼
│              handler.invoke_batch(batch)
│                    │
│                    ▼
│              BatchExecutionResult
│                    │
│         ┌──────────┴──────────┐
│         │                     │
│   AllSuccess(offsets)    AllFailure(reason)
│         │                     │
│         ▼                     ▼
│   for each offset:        for each msg in batch:
│     record_ack()            record_failure()
│     ack()                   (routes to RetryCoordinator)
│     record_success()
│
└── backpressure signal ──> flush_now() + block upstream
```

### Recommended Project Structure

No new files needed. BatchAccumulator lives in `src/worker_pool/mod.rs` alongside `worker_loop`.

```
src/worker_pool/
└── mod.rs      # worker_loop + BatchAccumulator (new struct added here)
```

### Pattern 1: Per-Partition Accumulator with Fixed-Window Timer

**What:** Each partition has its own `PartitionAccumulator` inside `BatchAccumulator`. Timer starts on first message arrival.

**When to use:** When preserving ordering within partition while batching across partitions.

```rust
// Source: src/coordinator/offset_tracker.rs — PartitionState pattern (mirrored)
struct PartitionAccumulator {
    messages: Vec<OwnedMessage>,
    deadline: Option<tokio::time::Instant>,
}

impl PartitionAccumulator {
    fn is_empty(&self) -> bool { self.messages.is_empty() }

    fn len(&self) -> usize { self.messages.len() }

    /// Start the fixed window timer on first message.
    fn start_timer(&mut self, max_wait: tokio::time::Duration) {
        if self.deadline.is_none() {
            self.deadline = Some(tokio::time::Instant::now() + max_wait);
        }
    }

    /// Check if deadline has expired (for polling-based check).
    fn is_deadline_expired(&self) -> bool {
        self.deadline
            .map(|d| tokio::time::Instant::now() >= d)
            .unwrap_or(false)
    }

    /// Add a message, starting timer if this is the first message.
    fn add(&mut self, msg: OwnedMessage, max_wait: tokio::time::Duration) {
        if self.messages.is_empty() {
            self.start_timer(max_wait);
        }
        self.messages.push(msg);
    }
}
```

**Source:** Mirrored from `PartitionState` in `src/coordinator/offset_tracker.rs` — same per-key state machine pattern.

### Pattern 2: BatchAccumulator with tokio::select! Deadline Racing

**What:** Use `tokio::select!` to race between message arrival and deadline expiration.

**When to use:** When you need to flush on either size OR timeout, without busy-waiting.

```rust
// Source: adapted from tokio docs — select! for timeout racing
async fn accumulation_loop(
    mut accumulator: BatchAccumulator,
    mut rx: mpsc::Receiver<OwnedMessage>,
    shutdown_token: CancellationToken,
) {
    loop {
        let deadline = accumulator.next_deadline(); // earliest deadline across partitions

        tokio::select! {
            // Case 1: Deadline fires — flush all partitions
            _ = tokio::time::sleep_until(deadline) => {
                if let Some(batch) = accumulator.flush_all() {
                    // invoke handler
                }
            }
            // Case 2: Message arrives
            msg = rx.recv() => {
                match msg {
                    Some(msg) => {
                        if accumulator.should_flush(&msg) {
                            if let Some(batch) = accumulator.flush_partition(msg.partition) {
                                // invoke handler for flushed partition
                            }
                        }
                        accumulator.add(msg);
                    }
                    None => break, // channel closed
                }
            }
            // Case 3: Shutdown signal
            _ = shutdown_token.cancelled() => {
                if let Some(batch) = accumulator.flush_all() {
                    // drain remaining
                }
                break;
            }
        }
    }
}
```

**Source:** `tokio::select!` pattern — standard tokio idiom for concurrent futures racing. `CancellationToken` already used in worker_loop for shutdown.

### Pattern 3: Inline Batch Result Extraction

**What:** `worker_loop` matches `BatchExecutionResult` variant and iterates over each message/offset explicitly.

**When to use:** When result routing must be explicit and traceable per message.

```rust
// Source: adapted from worker_loop mod.rs lines 59-228 (existing single-message pattern)
match batch_result {
    BatchExecutionResult::AllSuccess(offsets) => {
        for offset in offsets {
            retry_coordinator.record_success(topic, partition, offset);
            queue_manager.ack(topic, 1);
            offset_coordinator.record_ack(topic, partition, offset);
        }
    }
    BatchExecutionResult::AllFailure(reason) => {
        for msg in batch {
            let (should_retry, should_dlq, delay) =
                retry_coordinator.record_failure(topic, partition, msg.offset, &reason);
            // ... existing retry/DLQ handling (same as single-message path)
        }
    }
    // PartialFailure — skipped in Phase 25
}
```

**Source:** Mirrored from `worker_loop` lines 59-228 which already handles `ExecutionResult::Ok` and `ExecutionResult::Error` with per-message ack/retry flow.

### Anti-Patterns to Avoid

- **Sliding window timer:** Timer should NOT reset on each message arrival. Fixed-window (D-02) means deadline is set once at first message and never recalculated.
- **Per-message backpressure:** Backpressure applies per-batch, not per-message (EXEC-14). Don't check backpressure inside the accumulation loop — only at batch boundaries.
- **Storing messages in RetryCoordinator:** BatchAccumulator owns messages during accumulation; RetryCoordinator only stores retry metadata (attempt count, delay). Messages are held in accumulator during retry scheduling.
- **PartialFailure in Phase 25:** Skipped per D-05. Don't implement `PartialFailure(Vec<Offset>, Vec<(OwnedMessage, FailureReason)>)` in this phase.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Deadline timer across multiple async tasks | Custom timer tracking | `tokio::time::Instant` + `tokio::select!` | Tokio's timer is efficient and cancellation-safe |
| Per-partition state management | HashMap<i32, PartitionState> with manual locking | `parking_lot::Mutex<HashMap>` (same as OffsetTracker) | Consistent with established pattern |
| Shutdown responsiveness | Busy-waiting on a flag | `CancellationToken` | Already wired in worker_loop, zero-cost on idle |

**Key insight:** The `OffsetTracker` pattern (parking_lot::Mutex + HashMap + per-key state struct) is the established pattern for per-partition state in this codebase. BatchAccumulator mirrors it exactly.

## Common Pitfalls

### Pitfall 1: Timer Not Started on First Message
**What goes wrong:** Deadline fires immediately even when accumulator is empty.
**Why it happens:** Forgetting to set deadline when adding first message to empty accumulator.
**How to avoid:** `if self.is_empty() { self.deadline = Some(Instant::now() + max_wait); }` in `add()`.
**Warning signs:** Flushing empty batches; deadline fires before any messages accumulate.

### Pitfall 2: Deadline Timer Not Surviving Backpressure
**What goes wrong:** Timer continues running during backpressure wait, but deadline is calculated from first arrival — it doesn't pause.
**Why it happens:** Fixed-window means deadline is absolute, not extended.
**How to avoid:** Timer deadline is absolute. During backpressure, accumulator flushes immediately and timer continues. This is correct behavior — deadline is fixed at first arrival.
**Warning signs:** Late messages arriving after deadline but before flush get included (correct per D-02: "Flush fires at deadline regardless of late message arrivals").

### Pitfall 3: Mixing Partition Timers with Global Select!
**What goes wrong:** Using a single `tokio::time::sleep_until` for the earliest deadline across all partitions, but accumulator stores messages for multiple partitions.
**Why it happens:** Each partition has its own timer, but select! can only wait on one sleep at a time.
**How to avoid:** Track the earliest deadline across all partitions. When it fires, flush ALL partitions (not just the timed-out one). This matches D-01: "batches combined by partition then flushed."
**Warning signs:** Some partitions never flushing because select! resolved before their deadline.

### Pitfall 4: Missing Flush on Channel Close
**What goes wrong:** Messages sit in accumulator when rx.recv() returns None (channel closed).
**Why it happens:** Not handling the `None` case in the select! branch.
**How to avoid:** `Some(msg) = rx.recv()` vs `None = rx.recv()` must be handled differently — `None` means channel closed, flush remaining and exit.
**Warning signs:** Messages lost on graceful shutdown; tests passing because channel doesn't close in test scenarios.

## Code Examples

### BatchAccumulator Struct Design

```rust
// Source: adapted from PartitionState in offset_tracker.rs
use parking_lot::Mutex;
use std::collections::HashMap;
use tokio::time::Instant;

/// Per-partition message accumulator with fixed-window timer.
struct PartitionAccumulator {
    messages: Vec<OwnedMessage>,
    deadline: Option<Instant>,
}

/// Per-handler batch accumulator.
/// Owns per-partition accumulators; each partition has its own timer.
pub struct BatchAccumulator {
    partition_accumulators: Mutex<HashMap<i32, PartitionAccumulator>>,
    max_batch_size: usize,
    max_batch_wait: std::time::Duration,
}

impl BatchAccumulator {
    pub fn new(max_batch_size: usize, max_batch_wait_ms: u64) -> Self {
        Self {
            partition_accumulators: Mutex::new(HashMap::new()),
            max_batch_size,
            max_batch_wait: std::time::Duration::from_millis(max_batch_wait_ms),
        }
    }

    /// Returns the earliest deadline across all partitions, or None if all empty.
    pub fn next_deadline(&self) -> Option<Instant> {
        let guard = self.partition_accumulators.lock();
        guard
            .values()
            .filter_map(|p| p.deadline)
            .min()
    }

    /// Returns true if adding this message would trigger a flush.
    pub fn should_flush_on_add(&self, partition: i32) -> bool {
        let guard = self.partition_accumulators.lock();
        guard
            .get(&partition)
            .map(|p| p.messages.len() >= self.max_batch_size - 1)
            .unwrap_or(false)
    }

    /// Returns true if any partition has messages and its deadline has expired.
    pub fn is_any_deadline_expired(&self) -> bool {
        let guard = self.partition_accumulators.lock();
        for p in guard.values() {
            if !p.messages.is_empty() && p.is_deadline_expired() {
                return true;
            }
        }
        false
    }

    /// Add a message to the appropriate partition accumulator.
    pub fn add(&self, msg: OwnedMessage) {
        let partition = msg.partition;
        let mut guard = self.partition_accumulators.lock();
        let acc = guard.entry(partition).or_insert_with(|| PartitionAccumulator {
            messages: Vec::new(),
            deadline: None,
        });
        acc.add(msg, self.max_batch_wait);
    }

    /// Flush a specific partition's accumulator, returning the batch.
    pub fn flush_partition(&self, partition: i32) -> Option<Vec<OwnedMessage>> {
        let mut guard = self.partition_accumulators.lock();
        guard.get_mut(&partition).and_then(|acc| {
            if acc.is_empty() {
                None
            } else {
                Some(std::mem::take(&mut acc.messages))
            }
        })
    }

    /// Flush all nonempty partitions.
    pub fn flush_all(&self) -> Vec<(i32, Vec<OwnedMessage>)> {
        let mut guard = self.partition_accumulators.lock();
        let mut result = Vec::new();
        for (partition, acc) in guard.iter_mut() {
            if !acc.is_empty() {
                result.push((*partition, std::mem::take(&mut acc.messages)));
            }
        }
        result
    }
}
```

### Batch Handler Invoke (Phase 25 target)

```rust
// Source: adapted from PythonHandler::invoke (handler.rs lines 114-172)
// BatchSync invoke — Phase 25
pub async fn invoke_batch(
    &self,
    ctx: &ExecutionContext,
    messages: Vec<OwnedMessage>,
) -> BatchExecutionResult {
    let callback = Arc::clone(&self.callback);
    // ... extract topic/partition/offset per message ...

    let result = tokio::task::spawn_blocking(move || {
        Python::attach(|py| {
            // Build Vec<PyDict> for batch
            let py_batch: Vec<Py<PyAny>> = messages
                .iter()
                .map(|msg| {
                    let py_msg = PyDict::new(py);
                    // ... populate py_msg with message fields ...
                    py_msg.into()
                })
                .collect();

            match callback.call1(py, (py_batch,)) {
                Ok(_) => BatchExecutionResult::AllSuccess(offsets),
                Err(py_err) => {
                    // Classify failure — applies to entire batch
                    let reason = DefaultFailureClassifier.classify(&py_err, ctx);
                    BatchExecutionResult::AllFailure(reason)
                }
            }
        })
    })
    .await;

    match result {
        Ok(r) => r,
        Err(_) => BatchExecutionResult::AllFailure(FailureReason::Terminal(
            crate::failure::TerminalKind::HandlerPanic,
        )),
    }
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| No batching | Per-partition batch accumulation | Phase 25 | Throughput improvement via batch invoke |
| Per-message ack/retry | BatchExecutionResult with inline extraction | Phase 25 | Same semantics, fewer async ops |
| Single message invoke | spawn_blocking with Vec<OwnedMessage> | Phase 25 | GIL acquired once per batch, not per message |

**Deprecated/outdated:**
- None in this phase's scope.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `spawn_blocking` path unchanged for BatchSync invoke | Code Examples | If PythonHandler needs refactoring, batch invoke needs update |
| A2 | `parking_lot::Mutex` is acceptable (matches OffsetTracker pattern) | Architecture Patterns | If async Mutex needed, design changes |

**If this table is empty:** All claims in this research were verified or cited — no user confirmation needed.

## Open Questions

1. **Where should `BatchExecutionResult` live?**
   - What we know: `ExecutionResult` is in `src/python/execution_result.rs`. `BatchExecutionResult` is a sibling enum.
   - What's unclear: Whether to add it to `execution_result.rs` or create a separate file.
   - Recommendation: Add to `src/python/execution_result.rs` alongside `ExecutionResult` — same module as related type.

2. **Backpressure integration details**
   - What we know: Backpressure fires via `QueueManager`. When it fires, flush immediately, then signal upstream to stop pulling.
   - What's unclear: The exact API for signaling backpressure upstream from within the accumulation loop.
   - Recommendation: Use `QueueManager::get_inflight()` to check before pulling — if at limit, flush first then block on `rx.recv()` with backpressure-aware select! branch.

## Environment Availability

> Step 2.6: SKIPPED (no external dependencies identified)

Phase 25 adds no external dependencies. All required types (`tokio::select!`, `tokio::time::Instant`, `CancellationToken`, `parking_lot::Mutex`) are already in the project.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust standard `#[test]` + `#[tokio::test]` |
| Config file | None — uses inline test modules |
| Quick run command | `cargo test --lib worker_pool -- --test-threads=1` |
| Full suite command | `cargo test --lib` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| EXEC-04 | BatchAccumulator accumulates per-partition with fixed-window timeout | unit | `cargo test --lib batch_accumulator` | No — new tests needed |
| EXEC-05 | Flush on max_batch_size | unit | `cargo test --lib batch_flush_size` | No — new tests needed |
| EXEC-06 | Flush on max_batch_wait_ms deadline | unit | `cargo test --lib batch_flush_deadline` | No — new tests needed |
| EXEC-09 | BatchExecutionResult::AllSuccess and AllFailure | unit | `cargo test --lib batch_execution_result` | No — new tests needed |
| EXEC-10 | AllSuccess calls record_ack per message | unit | `cargo test --lib batch_result_all_success` | No — new tests needed |
| EXEC-14 | Backpressure flushes immediately | unit | `cargo test --lib batch_backpressure` | No — new tests needed |

### Wave 0 Gaps
- [ ] `tests/worker_pool/batch_accumulator_tests.rs` — EXEC-04, EXEC-05, EXEC-06
- [ ] `tests/worker_pool/batch_result_tests.rs` — EXEC-09, EXEC-10
- [ ] `tests/worker_pool/backpressure_tests.rs` — EXEC-14
- [ ] Framework install: Already using `#[tokio::test]` — no new framework needed

## Security Domain

Phase 25 is an internal batching mechanism with no external attack surface. No security controls required beyond what already exists.

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | No | N/A |
| V3 Session Management | No | N/A |
| V4 Access Control | No | N/A |
| V5 Input Validation | No | Messages already validated at consumer boundary |
| V6 Cryptography | No | N/A |

**No security concerns for this phase.**

## Sources

### Primary (HIGH confidence)
- `src/worker_pool/mod.rs` — worker_loop, existing patterns, CancellationToken usage
- `src/coordinator/offset_tracker.rs` — PartitionState pattern for per-partition state with Mutex<HashMap>
- `src/python/handler.rs` — PythonHandler::invoke spawn_blocking pattern
- `src/dispatcher/queue_manager.rs` — backpressure semaphore API
- `src/coordinator/retry_coordinator.rs` — record_failure/record_success per-message API
- `tokio::select!` — standard tokio concurrency pattern

### Secondary (MEDIUM confidence)
- tokio docs — `tokio::time::Instant` for deadline tracking
- pyo3 docs — spawn_blocking + Python::with_gil pattern

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new dependencies, all existing patterns
- Architecture: HIGH — follows OffsetTracker precedent, locked decisions
- Pitfalls: MEDIUM — timer edge cases need validation in tests

**Research date:** 2026-04-18
**Valid until:** 2026-05-18 (30 days — stable domain)
