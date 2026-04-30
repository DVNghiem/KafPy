# Phase 12: Fan-Out Offset Commit - Research

**Researched:** 2026-04-29
**Domain:** Rust async concurrency (fan-out branch completion gating), Kafka offset management, DLQ routing with branch metadata
**Confidence:** HIGH (codebase inspection confirmed all integration points)

## Summary

Phase 12 implements two requirements: FANOUT-04 (per-sink timeout) and FANOUT-05 (per-sink error classification + DLQ routing). The offset commit is gated on all fan-out branches reaching terminal state via `FanOutTracker::wait_all()`. Each branch carries `branch_id` and `fan_out_id` metadata through `ExecutionContext` into `DlqMetadata` for DLQ routing.

Key findings:
- `BranchResult` enum already covers the 3-category classification (Ok, Error, Timeout)
- `FanOutTracker::wait_all()` does NOT exist yet -- this is the primary new method
- `ExecutionContext` and `DlqMetadata` lack `branch_id`/`fan_out_id` fields -- must be added
- `SinkConfig` lacks a per-sink timeout field -- must be added
- The current fan-out branch dispatch in `worker.rs` (line 364-400) is fire-and-forget; offset commit happens immediately after primary handler success. This must change to await `wait_all()` before offset commit.

---

## User Constraints (from CONTEXT.md)

### Locked Decisions

- **D-01:** Offset commits when all fan-out branches finish, regardless of individual outcomes. Failures (error or timeout) are logged and routed to DLQ -- they do NOT block the offset commit.
- **D-02:** Per-sink timeout via `invoke_mode_with_timeout`. Each branch gets its own deadline scope. A timeout on one branch does not cancel other in-flight branches.
- **D-03:** Three-category classification: `Timeout`, `HandlerError`, `SystemError`. Each category carries `branch_id` and `fan_out_id` metadata for DLQ routing.
- **D-04:** `FanOutTracker::wait_all()` -- all branches must reach a terminal state before offset commit is triggered.

### Deferred Ideas

None.

---

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| FANOUT-04 | Per-fan-out-task timeout propagated to each sink handler | `SinkConfig` needs timeout field; `invoke_mode_with_timeout` already exists |
| FANOUT-05 | Per-sink error classification and DLQ routing (branch_id + fan_out_id in metadata) | `ExecutionContext` needs branch_id/fan_out_id; `DlqMetadata` needs same; `FanOutTracker::wait_all()` needed |

---

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Per-sink timeout | API / Backend (PythonHandler) | -- | `invoke_mode_with_timeout` wraps each branch invocation |
| Branch result tracking | API / Backend (FanOutTracker) | -- | `wait_all()` coordinates all branch terminal states |
| DLQ routing for branches | API / Backend (handle_execution_failure) | -- | Per-branch errors routed to sink-topic DLQ |
| Offset commit gating | API / Backend (worker_loop) | -- | `wait_all()` awaited before `record_ack()` |

---

## Standard Stack

No new external dependencies. Uses existing project primitives:

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `tokio::sync::Notify` | bundled | Branch completion signaling | Lightweight, correct for wait-all pattern |
| `tokio::sync::mpsc::oneshot` | bundled | Wait result collection | Standard channel for receiving terminal results |
| `Arc<AtomicU8>` | std | Completed branch counter | Minimal synchronization overhead |

---

## Architecture Patterns

### System Architecture Diagram

```
Worker Loop (worker.rs)
  │
  ├── Primary handler invoke ──> Success ──────────────────────────┐
  │                                                               │
  │                                                               ▼
  │                                                   FanOutTracker::wait_all()
  │                                                   (blocks offset commit)
  │
  └── Fan-Out Dispatch ──> JoinSet spawns N branch tasks
                               │
                               ├── Branch 1 ──> invoke_mode_with_timeout()
                               │                 └── BranchResult::Ok | Error | Timeout
                               │                     └── emit_completion(branch_id, result)
                               │                     └── DLQ routing if error/timeout
                               │
                               ├── Branch 2 ──> invoke_mode_with_timeout()
                               │                 └── (same pattern)
                               │
                               └── Branch N ──> invoke_mode_with_timeout()
                                                   └── (same pattern)

  All branches complete ──> wait_all() resolves ──> record_ack() for primary offset
```

### Recommended Project Structure

