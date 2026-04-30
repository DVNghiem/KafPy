---
wave: 11
depends_on: 10
files_modified:
  - src/worker_pool/fan_out.rs (new)
  - src/worker_pool/worker.rs
  - src/python/handler.rs
autonomous: true
---

# Phase 11 Plan: Fan-Out Core

**Requirements:** FANOUT-01, FANOUT-02, FANOUT-03
**Goal:** One message triggers multiple sinks in parallel via JoinSet, with bounded fan-out degree enforced at dispatch time.

## Success Criteria

1. A message dispatched to N registered sinks spawns N parallel handler futures via JoinSet
2. Fan-out degree is capped at configured `max_fan_out` (default 4, max 64) and enforced at dispatch time
3. Primary sink completion triggers message ACK immediately; sink failures are non-blocking

---

## Task 1: FanOutTracker struct + callback registration

### Context

Need to track in-flight fan-out branches per message and provide callback-based completion notifications. Each handler has static sink topics declared at registration. Fan-out degree is bounded by `max_fan_out` config (per-handler, max 64).

### <read_first>

- `src/worker_pool/pool.rs` — JoinSet usage pattern
- `src/worker_pool/worker.rs` — worker_loop message dispatch
- `src/python/handler.rs` — PythonHandler struct and invoke_mode_with_timeout

### <action>

Create `src/worker_pool/fan_out.rs` with:

```rust
/// FanOutTracker tracks in-flight branches for a single message dispatch.
///
/// # Fields
/// - `max_fan_out: u8` — configured max fan-out degree (default 4, max 64)
/// - `sinks: Vec<SinkConfig>` — registered sink topics and their handlers
/// - `inflight: AtomicU8` — current in-flight branch count
/// - `on_complete: CallbackRegistry` — completion callbacks per branch
pub struct FanOutTracker {
    max_fan_out: u8,
    sinks: Vec<SinkConfig>,
    inflight: std::sync::atomic::AtomicU8,
    callback_registry: CallbackRegistry,
}

/// SinkConfig holds topic + handler reference for a single sink branch.
pub struct SinkConfig {
    pub topic: String,
    pub handler: Arc<PythonHandler>,
}

/// CallbackRegistry manages completion callbacks per branch_id.
pub struct CallbackRegistry {
    callbacks: tokio::sync::Mutex<std::collections::HashMap<u64, Box<dyn Fn(BranchResult) + Send + Sync>>>,
    next_branch_id: std::sync::atomic::AtomicU64,
}

impl FanOutTracker {
    /// Create a new FanOutTracker with max_fan_out degree.
    /// max_fan_out is capped at 64 at construction time.
    pub fn new(max_fan_out: u8) -> Self {
        let max = max_fan_out.min(64);
        Self {
            max_fan_out: max,
            sinks: Vec::new(),
            inflight: std::sync::atomic::AtomicU8::new(0),
            callback_registry: CallbackRegistry::new(),
        }
    }

    /// Register a sink topic for this handler.
    pub fn register_sink(&mut self, topic: String, handler: Arc<PythonHandler>) {
        self.sinks.push(SinkConfig { topic, handler });
    }

    /// Returns the number of registered sinks.
    pub fn sink_count(&self) -> usize {
        self.sinks.len()
    }

    /// Returns true if fan-out slots are exhausted (inflight >= max_fan_out).
    pub fn is_exhausted(&self) -> bool {
        self.inflight.load(std::sync::atomic::Ordering::SeqCst) >= self.max_fan_out
    }

    /// Atomically increment inflight count.
    /// Returns false if would exceed max_fan_out (caller should backpressure).
    pub fn try_acquire_slot(&self) -> bool {
        // CAS loop: increment only if current < max
        let current = self.inflight.load(std::sync::atomic::Ordering::SeqCst);
        if current >= self.max_fan_out {
            return false;
        }
        self.inflight.compare_exchange(
            current,
            current + 1,
            std::sync::atomic::Ordering::SeqCst,
            std::sync::atomic::Ordering::SeqCst,
        ).is_ok()
    }

    /// Decrement inflight count (called when a branch completes).
    pub fn release_slot(&self) {
        self.inflight.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }

    /// Register a completion callback for a branch.
    /// Returns the branch_id for later correlation.
    pub fn on_sink_complete<F>(&self, callback: F) -> u64
    where
        F: Fn(BranchResult) + Send + Sync + 'static,
    {
        let branch_id = self.callback_registry.next_id();
        self.callback_registry.register(branch_id, Box::new(callback));
        branch_id
    }

    /// Invoke all registered callbacks with the branch result and clear them.
    pub fn emit_completion(&self, branch_id: u64, result: BranchResult) {
        self.callback_registry.emit(branch_id, result);
    }
}

/// Result of a single sink branch execution.
#[derive(Debug, Clone)]
pub enum BranchResult {
    Ok,
    Error { reason: crate::failure::FailureReason, exception: String },
    Timeout { timeout_ms: u64 },
}
```

