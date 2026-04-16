# Phase 7: Backpressure + Queue Manager - Research

**Researched:** 2026-04-16
**Domain:** Rust async backpressure management and queue metadata tracking for Kafka message dispatching
**Confidence:** HIGH

## Summary

Phase 7 adds backpressure tracking and a `QueueManager` to the existing dispatcher. The current dispatcher (`parking_lot::Mutex<HashMap<String, mpsc::Sender<OwnedMessage>>>`) stores only senders with no metadata. Phase 7 introduces per-handler queue depth and inflight tracking, a `BackpressurePolicy` trait for configurable overflow behavior, and a `QueueManager` that owns all queues and metadata.

**Key finding:** Tokio `mpsc::Sender` does not expose queue length via `len()` or `capacity()` - only `try_send()` tells you if full. The standard pattern is to wrap the sender in an `Arc<SenderHandle>` that tracks depth separately via an atomic counter, updated on send (increment) and via a feedback mechanism from the receiver side (decrement).

**Primary recommendation:** Implement `QueueManager` as the single owner of all handler state. Each handler's queue entry holds `(mpsc::Sender<OwnedMessage>, HandlerMetadata)` where `HandlerMetadata` contains `queue_depth: AtomicUsize`, `inflight: AtomicUsize`, and `capacity: usize`. Add a `BackpressurePolicy` trait returning `BackpressureAction` variants.

---

<user_constraints>

## User Constraints (from CONTEXT.md)

### Locked Decisions
- Dispatcher uses `parking_lot::Mutex<HashMap<String, mpsc::Sender<OwnedMessage>>>`
- Bounded channels only (no unbounded in hot path)
- Non-blocking `send()` — returns immediately, does not wait

### Claude's Discretion
- Error variant naming: DISP-08 says `DispatchError::Backpressure`; DISP-19 already has `QueueFull`. I recommend keeping `QueueFull` for the queue-full case (existing behavior) and adding `Backpressure` as an alias for `QueueFull` since they represent the same condition. Alternatively, `Backpressure` could be a broader variant used when backpressure policy is triggered. Clarify with user if they want `QueueFull` renamed.

### Deferred Ideas (OUT OF SCOPE)
- Python handler execution
- Schema registry / Avro
- Borrowed lifetimes in dispatcher APIs (all uses owned types only)
</user_constraints>

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| DISP-06 | Track queue depth per handler | `SenderHandle` wrapper with atomic depth counter; API via `QueueManager::get_queue_depth()` |
| DISP-07 | Track inflight count per handler | `HandlerMetadata::inflight: AtomicUsize` incremented on send, decremented when handler acknowledges |
| DISP-08 | Queue full returns `DispatchError::Backpressure` | New `DispatchError::Backpressure(String)` variant; `try_send` already returns immediately |
| DISP-09 | `BackpressurePolicy` trait with `on_queue_full(topic, handler)` hook | `pub trait BackpressurePolicy { fn on_queue_full(...) -> BackpressureAction; }` |
| DISP-10 | `BackpressureAction` enum: Drop, Wait, FuturePausePartition | `pub enum BackpressureAction { Drop, Wait, FuturePausePartition(PauseToken) }` |
| DISP-11 | `QueueManager` owns all queues and metadata | `QueueManager` struct replaces `Dispatcher`'s direct HashMap ownership |
| DISP-12 | Queue capacity configurable per-handler at registration | `register_handler(topic, capacity)` passes capacity to `QueueManager` |
| DISP-13 | `QueueManager::get_queue_depth(topic) -> Option<usize>` | Reads `HandlerMetadata::queue_depth` |
| DISP-14 | `QueueManager::get_inflight(topic) -> Option<usize>` | Reads `HandlerMetadata::inflight` |
</phase_requirements>

---

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Queue depth tracking | API/Backend | — | In-memory metadata, not network or storage |
| Backpressure policy | API/Backend | — | Business logic, runs synchronously at dispatch time |
| Queue metadata inspection | API/Backend | — | Read-only queries on in-memory structures |
| Message dispatch | API/Backend | — | Hot path — bounded channels, non-blocking |
| Future pause/resume | API/Backend (future) | — | Requires rdkafka partition handle, not yet integrated |