```
src/
├── worker_pool/
│   └── fan_out.rs     # FanOutTracker (wait_all addition), BranchResult, SinkConfig(timeout field)
├── python/
│   ├── context.rs     # ExecutionContext (+branch_id, fan_out_id fields)
│   └── handler.rs     # invoke_mode_with_timeout (already exists, use per-sink)
├── dlq/
│   ├── metadata.rs    # DlqMetadata (+branch_id, fan_out_id fields)
│   └── router.rs      # DefaultDlqRouter (no change needed)
├── failure/
│   └── reason.rs      # BranchErrorCategory enum (Timeout, HandlerError, SystemError)
└── worker_pool/
    └── worker.rs      # Fan-out dispatch wiring (await wait_all(), DLQ routing per branch)
```

### Pattern 1: `FanOutTracker::wait_all()`

**What:** An async method that blocks until all registered fan-out branches reach a terminal state (Ok, Error, or Timeout). Returns a `BranchResults` struct containing all branch outcomes.

**When to use:** Before committing offset for a fanned-out message.

**Implementation sketch** (verified against existing codebase patterns):

```rust
// In src/worker_pool/fan_out.rs

/// Results from all fan-out branches for a single message dispatch.
#[derive(Debug, Clone)]
pub struct BranchResults {
    pub results: Vec<(u64, BranchResult)>, // (branch_id, result)
}

pub struct FanOutTracker {
    // ... existing fields ...
    /// Counter for completed branches (AtomicU8).
    completed: std::sync::atomic::AtomicU8,
    /// Total number of registered branches.
    total: u8,
    /// Notify signal fired when all branches complete.
    notify: tokio::sync::Notify,
    /// Results collected from all branches.
    results: Mutex<Vec<(u64, BranchResult)>>,
}

impl FanOutTracker {
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

    /// Called by a branch task when it reaches terminal state.
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
}
```

### Pattern 2: Per-Sink Timeout via `invoke_mode_with_timeout`

**What:** Each fan-out branch gets its own timeout scope passed into `invoke_mode_with_timeout`. A timeout on one branch does NOT cancel other branches (non-blocking cancellation).

**When to use:** For FANOUT-04.

**Verified:** `invoke_mode_with_timeout` in `src/python/handler.rs` already wraps `tokio::time::timeout` around `invoke_mode`. It accepts `&self` (handler) and `(ctx, message)`. The per-sink timeout is achieved by passing a `timeout`-configured handler per sink, OR by adding a timeout parameter directly to `SinkConfig`.

**Implementation sketch:**

```rust
// In SinkConfig (fan_out.rs):
pub struct SinkConfig {
    pub topic: String,
    pub handler: Arc<PythonHandler>,
    /// Per-sink timeout. None means use handler's default timeout.
    pub timeout: Option<std::time::Duration>,
}

// In branch spawn (worker.rs):
let branch_timeout = sink.timeout.or(handler.handler_timeout());
let result = sink_handler.invoke_mode_with_timeout(&ctx_clone, msg_clone).await;
```

### Pattern 3: DLQ Routing Per Branch

**What:** When a fan-out branch completes with Error or Timeout, route to DLQ using the sink topic as `original_topic`. Attach `branch_id` and `fan_out_id` to both `ExecutionContext` and `DlqMetadata`.

**When to use:** For FANOUT-05.

**Verified:** Existing `handle_execution_failure()` in `worker_pool/mod.rs` already handles Error/Rejected/Timeout and routes to DLQ. The branch DLQ routing would call the same function with enriched context.

**Implementation sketch:**

```rust
// In src/python/context.rs:
pub struct ExecutionContext {
    // ... existing fields ...
    pub branch_id: Option<u64>,
    pub fan_out_id: Option<u64>,
}

// In src/dlq/metadata.rs:
pub struct DlqMetadata {
    // ... existing fields ...
    pub branch_id: Option<u64>,
    pub fan_out_id: Option<u64>,
}
```

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Branch completion coordination | Manual RefCell counters | `tokio::sync::Notify` + `AtomicU8` | Correct, async-native, minimal contention |
| Timeout per branch | `select!` with deadline | `tokio::time::timeout` + per-sink timeout field | Already implemented in `invoke_mode_with_timeout` |
| Branch result collection | Custom channel | `Mutex<Vec<(u64, BranchResult)>>` | Low contention (written once per branch), avoids async channel overhead |

---