Add to `src/python/handler.rs` a new field to `PythonHandler`:

```rust
/// Fan-out configuration for this handler.
/// None means fan-out is not configured (single sink mode).
fan_out: Option<FanOutConfig>,
```

Where `FanOutConfig` is:

```rust
/// Fan-out configuration for a handler.
pub struct FanOutConfig {
    /// Sinks to dispatch to in parallel (topic + handler).
    pub sinks: Vec<(String, Arc<PythonHandler>)>,
    /// Max fan-out degree for this handler (default 4, max 64).
    pub max_fan_out: u8,
}
```

### <acceptance_criteria>

- [ ] `FanOutTracker::new(max)` caps at 64: `FanOutTracker::new(128).max_fan_out == 64`
- [ ] `FanOutTracker::try_acquire_slot` returns false when inflight >= max
- [ ] `FanOutTracker::release_slot` decrements inflight count
- [ ] `FanOutTracker::is_exhausted` returns true when inflight >= max_fan_out
- [ ] `FanOutTracker::register_sink` adds sink to the sinks list
- [ ] `FanOutTracker::sink_count` returns correct count after registration
- [ ] CallbackRegistry generates unique branch_ids via atomic counter
- [ ] Callback invocation compiles with `Box<dyn Fn(BranchResult) + Send + Sync>`

---

## Task 2: JoinSet dispatch integration

### Context

Modify the worker_loop to spawn N parallel handler futures via JoinSet when a message has registered sinks. Primary handler completes first and ACKs immediately; sink futures run to completion in background.

### <read_first>

- `src/worker_pool/worker.rs` — existing worker_loop message dispatch
- `src/worker_pool/pool.rs` — JoinSet spawn pattern

### <action>

Modify `worker_loop` in `src/worker_pool/worker.rs`:

1. Add a new branch in the message processing section for fan-out:

```rust
// In the message processing block, after result handling:
// Check if handler has fan-out config
if let Some(fan_out_config) = handler.fan_out_config() {
    // FANOUT-01: Dispatch to multiple sinks in parallel via JoinSet
    let fan_tracker = Arc::new(FanOutTracker::new(fan_out_config.max_fan_out));
    for (sink_topic, sink_handler) in &fan_out_config.sinks {
        let _ = fan_tracker.try_acquire_slot(); // Slot already acquired at dispatch time
    }

    // Primary handler already completed (result available from above).
    // ACK the primary immediately (FANOUT-03: primary ACKed immediately).
    queue_manager.ack(&msg.topic, 1);
    offset_coordinator.record_ack(&ctx.topic, ctx.partition, ctx.offset);

    // Now spawn sink handlers in parallel via JoinSet
    let mut sink_join_set = JoinSet::new();
    for (sink_topic, sink_handler) in fan_out_config.sinks.iter() {
        let sink_tracker = Arc::clone(&fan_tracker);
        let sink_ctx = ExecutionContext::with_trace(
            sink_topic.clone(),
            ctx.partition,
            ctx.offset,
            ctx.worker_id,
            ctx.trace_id.clone(),
            ctx.span_id.clone(),
            ctx.trace_flags.clone(),
        );
        let sink_msg = msg.clone();

        sink_join_set.spawn(async move {
            let branch_id = sink_tracker.on_sink_complete(|result| {
                tracing::debug!(topic = %sink_topic, "fan-out sink completed: {:?}", result);
            });
            let result = sink_handler.invoke_mode_with_timeout(&sink_ctx, sink_msg).await;
            let branch_result = match result {
                ExecutionResult::Ok => BranchResult::Ok,
                ExecutionResult::Error { reason, exception, .. } => {
                    BranchResult::Error { reason, exception }
                }
                ExecutionResult::Timeout { info } => {
                    BranchResult::Timeout { timeout_ms: info.timeout_ms }
                }
                ExecutionResult::Rejected { .. } => BranchResult::Error {
                    reason: crate::failure::FailureReason::Terminal(
                        crate::failure::TerminalKind::HandlerPanic,
                    ),
                    exception: "Rejected".to_string(),
                },
            };
            sink_tracker.emit_completion(branch_id, branch_result);
            sink_tracker.release_slot();
        });
    }

    // Await all sink futures to complete (background, non-blocking for primary ACK)
    // Note: This awaits in a spawned task — primary ACK already returned.
    // In this task, we spawn a background task to drive the JoinSet to completion.
    tokio::spawn(async move {
        while let Some(result) = sink_join_set.join_next().await {
            match result {
                Ok(()) => {}
                Err(e) => {
                    tracing::error!(error = ?e, "fan-out sink task panicked");
                }
            }
        }
    });
} else {
    // Existing single-sink dispatch path (unchanged)
    // ... existing result handling with ACK/DLQ/Retry
}
```

2. The primary result is already handled above in the existing match block — after the match, check for fan-out config and spawn sink futures if present.

### <acceptance_criteria>

- [ ] `cargo check` passes with no errors
- [ ] Fan-out dispatch path compiles with `JoinSet` import
- [ ] `handler.fan_out_config()` returns `Option<&FanOutConfig>` from PythonHandler
- [ ] Sink futures are spawned via `JoinSet::spawn` with `Arc<FanOutTracker>` shared across branches
- [ ] Primary ACK fires before sink futures complete (FANOUT-03)
- [ ] Sink branch completion calls `emit_completion(branch_id, result)`
- [ ] `release_slot()` called after each sink branch completes

---

## Task 3: Backpressure on overflow

### Context

When `FanOutTracker::is_exhausted()` returns true at dispatch time, Kafka consumer must be paused (`PausePartition`). Message waits in queue until slot frees. This extends the existing `PauseOnFullPolicy` backpressure mechanism from v1.1.

### <read_first>

- `src/dispatcher/backpressure.rs` — BackpressureAction and PauseOnFullPolicy
- `src/dispatcher/consumer_dispatcher.rs` — pause_partition method

### <action>

Add a backpressure slot manager in `src/worker_pool/fan_out.rs`:

```rust
/// Semaphore-based slot manager for bounded fan-out.
///
/// Controls concurrent fan-out branch count using a semaphore.
/// Used by worker_loop to acquire a slot before dispatching a fan-out branch.
/// If no slots available, returns BackpressureAction::PausePartition.
pub struct FanOutSlotManager {
    semaphore: Arc<tokio::sync::Semaphore>,
    max_slots: usize,
    topic: String,
}

impl FanOutSlotManager {
    pub fn new(max_fan_out: u8, topic: String) -> Self {
        let max = std::cmp::min(max_fan_out as usize, 64);
        Self {
            semaphore: Arc::new(tokio::sync::Semaphore::new(max)),
            max_slots: max,
            topic,
        }
    }

    /// Try to acquire a fan-out slot.
    /// Returns Ok(permit) if available, Err(BackpressureAction::PausePartition) if exhausted.
    pub async fn acquire(&self) -> Result<tokio::sync::Permit<'_>, BackpressureAction> {
        match self.semaphore.clone().acquire().await {
            Ok(permit) => Ok(permit),
            Err(_) => Err(BackpressureAction::PausePartition {
                topic: self.topic.clone(),
                partition: -1,
            }),
        }
    }

    /// Returns the number of available slots.
    pub fn available(&self) -> usize {
        self.semaphore.available_permits()
    }
}
```