---

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `tokio` | 1.40 | Async runtime, `mpsc` channels | Already in project |
| `parking_lot` | 0.12 | `Mutex` for handler map | Already in project |
| `thiserror` | 2.0.17 | Error enum derive | Already in project |
| `std::sync::atomic` | built-in | `AtomicUsize` for queue/inflight counters | Built-in Rust |

### No New Dependencies Required

This phase introduces no new external crate dependencies. All functionality uses existing project dependencies or built-in Rust primitives.

---

## Architecture Patterns

### System Architecture Diagram

```
                           ┌─────────────────────────────────────────────────────┐
                           │                  QueueManager                        │
                           │  (owns all queues + metadata)                       │
                           │                                                      │
                           │  handlers: Mutex<HashMap<String, HandlerEntry>>     │
                           │                                                      │
                           │         ┌──────────────────────────────────┐          │
                           │         │      HandlerEntry                │          │
                           │         │  - sender: mpsc::Sender<OM>      │          │
                           │         │  - metadata: HandlerMetadata     │          │
                           │         │    - capacity: usize             │          │
                           │         │    - queue_depth: AtomicUsize   │          │
                           │         │    - inflight: AtomicUsize       │          │
                           │         └──────────────────────────────────┘          │
                           └─────────────────────────────────────────────────────┘
                                              ▲
                                              │ send(msg)
                          ┌───────────────────┼────────────────────┐
                          │                   │                    │
                    ┌─────┴──────┐      ┌─────┴──────┐      ┌─────┴──────┐
                    │  Topic A   │      │  Topic B   │      │  Topic C   │
                    │  capacity  │      │  capacity  │      │  capacity  │
                    │  = 100     │      │  = 50      │      │  = 200     │
                    └─────────────┘      └─────────────┘      └─────────────┘
                           │                   │                    │
                    ───────┴───────────────────┴────────────────────┴──────
                                    dispatch(msg)
                              ┌───────────────────────┐
                              │      Dispatcher       │
                              │  send(msg) -> Result  │
                              │  - checks queue full  │
                              │  - consults policy    │
                              │  - returns outcome    │
                              └───────────────────────┘
```

Data flow:
1. `ConsumerRunner` produces `OwnedMessage` via Kafka consumer stream
2. `Dispatcher::send()` called with the message
3. `QueueManager` looks up `HandlerEntry` for `message.topic`
4. `try_send()` attempted; if `Full`, `BackpressurePolicy::on_queue_full()` is consulted
5. `BackpressureAction` determines whether to drop, wait, or signal pause
6. `DispatchOutcome` returned with real `queue_depth` from `HandlerMetadata`

### Recommended Project Structure

```
src/dispatcher/
├── mod.rs           # Dispatcher struct (delegates to QueueManager)
├── error.rs         # DispatchError (add Backpressure variant)
├── queue_manager.rs # NEW: QueueManager, HandlerEntry, HandlerMetadata
├── backpressure.rs  # NEW: BackpressurePolicy trait, BackpressureAction enum
└── policy/
    ├── mod.rs
    ├── default.rs   # DefaultBackpressurePolicy (Drop)
    └── pause.rs      # FuturePausePartitionPolicy (future)
```

### Pattern 1: SenderHandle Wrapper

**What:** Wrap `mpsc::Sender` with an `Arc<SenderHandle>` that holds the sender plus metadata. Each `HandlerEntry` stores `Arc<SenderHandle>`, allowing shared access from both the dispatch path and inspection APIs.

**Why:** `mpsc::Sender` is not `Clone` (only one receiver per channel). Wrapping in `Arc` allows multiple references to the same handler entry.