## Common Pitfalls

### Pitfall 1: Fire-and-Forget Offset Commit
**What goes wrong:** Offset is committed immediately after primary handler success; fan-out branches complete in background. `wait_all()` is never awaited.

**Why it happens:** Current code (worker.rs line 329-401) spawns branches as detached tasks and does not await them. The `tokio::spawn` at line 391 creates a detached task that runs to completion with no hook back into offset commit.

**How to avoid:** Restructure so `wait_all()` is awaited in the main worker loop BEFORE `record_ack()`. The offset commit becomes `async` in the fan-out path.

**Warning signs:** Searching for `tokio::spawn` near fan-out code with no `.await` on completion.

### Pitfall 2: Timeout Does Not Cancel Other Branches
**What goes wrong:** A branch timeout cancels only that branch via `tokio::time::timeout`, but the branch task itself is dropped. Other in-flight branches continue correctly.

**Why it happens:** `tokio::time::timeout` returns an `Err(Elapsed)` on timeout, but the inner future is dropped. The timeout is branch-scoped so other branches are unaffected.

**How to avoid:** Already handled by per-branch `invoke_mode_with_timeout`. Ensure each branch task uses its own timeout scope, not a shared deadline.

### Pitfall 3: Blocking wait_all() in Sync Context
**What goes wrong:** `wait_all()` uses `.await` but the counter increment `fetch_add` is in the completion callback which is called from within a branch task.

