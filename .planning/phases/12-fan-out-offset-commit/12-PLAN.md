---
wave: 12
depends_on: 11
files_modified:
  - src/worker_pool/fan_out.rs
  - src/python/context.rs
  - src/dlq/metadata.rs
  - src/worker_pool/worker.rs
  - src/python/handler.rs
autonomous: true
---

# Phase 12 Plan: Fan-Out Offset Commit

**Requirements:** FANOUT-04, FANOUT-05
**Goal:** Offset commit gated on all fan-out branches completing; per-sink errors classified and routed to DLQ with branch metadata.

## Success Criteria

1. Offset is only committed after all fan-out branches complete (tracked by `FanOutTracker::wait_all()`)
2. Each fan-out branch has its own timeout scope propagated via `invoke_mode_with_timeout`
3. Per-sink errors carry `branch_id` and `fan_out_id` metadata for DLQ routing

---

## Task 1: Add `wait_all()` to FanOutTracker

### Context

FanOutTracker currently tracks branch completion via callbacks, but has no mechanism to await all branches reaching terminal state. `wait_all()` is needed so the worker loop can gate the offset commit until all branches finish.

### <read_first>

- `src/worker_pool/fan_out.rs` — existing FanOutTracker, BranchResult, CallbackRegistry

### <action>

Add to `src/worker_pool/fan_out.rs`:

1. Add new struct `BranchResults`:
```rust
/// Results from all fan-out branches for a single message dispatch.
#[derive(Debug, Clone, Default)]
pub struct BranchResults {
    pub results: Vec<(u64, BranchResult)>,
}
```

2. Add new fields to `FanOutTracker`:
```rust
pub struct FanOutTracker {
    // ... existing fields (keep) ...
    /// Total number of registered branches.
    total: u8,
    /// Counter for completed branches.
    completed: std::sync::atomic::AtomicU8,
    /// Notify signal fired when all branches complete.
    notify: tokio::sync::Notify,
    /// Results collected from all branches (branch_id -> result).
    results: Mutex<Vec<(u64, BranchResult)>>,
}
```

3. Modify `FanOutTracker::new(max_fan_out)` to initialize new fields:
```rust
pub fn new(max_fan_out: u8) -> Self {
    let max = max_fan_out.min(64);
    Self {
        // ... existing field initializations ...
        total: 0,
        completed: std::sync::atomic::AtomicU8::new(0),
        notify: tokio::sync::Notify::new(),
        results: Mutex::new(Vec::new()),
    }
}
```

4. Add `register_branch()` method:
```rust
/// Register a new branch and return its branch_id.
/// Must be called before spawning the branch task.
pub fn register_branch(&self) -> u64 {
    let branch_id = self.total;
    self.total.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    branch_id
}
```

5. Add `wait_all()` method:
```rust
/// Wait for all fan-out branches to reach terminal state.
///
/// Returns a `BranchResults` containing all branch outcomes.
/// This MUST be awaited after all branches have been spawned.
pub async fn wait_all(&self) -> BranchResults {
    loop {
        let done = self.completed.load(std::sync::atomic::Ordering::SeqCst);
        if done >= self.total && done > 0 {
            let results = self.results.lock().expect("poisoned").clone();
            return BranchResults { results };
        }
        self.notify.notified().await;
    }
}
```

6. Add `record_branch_result()` method:
```rust
/// Record a branch result and signal completion if all branches are done.
pub fn record_branch_result(&self, branch_id: u64, result: BranchResult) {
    {
        let mut results = self.results.lock().expect("poisoned");
        results.push((branch_id, result));
    }
    let prev = self.completed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    if prev + 1 >= self.total {
        self.notify.notify_waiters();
    }
}
```

### <verify>

```bash
cargo check -p kafpy 2>&1 | grep -E '^error' | head -5; echo $?
# Expected: exit code 0 (no errors printed)
```

### <acceptance_criteria>

- [ ] `BranchResults` struct exists with `results: Vec<(u64, BranchResult)>`
- [ ] `FanOutTracker` has `wait_all()` method that returns `BranchResults`
- [ ] `FanOutTracker` has `register_branch()` that returns `u64` and increments `total`
- [ ] `FanOutTracker` has `record_branch_result(branch_id, result)` that stores result and notifies when done
- [ ] `wait_all()` uses `tokio::sync::Notify` for efficient waiting
- [ ] `cargo check -p kafpy` passes with no errors