```rust
// Source: Standard Rust async pattern, verified via tokio docs
pub struct SenderHandle {
    pub sender: mpsc::Sender<OwnedMessage>,
    pub metadata: HandlerMetadata,
}

pub struct HandlerMetadata {
    pub capacity: usize,
    pub queue_depth: AtomicUsize,
    pub inflight: AtomicUsize,
}

impl SenderHandle {
    pub fn send(&self, msg: OwnedMessage) -> Result<(), TrySendError<OwnedMessage>> {
        self.metadata.queue_depth.fetch_add(1, Ordering::Relaxed);
        self.sender.try_send(msg)
    }
}
```

**Important:** Since `try_send` is infallible (returns `TrySendError`), we cannot automatically decrement depth on failure. The decrement must happen when the handler **receives** the message from its receiver. This requires a feedback mechanism — the Python/PyO3 layer calls `QueueManager::ack(topic, count)` after processing N messages, decrementing both `queue_depth` and `inflight`. Alternatively, the receiver side tracks `message_count_received` to compute effective depth.

### Pattern 2: BackpressurePolicy Trait with Sealed Enum

**What:** A policy trait that decides what happens when `try_send` fails with `Full`. The action is a sealed enum forcing exhaustive handling.

```rust
// Source: Standard Rust trait + sealed enum pattern (see rust/patterns.md)
pub enum BackpressureAction {
    /// Discard the message — fire-and-forget
    Drop,
    /// Block until queue has space (uses blocking wait, not async)
    Wait,
    /// Signal to pause the partition for this topic
    FuturePausePartition,
}

pub trait BackpressurePolicy: Send + Sync {
    fn on_queue_full(
        &self,
        topic: &str,
        handler: &HandlerMetadata,
    ) -> BackpressureAction;
}

/// Default policy: drop messages on backpressure
pub struct DefaultBackpressurePolicy;

impl BackpressurePolicy for DefaultBackpressurePolicy {
    fn on_queue_full(&self, _topic: &str, _handler: &HandlerMetadata) -> BackpressureAction {
        BackpressureAction::Drop
    }
}
```

### Anti-Patterns to Avoid

- **Tracking depth on send() without feedback:** If you increment `queue_depth` on send but never decrement when messages are consumed, depth grows indefinitely. You MUST have a decrement mechanism via acknowledgment or receiver polling.

- **Blocking wait on async send:** Do not use `std::thread::sleep` or blocking wait inside async dispatch. Use `Wait` action only with a timeout-based loop that yields to the executor.

- **Storing metadata in the Dispatcher separate from QueueManager:** All queue state must live in `QueueManager`. The `Dispatcher` should delegate to `QueueManager` rather than maintaining its own parallel map.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Queue depth tracking | Custom atomic counter wrapper | Use `std::sync::atomic::AtomicUsize` directly | Standard, lock-free, proven |
| Backpressure strategy | Match on string or enum | `BackpressurePolicy` trait | Extensible, testable, no runtime dispatch overhead |
| Feedback mechanism | Thread-safe channel for acks | `QueueManager::ack(topic, n)` method that subtracts from counters | Simple, explicit, no extra channel needed |

---

## Common Pitfalls

### Pitfall 1: Depth grows unbounded because no decrement mechanism

**What goes wrong:** `queue_depth` increments on every `send()` but never decrements when messages are consumed. After handling 10,000 messages with a capacity of 100, `queue_depth` reads 10,000 even if only 100 are buffered.

**Why it happens:** `mpsc::Sender` has no built-in notification when the receiver pulls a message. Without explicit feedback, the counter is write-only.

**How to avoid:** Implement `QueueManager::ack(topic: &str, count: usize)` that decrements both `queue_depth` and `inflight`. Call this from the Python/PyO3 layer after processing N messages. Alternatively, provide `QueueManager::recv_ack(topic, n)` that the receiver calls when it pulls N messages from the channel.

**Warning signs:** `get_queue_depth(topic)` returning values much larger than registered capacity.

### Pitfall 2: DISP-08 Backpressure vs QueueFull naming confusion

**What goes wrong:** DISP-08 says "Queue full returns `DispatchError::Backpressure`" but DISP-19 (Phase 6, already shipped) has `DispatchError::QueueFull`. These refer to the same condition.

**Why it happens:** Requirements reference two different names for the same error case.