**Why it happens:** The `on_sink_complete` callback is a synchronous `Fn(BranchResult)`. Calling `.record_branch_result()` from a synchronous callback requires either a lock (what we're using) or a dedicated async channel. Using `Mutex<Vec<...>>` is safe here because each branch calls `record_branch_result()` exactly once, and the lock is held briefly.

**Warning signs:** `await` inside a non-async callback. Use `tokio::spawn` to make the record step async if needed.

### Pitfall 4: Missing `branch_id` / `fan_out_id` in DLQ Metadata
**What goes wrong:** DLQ messages from fan-out branches have no indication which branch produced them, making DLQ replay ambiguous.

**Why it happens:** `ExecutionContext` and `DlqMetadata` lack these fields.

**How to avoid:** Add fields to both structs. Ensure `handle_execution_failure` propagates them.

---

## Code Examples

### Modified `FanOutTracker` (wait_all addition)

Verified from existing `src/worker_pool/fan_out.rs` structure (lines 154-241):

```rust
// New fields added to FanOutTracker struct:
completed: std::sync::atomic::AtomicU8,
results: Mutex<Vec<(u64, BranchResult)>>,
notify: tokio::sync::Notify,
```

### DLQ Metadata with Branch Fields

Verified from existing `src/dlq/metadata.rs` (line 10-32):

```rust
// New fields in DlqMetadata:
pub branch_id: Option<u64>,
pub fan_out_id: Option<u64>,
```

### ExecutionContext with Branch Fields

Verified from existing `src/python/context.rs` (line 4-16):

```rust
// New fields in ExecutionContext:
pub branch_id: Option<u64>,
pub fan_out_id: Option<u64>,
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Primary ACK immediately; branches fire-and-forget | Primary ACK gated on all branches via `wait_all()` | Phase 12 | Stronger at-least-once guarantee for fan-out |
| No per-sink timeout | Per-sink timeout via `SinkConfig::timeout` | Phase 12 | Independent timeout per sink |
| No branch DLQ routing | Errors classified and routed to sink DLQ with branch metadata | Phase 12 | Full observability and replay for fan-out failures |

**Deprecated/outdated:**
- Fire-and-forget fan-out branches: No longer acceptable; offset commit must wait.

---

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `handle_execution_failure` can be called for fan-out branch errors with enriched context | Pattern 3 | If the function requires primary-message-only invariants, branch DLQ routing needs a separate path |
| A2 | `tokio::time::timeout` per branch does not interfere with other branch timeouts | Pattern 2 | Independent timeout futures per branch; should be isolated by design |
| A3 | `SinkConfig::timeout` field can be added without breaking existing `FanOutConfig` construction | Architecture | `FanOutConfig` uses `Vec<SinkConfig>`; adding a field is a non-breaking API addition |

---

## Open Questions (RESOLVED)

1. **What is `fan_out_id` uniquely identifying?**
   - What we know: It's a unique ID per fan-out dispatch (per primary message).
   - **RESOLVED:** `fan_out_id` is a monotonic counter incremented via `FAN_OUT_COUNTER.fetch_add(1, Ordering::SeqCst)` at dispatch time. It is NOT the same as `branch_id` -- `fan_out_id` identifies the entire dispatch operation while `branch_id` identifies individual sink branches.

2. **Should `wait_all()` have a timeout?**
   - What we know: `D-01` says failures do not block offset commit.
   - **RESOLVED:** `wait_all()` itself has no explicit timeout -- it blocks until all registered branches complete. However, each branch has its own per-sink timeout via `invoke_mode_with_timeout`. If a branch hangs indefinitely (edge case), the branch task itself is dropped, but `wait_all()` would wait forever. For safety, a `wait_all_timeout` can be added as a future enhancement, but for now D-01 semantics are honored by per-branch timeouts only.

3. **How does DLQ routing interact with sink topic (not primary topic)?**
   - What we know: `DefaultDlqRouter` uses `original_topic` to compute DLQ destination.
   - **RESOLVED:** For fan-out branches, use sink topic as `original_topic` so DLQ goes to `{dlq_prefix}{sink_topic}`. This is more actionable for operators since it indicates which sink failed, not just the primary topic.

---

## Environment Availability

Step 2.6: SKIPPED (no external dependencies -- all implementation is in existing Rust codebase using bundled tokio primitives).

---

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust `#[tokio::test]`, `#[test]` |
| Config file | `Cargo.toml` |
| Quick run command | `cargo test -p kafpy --lib -- worker_pool::fan_out` |
| Full suite command | `cargo test -p kafpy` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| FANOUT-04 | Per-sink timeout fires and produces Timeout BranchResult | unit | `cargo test -p kafpy --lib -- fan_out` | `src/worker_pool/fan_out.rs` |
| FANOUT-05 | FanOutTracker::wait_all() returns all branch results | unit | `cargo test -p kafpy --lib -- fan_out` | `src/worker_pool/fan_out.rs` |
| FANOUT-05 | BranchError classified and routed to DLQ with branch metadata | unit | `cargo test -p kafpy --lib -- worker::tests` | `src/worker_pool/worker.rs` |
| FANOUT-05 | DlqMetadata contains branch_id and fan_out_id | unit | `cargo test -p kafpy --lib -- dlq` | `src/dlq/metadata.rs` |

### Wave 0 Gaps
- `src/failure/classifier.rs` -- needs `BranchErrorCategory` enum for three-category classification
- `src/worker_pool/fan_out.rs` -- needs `wait_all()`, `record_branch_result()`, `BranchResults`
- `src/python/context.rs` -- needs `branch_id`, `fan_out_id` fields
- `src/dlq/metadata.rs` -- needs `branch_id`, `fan_out_id` fields
- `src/worker_pool/worker.rs` -- needs to await `wait_all()` before `record_ack()` and call DLQ routing per branch

---

## Security Domain

> Omitted -- Phase 12 does not introduce new security boundaries. Fan-out is a concurrency mechanism within the existing trust boundary of the Python handler layer.

---

## Sources

### Primary (HIGH confidence)
- `src/worker_pool/fan_out.rs` lines 1-480 -- FanOutTracker, BranchResult, CallbackRegistry, SinkConfig
- `src/python/handler.rs` lines 319-377 -- `invoke_mode_with_timeout` (confirmed timeout wrapping)
- `src/worker_pool/worker.rs` lines 329-401 -- current fan-out branch dispatch (fire-and-forget)
- `src/dlq/metadata.rs` lines 1-60 -- DlqMetadata structure
- `src/dlq/router.rs` lines 1-85 -- DlqRouter trait and DefaultDlqRouter
- `src/python/context.rs` lines 1-52 -- ExecutionContext structure

### Secondary (MEDIUM confidence)
- tokio `Notify` + `AtomicU8` pattern for barrier synchronization -- confirmed from tokio docs (standard pattern, not library-specific)

### Tertiary (LOW confidence)
- None

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH -- only uses existing tokio primitives bundled with project
- Architecture: HIGH -- all integration points confirmed in codebase
- Pitfalls: HIGH -- all three pitfalls identified from code review of current fan-out implementation

**Research date:** 2026-04-29
**Valid until:** 2026-05-29 (30 days -- phase uses stable Rust async patterns)