---

## Task 2: Add `timeout` field to `SinkConfig`

### Context

FANOUT-04 requires per-sink timeout propagated to each branch handler. `SinkConfig` needs a `timeout` field so each sink can have its own timeout duration, independent of the handler's default timeout.

### <read_first>

- `src/worker_pool/fan_out.rs` — SinkConfig struct

### <action>

Modify `SinkConfig` in `src/worker_pool/fan_out.rs` to add:
```rust
#[derive(Clone)]
pub struct SinkConfig {
    pub topic: String,
    pub handler: Arc<PythonHandler>,
    /// Per-sink timeout. None means use handler's default timeout.
    pub timeout: Option<std::time::Duration>,
}
```

Update `SinkConfig::debug()` (if present) to also show timeout.

### <acceptance_criteria>

- [ ] `SinkConfig` has `timeout: Option<std::time::Duration>` field
- [ ] `SinkConfig` derives `Clone`
- [ ] `cargo check -p kafpy` passes

---

## Task 3: Add `branch_id` and `fan_out_id` to ExecutionContext

### Context

FANOUT-05 requires per-sink error classification and DLQ routing with `branch_id` and `fan_out_id` in metadata. `ExecutionContext` carries context through the pipeline and into `DlqMetadata`.

### <read_first>

- `src/python/context.rs` — ExecutionContext struct and constructors

### <action>

Modify `ExecutionContext` in `src/python/context.rs`:

1. Add new fields:
```rust
pub struct ExecutionContext {
    // ... existing fields ...
    /// Fan-out branch ID. None when not a fan-out branch.
    pub branch_id: Option<u64>,
    /// Fan-out dispatch ID (unique per primary message). None when not a fan-out dispatch.
    pub fan_out_id: Option<u64>,
}
```

2. Update `ExecutionContext::new()` to initialize `branch_id: None, fan_out_id: None`

3. Update `ExecutionContext::with_trace()` to accept `branch_id: Option<u64>` and `fan_out_id: Option<u64>` parameters

### <acceptance_criteria>

- [ ] `ExecutionContext` has `branch_id: Option<u64>` field
- [ ] `ExecutionContext` has `fan_out_id: Option<u64>` field
- [ ] Both fields are set to `None` in `ExecutionContext::new()`
- [ ] `with_trace()` accepts and passes through `branch_id` and `fan_out_id`
- [ ] `cargo check -p kafpy` passes

---

## Task 4: Add `branch_id` and `fan_out_id` to DlqMetadata

### Context

DLQ messages from fan-out branches need `branch_id` and `fan_out_id` metadata for replay routing and observability. When a fan-out branch fails, the DLQ message should carry which branch produced it.

### <read_first>

- `src/dlq/metadata.rs` — DlqMetadata struct and constructor

### <action>

Modify `DlqMetadata` in `src/dlq/metadata.rs`:

1. Add new fields:
```rust
pub struct DlqMetadata {
    // ... existing fields ...
    /// Fan-out branch ID that produced this DLQ message. None when not from fan-out.
    pub branch_id: Option<u64>,
    /// Fan-out dispatch ID (unique per primary message). None when not from fan-out.
    pub fan_out_id: Option<u64>,
}
```

2. Update `DlqMetadata::new()` signature and body to accept and store `branch_id: Option<u64>` and `fan_out_id: Option<u64>`

### <acceptance_criteria>

- [ ] `DlqMetadata` has `branch_id: Option<u64>` field
- [ ] `DlqMetadata` has `fan_out_id: Option<u64>` field
- [ ] `DlqMetadata::new()` accepts both new fields
- [ ] `cargo check -p kafpy` passes

---

## Task 5: Wire `wait_all()` into worker_loop offset commit

### Context

Current fan-out dispatch is fire-and-forget: branches spawn via JoinSet and worker_loop immediately continues to idle. For FANOUT-04/FANOUT-05, offset commit must be gated on all branches completing via `wait_all()`.