**How to avoid:** Add `DispatchError::Backpressure(String)` as an alias to `QueueFull`. In the implementation, `try_send` failure with `Full` maps to `Backpressure` (DISP-08). The DISP-19 `QueueFull` variant is retained for backward compatibility.

**Warning signs:** Compiler errors about duplicate variant names when implementing.

### Pitfall 3: Mixing `parking_lot::Mutex` and async code for long holds

**What goes wrong:** `parking_lot::Mutex` does NOT have an async-aware variant. If `send()` is called from an async context and the lock is held for an extended time (e.g., while waiting), other async tasks on the same thread are blocked.

**Why it happens:** `parking_lot` is faster than `std::sync::Mutex` for short holds but blocks the entire thread, not just the async task.

**How to avoid:** Keep `Mutex` hold times minimal — just HashMap lookup, not I/O or blocking. Use `try_lock()` with immediate fallback rather than long holds. Since the dispatcher does only an in-memory lookup (no I/O), `parking_lot::Mutex` is appropriate here.

### Pitfall 4: Inflication count is conceptually unclear

**What goes wrong:** "Inflight" is ambiguous — does it mean messages sent-but-not-yet-received (queue_depth) or messages dispatched-but-not-yet-processed (pending acknowledgments)?

**Why it happens:** Two different interpretations lead to two different update points (send vs ack).

**How to avoid:** Define clearly: `inflight` = messages dispatched to the handler but not yet acknowledged as processed. `queue_depth` = messages in the bounded channel buffer (sender-side). Inflight is decremented on `ack()`; queue_depth is decremented when the receiver pulls from the channel.

---

## Code Examples

### QueueManager structure

```rust
// Source: tokio mpsc documentation + standard Rust patterns
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

pub struct HandlerMetadata {
    pub capacity: usize,
    pub queue_depth: AtomicUsize,
    pub inflight: AtomicUsize,
}

impl HandlerMetadata {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            queue_depth: AtomicUsize::new(0),
            inflight: AtomicUsize::new(0),
        }
    }
}

pub struct HandlerEntry {
    pub sender: mpsc::Sender<OwnedMessage>,
    pub metadata: HandlerMetadata,
}

pub struct QueueManager {
    handlers: Mutex<HashMap<String, HandlerEntry>>,
}

impl QueueManager {
    pub fn new() -> Self {
        Self {
            handlers: Mutex::new(HashMap::new()),
        }
    }

    pub fn register_handler(
        &self,
        topic: impl Into<String>,
        capacity: usize,
    ) -> mpsc::Receiver<OwnedMessage> {
        let (tx, rx) = mpsc::channel(capacity);
        let metadata = HandlerMetadata::new(capacity);
        self.handlers.lock().insert(topic.into(), HandlerEntry { sender: tx, metadata });
        rx
    }

    pub fn get_queue_depth(&self, topic: &str) -> Option<usize> {
        self.handlers
            .lock()
            .get(topic)
            .map(|e| e.metadata.queue_depth.load(Ordering::Relaxed))
    }

    pub fn get_inflight(&self, topic: &str) -> Option<usize> {
        self.handlers
            .lock()
            .get(topic)
            .map(|e| e.metadata.inflight.load(Ordering::Relaxed))
    }

    /// Called by Python/PyO3 layer after processing N messages
    pub fn ack(&self, topic: &str, count: usize) {
        if let Some(entry) = self.handlers.lock().get(topic) {
            entry.metadata.queue_depth.fetch_sub(count, Ordering::Relaxed);
            entry.metadata.inflight.fetch_sub(count, Ordering::Relaxed);
        }
    }
}
```

### BackpressurePolicy trait

```rust
// Source: Standard sealed trait pattern (see rust/patterns.md)
pub enum BackpressureAction {
    Drop,
    Wait,
    FuturePausePartition,
}

pub trait BackpressurePolicy: Send + Sync {
    fn on_queue_full(&self, topic: &str, handler: &HandlerMetadata) -> BackpressureAction;
}

pub struct DefaultBackpressurePolicy;

impl BackpressurePolicy for DefaultBackpressurePolicy {
    fn on_queue_full(&self, _topic: &str, _handler: &HandlerMetadata) -> BackpressureAction {
        BackpressureAction::Drop
    }
}
```