In `worker_loop`, before spawning fan-out futures:

```rust
// Check if fan-out slots are available
let slot_manager = fan_out_config.slot_manager.as_ref();
// If slot_manager.is_none() or slot_manager.available() == 0, trigger backpressure

if slot_manager.map(|s| s.available()).unwrap_or(0) == 0 {
    // FANOUT-02: max_fan_out reached — backpressure
    tracing::warn!(
        topic = %msg.topic,
        max_fan_out = fan_out_config.max_fan_out,
        "fan-out slots exhausted, pausing partition"
    );
    queue_manager.ack(&msg.topic, 1); // ACK primary to unblock consumer
    offset_coordinator.record_ack(&ctx.topic, ctx.partition, ctx.offset);
    // Return backpressure signal to caller via channel or return
    // The caller (ConsumerDispatcher) will call pause_partition on the topic
    return (Err(DispatchError::Backpressure { queue_name: msg.topic.clone(), reason: "fan_out_exhausted".to_string() }),
            Some(BackpressureAction::PausePartition { topic: msg.topic.clone(), partition: -1 }));
}
```

Actually, the backpressure signal needs to propagate to the ConsumerDispatcher. In worker_loop, we return `Result<(), DispatchError>` and an optional `BackpressureAction`. The ConsumerDispatcher handles pause/resume.

### <acceptance_criteria>

- [ ] `FanOutSlotManager::new(4).available() == 4`
- [ ] `FanOutSlotManager::new(64).available() == 64` (cap enforcement)
- [ ] `FanOutSlotManager::new(128).available() == 64` (cap at 64)
- [ ] After `acquire().await` on exhausted manager, returns `Err(BackpressureAction::PausePartition { partition: -1 })`
- [ ] Backpressure return type is `(Result<(), DispatchError>, Option<BackpressureAction>)` from worker_loop
- [ ] When backpressure returned, primary ACK fires to unblock consumer (D-03)

---

## Verification Criteria

### Code-level checks

```bash
# 1. Compilation check
cargo check 2>&1 | grep -E "^error|^warning" | head -20

# 2. FanOutTracker fields present
grep -n "max_fan_out\|inflight\|sinks\|CallbackRegistry" src/worker_pool/fan_out.rs | wc -l
# Expected: > 10 lines

# 3. JoinSet usage in worker_loop
grep -n "JoinSet\|join_next\|spawn" src/worker_pool/worker.rs | wc -l
# Expected: > 5 lines

# 4. FanOutSlotManager available permits
grep -n "available_permits\|available()" src/worker_pool/fan_out.rs | wc -l
# Expected: > 2 lines
```

### Unit test checks

```bash
cargo test fan_out -- --nocapture 2>&1 | tail -20
# Expected: test results showing FanOutTracker, FanOutSlotManager tests

cargo test worker -- --nocapture 2>&1 | tail -20
# Expected: worker_loop tests pass
```

### Integration checks

```bash
# Fan-out degree enforcement
grep -n "max_fan_out\|is_exhausted\|try_acquire_slot" src/worker_pool/fan_out.rs | wc -l
# Expected: > 6 lines

# Primary ACK immediate (search for ack before join_next)
grep -n "ack\|record_ack" src/worker_pool/worker.rs | head -10
# Expected: ACK happens before sink JoinSet spawn (in the fan-out branch)
```

---

## must_haves

- FanOutTracker struct with max_fan_out (default 4, max 64), sink registration, slot acquisition/release, and callback-based completion
- FanOutSlotManager using Semaphore for bounded concurrency control
- worker_loop dispatch modified to spawn N parallel futures via JoinSet when fan-out configured
- Primary ACK fires immediately; sink failures are non-blocking (FANOUT-03)
- Backpressure signal returned when fan-out slots exhausted (FANOUT-02)
- All compilation checks pass with `cargo check`
- Unit tests for FanOutTracker::new (capped at 64), slot acquire/release, exhaust behavior