The worker_loop currently:
1. Handles primary handler result (ACK or DLQ)
2. Spawns fan-out branches in background (line 364-400)
3. Immediately goes to Idle

The new flow:
1. Handles primary handler result (ACK or DLQ)
2. Spawns fan-out branches with `register_branch()` and stores branch_ids
3. Awaits `wait_all()` before going to Idle (offset commit gating)
4. For each branch result that is Error or Timeout, route to DLQ with branch metadata

### <read_first>

- `src/worker_pool/worker.rs` — lines 326-401 (fan-out dispatch section)
- `src/worker_pool/fan_out.rs` — FanOutTracker, BranchResults
- `src/dlq/metadata.rs` — DlqMetadata
- `src/failure/reason.rs` — FailureReason for DLQ routing

### <action>

Modify `src/worker_pool/worker.rs` fan-out dispatch section (around line 326-401):

1. Replace the JoinSet spawn block with branches that:
   - Register branch via `fan_tracker.register_branch()` to get `branch_id`
   - Pass `branch_id` and `fan_out_id` through `ExecutionContext`
   - Call `record_branch_result()` instead of just `emit_completion()`
   - Route Error/Timeout to DLQ with metadata

2. After the JoinSet spawn loop, add `wait_all()` await:
```rust
// FANOUT-04: Await all branches to complete before offset commit gating
let branch_results = fan_tracker.wait_all().await;
tracing::debug!(fan_out_id = ?fan_out_id, branch_count = branch_results.results.len(), "all fan-out branches completed");
```

3. For each branch result that is Error or Timeout, call DLQ routing with metadata:
```rust
for (branch_id, result) in branch_results.results {
    if !matches!(result, BranchResult::Ok) {
        let (reason, exception, is_timeout) = match &result {
            BranchResult::Error { reason, exception } => {
                (reason.clone(), exception.clone(), false)
            }
            BranchResult::Timeout { timeout_ms } => {
                let reason = FailureReason::Terminal(crate::failure::TerminalKind::HandlerPanic);
                let exception = format!("sink timeout after {}ms", timeout_ms);
                (reason, exception, true)
            }
            BranchResult::Ok => continue,
        };
        // Route to DLQ with branch metadata
        let dlq_meta = DlqMetadata::new(
            original_topic: sink.topic.clone(),
            original_partition: ctx.partition,
            original_offset: ctx.offset,
            failure_reason: exception.clone(),
            attempt_count: 1,
            first_failure_timestamp: chrono::Utc::now(),
            last_failure_timestamp: chrono::Utc::now(),
            timeout_duration: if is_timeout { Some(*timeout_ms) } else { None },
            last_processed_offset: None,
            branch_id: Some(*branch_id),
            fan_out_id: Some(*fan_out_id),
        );
        // Call DLQ routing here
        queue_manager.route_to_dlq(&msg.topic, msg.clone(), dlq_meta);
    }
}
```

4. Add `fan_out_id` generation using a monotonic counter or UUID:
```rust
use std::sync::atomic::AtomicU64;
static FAN_OUT_COUNTER: AtomicU64 = AtomicU64::new(1);
let fan_out_id = FAN_OUT_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
```

### <verify>

```bash
cargo check -p kafpy 2>&1 | grep -E '^error' | head -5; echo $?
# Expected: exit code 0 (no errors printed)
```

### <acceptance_criteria>

- [ ] `wait_all()` is awaited before worker goes to Idle state in fan-out path
- [ ] Each branch is registered via `register_branch()` before spawning
- [ ] `ExecutionContext` is created with `branch_id` and `fan_out_id` for each sink
- [ ] Error/Timeout branches are routed to DLQ with `DlqMetadata` containing `branch_id` and `fan_out_id`
- [ ] `cargo check -p kafpy` passes with no errors

---

## Task 6: Add per-sink timeout support to invoke_mode_with_timeout

### Context

FANOUT-04 requires per-sink timeout propagated to each sink handler. Currently `invoke_mode_with_timeout` uses the handler's `handler_timeout` field. Need to support passing a per-invocation timeout that overrides the handler default.

### <read_first>