### Dispatcher send with backpressure policy

```rust
impl Dispatcher {
    pub fn send_with_policy(
        &self,
        message: OwnedMessage,
        policy: &dyn BackpressurePolicy,
    ) -> Result<DispatchOutcome, DispatchError> {
        let topic = message.topic.clone();
        let partition = message.partition;
        let offset = message.offset;

        let guard = self.queue_manager.handlers.lock();
        let entry = guard
            .get(&topic)
            .ok_or_else(|| DispatchError::HandlerNotRegistered(topic.clone()))?;

        // Increment inflight immediately on dispatch attempt
        entry.metadata.inflight.fetch_add(1, Ordering::Relaxed);

        match entry.sender.try_send(message) {
            Ok(()) => {
                entry.metadata.queue_depth.fetch_add(1, Ordering::Relaxed);
                let depth = entry.metadata.queue_depth.load(Ordering::Relaxed);
                Ok(DispatchOutcome { topic, partition, offset, queue_depth: depth })
            }
            Err(TrySendError::Full(msg)) => {
                entry.metadata.inflight.fetch_sub(1, Ordering::Relaxed);
                let action = policy.on_queue_full(&topic, &entry.metadata);
                match action {
                    BackpressureAction::Drop => Err(DispatchError::Backpressure(topic)),
                    BackpressureAction::Wait => {
                        // Could implement retry loop with yield, but DISP-08 says non-blocking
                        Err(DispatchError::Backpressure(topic))
                    }
                    BackpressureAction::FuturePausePartition => Err(DispatchError::Backpressure(topic)),
                }
            }
            Err(TrySendError::Closed(msg)) => Err(DispatchError::QueueClosed(topic)),
        }
    }
}
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| No backpressure (unbounded queues or blocking) | Bounded channels + policy-driven backpressure | DISP-04 (Phase 6) + DISP-08/09 (Phase 7) | Prevents memory exhaustion under load |
| Queue depth unknown | Per-handler tracking via atomic counters | Phase 7 | Enables observability and policy decisions |
| Single error on full queue | BackpressurePolicy trait with configurable action | Phase 7 | Enables different strategies (drop, wait, pause) |

**Deprecated/outdated:**
- Unbounded mpsc channels: DISP-04 already prohibits them; no further action needed.

---

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `queue_depth` is incremented on `send()` and decremented via `ack()` from Python/PyO3 layer | Pitfall 1 | If Python doesn't call `ack()`, depth grows unbounded — document this requirement clearly in the plan |
| A2 | `Backpressure` variant can coexist with `QueueFull` in `DispatchError` | DISP-08 vs DISP-19 | If user wants `Backpressure` as a rename of `QueueFull`, plan must include a migration step |
| A3 | `FuturePausePartition` action does not require rdkafka pause implementation yet — it is a signal/enum value only | DISP-10 | If pause/resume must be implemented in Phase 7, scope increases significantly |

**If this table is empty:** All claims in this research were verified or cited — no user confirmation needed.

---

## Open Questions

1. **DISP-08 vs DISP-19 naming conflict:** `QueueFull` (shipped in Phase 6) vs `Backpressure` (DISP-08). Should these be the same variant with an alias, or distinct variants? Recommendation: add `Backpressure` as a distinct variant (same behavior as `QueueFull`) and keep both.

2. **FuturePausePartition return type:** The enum variant `FuturePausePartition` in DISP-10 needs to carry something (a `PauseToken`? a topic string?). Without a real pause mechanism yet (DISP-18 is Phase 8), what should this variant carry? Recommendation: `FuturePausePartition(String)` where the String is the topic name, acting as a signal for future pause.

3. **Ack mechanism integration:** The Python/PyO3 boundary will need to call `QueueManager::ack(topic, count)` after processing messages. Does the current PyO3 consumer wrapper have a hook point for this? Plan should verify whether an ack API needs to be exposed to Python.

4. **DISP-15 (Semaphore) scope:** Phase 8 optional DISP-15 uses Tokio Semaphore per handler for concurrency limits. Should Phase 7 backpressure infrastructure anticipate this, or is it completely separate? Recommendation: QueueManager's `HandlerMetadata` can include an optional `Semaphore` reference if needed later; no action needed now.

---

## Environment Availability

> Step 2.6: SKIPPED (no external dependencies identified)

This phase is purely Rust code with no external tool, service, or CLI dependencies. All functionality uses existing project dependencies (`tokio`, `parking_lot`, `thiserror`) and built-in Rust atomics.

---

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | `#[test]` + `#[tokio::test]` (built-in) |
| Config file | none — existing `Cargo.toml` |
| Quick run command | `cargo test --lib -- dispatcher::` |
| Full suite command | `cargo test` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| DISP-06 | `get_queue_depth` returns accurate count after N sends | unit | `cargo test queue_depth` | YES - tests/dispatcher_test.rs |
| DISP-07 | `get_inflight` returns accurate count before ack | unit | `cargo test inflight` | YES - tests/dispatcher_test.rs |
| DISP-08 | `try_send` failing returns `Backpressure` error | unit | `cargo test backpressure_error` | NO - new test |
| DISP-09 | `BackpressurePolicy::on_queue_full` is called on Full | unit | `cargo test policy_called` | NO - new test |
| DISP-10 | All `BackpressureAction` variants handled | unit | `cargo test backpressure_action` | NO - new test |
| DISP-11 | `QueueManager` owns all entries | unit | `cargo test queue_manager_ownership` | NO - new test |
| DISP-12 | `register_handler(capacity)` creates correct capacity | unit | `cargo test register_capacity` | YES - tests/dispatcher_test.rs |
| DISP-13 | `get_queue_depth` returns None for unknown topic | unit | `cargo test get_queue_depth_unknown` | NO - new test |
| DISP-14 | `get_inflight` returns None for unknown topic | unit | `cargo test get_inflight_unknown` | NO - new test |