- `src/python/handler.rs` — `invoke_mode_with_timeout` method (line 324-377)

### <action>

Add a new method to `PythonHandler` in `src/python/handler.rs`:

```rust
/// Invokes the handler with a per-invocation timeout that overrides handler_timeout.
/// If `timeout` is None, falls back to `self.handler_timeout`.
pub async fn invoke_mode_with_timeout_override(
    &self,
    ctx: &ExecutionContext,
    message: OwnedMessage,
    timeout: Option<Duration>,
) -> ExecutionResult {
    let effective_timeout = timeout.or(self.handler_timeout);
    match effective_timeout {
        Some(t) => {
            match tokio::time::timeout(t, self.invoke_mode(ctx, message)).await {
                Ok(result) => result,
                Err(_) => {
                    tracing::error!(
                        handler_id = %ctx.topic,
                        topic = %ctx.topic,
                        partition = ctx.partition,
                        offset = ctx.offset,
                        timeout_ms = t.as_millis() as u64,
                        "handler timed out after {}ms",
                        t.as_millis()
                    );
                    ExecutionResult::Timeout {
                        info: TimeoutInfo {
                            timeout_ms: t.as_millis() as u64,
                            last_processed_offset: None,
                        },
                    }
                }
            }
        }
        None => self.invoke_mode(ctx, message).await,
    }
}
```

Then update the fan-out dispatch in worker.rs to use this method with `sink.timeout`.

### <acceptance_criteria>

- [ ] `PythonHandler` has `invoke_mode_with_timeout_override(ctx, msg, timeout)` method
- [ ] When `timeout` parameter is `Some(d)`, uses `d` regardless of `handler_timeout`
- [ ] When `timeout` parameter is `None`, falls back to `handler_timeout`
- [ ] `cargo check -p kafpy` passes

---

## Verification Criteria

### Code-level checks

```bash
# 1. Compilation check
cargo check -p kafpy 2>&1 | grep -E "^error" | head -20
# Expected: no errors

# 2. FanOutTracker wait_all fields present
grep -n "wait_all\|register_branch\|record_branch_result\|completed\|notify\|BranchResults" src/worker_pool/fan_out.rs | wc -l
# Expected: > 10 lines

# 3. SinkConfig timeout field
grep -n "timeout.*Duration\|timeout:" src/worker_pool/fan_out.rs | head -5
# Expected: includes "timeout: Option<std::time::Duration>"

# 4. ExecutionContext branch fields
grep -n "branch_id\|fan_out_id" src/python/context.rs | head -10
# Expected: > 4 lines

# 5. DlqMetadata branch fields
grep -n "branch_id\|fan_out_id" src/dlq/metadata.rs | head -10
# Expected: > 4 lines

# 6. Per-invocation timeout method
grep -n "invoke_mode_with_timeout_override" src/python/handler.rs | head -5
# Expected: > 1 line
```

### Unit test checks

```bash
cargo test -p kafpy --lib -- fan_out --nocapture 2>&1 | tail -30
# Expected: FanOutTracker::wait_all, register_branch, record_branch_result tests

cargo test -p kafpy --lib -- worker --nocapture 2>&1 | tail -30
# Expected: worker fan-out tests pass
```

---

## must_haves

- `FanOutTracker::wait_all()` async method that blocks until all registered branches reach terminal state
- `FanOutTracker::register_branch()` returning `u64` branch_id and incrementing total
- `FanOutTracker::record_branch_result()` that stores result and notifies on completion
- `BranchResults` struct with `results: Vec<(u64, BranchResult)>`
- `SinkConfig.timeout: Option<Duration>` for per-sink timeout
- `ExecutionContext` with `branch_id: Option<u64>` and `fan_out_id: Option<u64>` fields
- `DlqMetadata` with `branch_id: Option<u64>` and `fan_out_id: Option<u64>` fields
- Worker loop awaits `wait_all()` before going to Idle in fan-out path
- Per-sink Error/Timeout routed to DLQ with branch metadata
- `PythonHandler::invoke_mode_with_timeout_override(ctx, msg, timeout)` for per-invocation timeout
- All compilation checks pass with `cargo check -p kafpy`