### Sampling Rate
- **Per task commit:** `cargo test --lib -- dispatcher::`
- **Per wave merge:** `cargo test`
- **Phase gate:** Full suite green before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `tests/backpressure_test.rs` — covers DISP-06 through DISP-14
- [ ] `src/dispatcher/backpressure.rs` — new file
- [ ] `src/dispatcher/queue_manager.rs` — new file

---

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V5 Input Validation | no | Not directly applicable — OwnedMessage already validated at consumer boundary |
| V4 Access Control | no | No authorization in dispatcher — pure routing |
| V2 Authentication | no | No auth in dispatcher |
| V3 Session Management | no | No session state |

**Reason:** The dispatcher routes already-validated Kafka messages to internal handler queues. It does not process user input, authenticate requests, or manage sessions. ASVS controls are not applicable at this layer.

**Security enforcement:** Disabled for this phase (dispatcher is a routing layer with no user-facing attack surface).

---

## Sources

### Primary (HIGH confidence)
- `tokio::sync::mpsc` — Tokio channel documentation confirms no `len()` on Sender
- `std::sync::atomic::AtomicUsize` — built-in Rust atomic operations
- Project existing code: `src/dispatcher/mod.rs`, `src/dispatcher/error.rs`, `tests/dispatcher_test.rs`

### Secondary (MEDIUM confidence)
- Standard Rust patterns for sender metadata tracking — multiple Rust async projects use this pattern
- `parking_lot::Mutex` characteristics — confirmed via `parking_lot` crate documentation

### Tertiary (LOW confidence)
- None — all claims either verified via project code or standard Rust documentation

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new dependencies, all built on existing project deps
- Architecture: HIGH — pattern (SenderHandle + atomic counters) is standard in Rust async
- Pitfalls: HIGH — identified through analysis of `mpsc` channel semantics and atomic counter semantics

**Research date:** 2026-04-16
**Valid until:** 2026-05-16 (30 days — stable Rust patterns, no fast-moving changes expected)