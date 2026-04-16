# Phase 8: ConsumerRunner Integration - Research

**Researched:** 2026-04-16
**Domain:** Rust async integration between `ConsumerRunner::stream()` and `Dispatcher`, with optional semaphore-based concurrency limits and pause/resume signaling
**Confidence:** HIGH

## Summary

Phase 8 integrates the `ConsumerRunner` stream with the `Dispatcher` pipeline, completing the message flow from Kafka consumer to per-handler bounded queues. The primary integration pattern is a `ConsumerDispatcher` struct that owns both the runner and dispatcher, polls the consumer stream asynchronously, and dispatches each `OwnedMessage`. DISP-15 adds optional Tokio `Semaphore` per handler for concurrency limiting. DISP-18 ensures the architecture supports rdkafka partition pause/resume as the actual backpressure action.

**Key finding:** The existing `ConsumerStream` (wrapping `ReceiverStream`) implements `Stream` natively via `StreamExt::next()`. Integration is a straightforward async loop: `while let Some(result) = stream.next().await { match result { Ok(msg) => dispatcher.send(msg)?, Err(e) => log + continue } }`. No custom futures or complex combinators needed.

**Primary recommendation:** Add a `ConsumerDispatcher` coordinator that owns `ConsumerRunner` + `Dispatcher`. Add optional `tokio::sync::Semaphore` to `HandlerMetadata` gated by a `max_concurrency: Option<usize>` field at registration time.

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| DISP-15 | Optional Tokio `Semaphore` per handler for concurrency limits | `HandlerMetadata::semaphore: Option<Arc<Semaphore>>`; acquire on `try_send`, release on `ack` |
| DISP-16 | Dispatcher receives messages from `ConsumerRunner` stream | `ConsumerDispatcher::run()` async loop polling `stream.next()`, dispatching each `OwnedMessage` |
| DISP-17 | Python boundary preserved - owned types only, no borrowed lifetimes | All types (`OwnedMessage`, `DispatchOutcome`, `DispatchError`) are fully owned; `mpsc::Receiver<OwnedMessage>` returned from registration |
| DISP-18 | Designed for `pause_resume(topic)` via rdkafka partition pause/resume | `BackpressureAction::FuturePausePartition(topic)` already exists; pause state tracked in `PausedTopicSet`; `consumer.pause()/resume()` on `StreamConsumer` |
</phase_requirements>

---

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Consumer stream polling | API/Backend | — | Async loop calling `stream.next()`, owned by `ConsumerDispatcher` |
| Message dispatch | API/Backend | — | `Dispatcher::send()` called per message; `DispatchOutcome` returned |
| Optional concurrency limiting | API/Backend | — | `Semaphore` per handler; acquire/release around dispatch |
| Pause/resume signaling | API/Backend | — | `FuturePausePartition` already exists; Phase 8 wires it to actual pause |
| Python boundary | Python FFI | — | `mpsc::Receiver<OwnedMessage>` returned to Python; Python pulls messages |
| Owned-message ownership | Consumer | Dispatcher | `OwnedMessage` is fully owned at every boundary — no lifetimes |

---

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `tokio` | 1.40 | Async runtime, `Semaphore`, `StreamExt` | Already in project |
| `tokio-stream` | 0.1 | `StreamExt::next()` for consumer stream | Already in project |
| `parking_lot` | 0.12 | `Mutex` for handler map | Already in project |
| `thiserror` | 2.0.17 | Error enum derive | Already in project |
| `rdkafka` | 0.36 | `StreamConsumer::pause()/resume()` | Already in project |

### No New Dependencies Required

This phase introduces no new external crate dependencies. All functionality uses existing project dependencies (`tokio`, `rdkafka`, `thiserror`, `parking_lot`).

---

## Architecture Patterns

### System Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                    ConsumerDispatcher                                 │
│                                                                      │
│  - owns: ConsumerRunner + Dispatcher + PausedTopics                  │
│  - owns: partition_handles: Mutex<HashMap<String, TopicPartitionList>>│
│                                                                      │
│  run() async loop:                                                  │
│    while let Some(result) = stream.next().await {                    │
│      match result {                                                  │
│        Ok(msg) => dispatcher.send_with_policy(msg, &policy)?         │
│                 if FuturePausePartition(topic) => pause_partition()  │
│        Err(e) => log + continue (non-fatal)                          │
│      }                                                               │
│    }                                                                 │
└────────────────────────────┬────────────────────────────────────────┘
                             │
              ┌──────────────┴──────────────────┐
              │       dispatch(msg)              │
              ▼                                  ▼
┌─────────────────────────┐        ┌──────────────────────────────┐
│   ConsumerRunner         │        │        Dispatcher             │
│   stream() -> ConsumerStream    │  send_with_policy(msg, policy) │
│   - from_borrowed()      │        │  → QueueManager::send_to_handler │
│   - OwnedMessage output  │        │                               │
└─────────────────────────┘        └──────────────────────────────┘
                                                        │
                                                        ▼
                                            ┌────────────────────────┐
                                            │     QueueManager        │
                                            │                         │
                                            │  handlers: Mutex<HashMap│
                                            │  ┌───────────────────┐  │
                                            │  │ HandlerEntry      │  │
                                            │  │ sender: mpsc::Sender│ │
                                            │  │ metadata: HandlerMetadata│
                                            │  │  - capacity       │  │
                                            │  │  - queue_depth    │  │
                                            │  │  - inflight       │  │
                                            │  │  - semaphore: Opt │  │
                                            │  └───────────────────┘  │  │
                                            └────────────────────────┘  │
                                                        │
                                              ┌─────────┴─────────┐
                                              │ Python mpsc::Receiver │
                                              │ OwnedMessage pulled  │
                                              │ by Python handler    │
                                              │ (GilDrop or ack call) │
                                              └──────────────────────┘
```

**Data flow:**
1. `ConsumerDispatcher::run()` spawns and runs the consumer stream loop
2. Each `OwnedMessage` from the stream is sent via `dispatcher.send_with_policy(msg, &policy)`
3. If the policy returns `FuturePausePartition(topic)`, `consumer.pause(partitions)` is called
4. `Dispatcher` routes to the correct handler channel via `QueueManager`
5. Python receives messages via `mpsc::Receiver<OwnedMessage>` returned from `register_handler`

### Recommended Project Structure

```
src/
├── consumer/
│   ├── mod.rs
│   ├── runner.rs         # UNCHANGED - ConsumerRunner, ConsumerStream, ConsumerTask
│   ├── message.rs        # UNCHANGED - OwnedMessage
│   └── error.rs          # UNCHANGED - ConsumerError
├── dispatcher/
│   ├── mod.rs            # MODIFIED - add ConsumerDispatcher struct
│   ├── error.rs          # UNCHANGED - DispatchError
│   ├── backpressure.rs   # UNCHANGED - BackpressurePolicy, BackpressureAction
│   └── queue_manager.rs   # MODIFIED - add semaphore support
└── lib.rs                # MODIFIED - re-export ConsumerDispatcher
```

**New in `dispatcher/mod.rs`:**
```rust
pub struct ConsumerDispatcher {
    runner: ConsumerRunner,
    dispatcher: Dispatcher,
    partition_handles: Mutex<HashMap<String, TopicPartitionList>>,
    paused_topics: Set<String>,
}

impl ConsumerDispatcher {
    pub fn new(runner: ConsumerRunner, dispatcher: Dispatcher) -> Self { ... }
    pub async fn run(&self, policy: &dyn BackpressurePolicy) { ... }
    fn pause_partition(&self, topic: &str) -> Result<(), ConsumerError> { ... }
    fn resume_partition(&self, topic: &str) -> Result<(), ConsumerError> { ... }
}
```

### Pattern 1: Stream Integration Loop

**What:** An async loop that polls `ConsumerStream` and dispatches each message.

**When to use:** Any time a stream needs to feed into a channel-based handler system.

```rust
// Source: tokio_stream::StreamExt usage pattern
async fn run(&self, policy: &dyn BackpressurePolicy) {
    let mut stream = self.runner.stream();
    while let Some(result) = stream.next().await {
        match result {
            Ok(msg) => {
                match self.dispatcher.send_with_policy(msg, policy) {
                    Ok(outcome) => {
                        // Check if resume is needed (queue draining)
                        self.check_resume(&outcome.topic);
                    }
                    Err(DispatchError::Backpressure(topic)) => {
                        // Policy already applied in send_with_policy
                        // Log and continue
                    }
                    Err(DispatchError::HandlerNotRegistered(_)) => {
                        // Unknown topic — skip non-fatally
                    }
                    Err(e) => {
                        tracing::error!("dispatch error: {}", e);
                    }
                }
            }
            Err(e) => {
                tracing::error!("consumer error: {}", e);
                // Non-fatal — continue polling
            }
        }
    }
}
```

### Pattern 2: Optional Semaphore per Handler

**What:** Add `Option<Arc<Semaphore>>` to `HandlerMetadata`. If `Some(sem)`, dispatch acquires a permit before sending.

**Why:** Tokio `Semaphore` is the standard way to limit concurrent operations in async code. Using it here avoids a custom counter.

```rust
// In HandlerMetadata
pub struct HandlerMetadata {
    pub capacity: usize,
    pub(crate) queue_depth: AtomicUsize,
    pub(crate) inflight: AtomicUsize,
    pub(crate) semaphore: Option<Arc<Semaphore>>,  // NEW
}

// On dispatch (pseudo-code):
if let Some(sem) = &entry.metadata.semaphore {
    let permit = sem.acquire().await.unwrap(); // or try_acquire for non-blocking
    // send message
    // permit is held until ack() — ok since ack is called from Python
}
// If semaphore is None, send directly (no concurrency limit)
```

**Important:** `Semaphore::acquire()` is async and blocking on the semaphore itself, not on the channel. Since `try_send` is non-blocking, the semaphore should use `try_acquire()` to avoid stalling the consumer loop. If no permit is available, return `DispatchError::Backpressure`.

### Pattern 3: Pause/Resume via rdkafka

**What:** When `BackpressureAction::FuturePausePartition(topic)` occurs, call `consumer.pause(partitions)`. Resume when the queue drains below a threshold.

**Why:** rdkafka's `StreamConsumer` supports `pause()` and `resume()` on assigned partitions.

```rust
// Source: rdkafka::consumer::StreamConsumer docs
use rdkafka::consumer::Consumer;

fn pause_partition(&self, topic: &str) -> Result<(), ConsumerError> {
    let mut tpl = TopicPartitionList::new();
    tpl.add_topic_partition(topic, rdkafka::Partition::ANY);
    self.consumer.pause(&tpl)
}

fn resume_partition(&self, topic: &str) -> Result<(), ConsumerError> {
    let mut tpl = TopicPartitionList::new();
    tpl.add_topic_partition(topic, rdkafka::Partition::ANY);
    self.consumer.resume(&tpl)
}
```

**Drain threshold:** Resume when `queue_depth` drops below `capacity * 0.5` (50% of capacity). This hysteresis prevents thrashing.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Concurrency limiting | Custom atomic counter for max in-flight | `tokio::sync::Semaphore` | Standard, fair, async-aware |
| Stream polling | Custom `Future` implementation | `StreamExt::next()` in a `while let` loop | Idiomatic, no state machine needed |
| Pause tracking | Store pause state in Dispatcher | Separate `PausedTopics` set alongside consumer reference | Clear separation of concerns |
| Partition handle storage | Pass consumer into Dispatcher | Store `Arc<StreamConsumer>` in `ConsumerDispatcher` only | Python boundary stays clean; Dispatcher stays sync |

---

## Common Pitfalls

### Pitfall 1: Semaphore blocking the consumer loop

**What goes wrong:** Using `sem.acquire().await` in the dispatch loop blocks the entire loop if no permit is available, since `Semaphore::acquire` waits.

**Why it happens:** `acquire()` suspends the task until a permit is available. In a single-threaded executor or under heavy load, this can stall the consumer.

**How to avoid:** Use `try_acquire()` (non-blocking). If no permit, return `Backpressure` immediately. The Python handler acknowledging messages will release permits.

### Pitfall 2: Pause without storing partition assignment

**What goes wrong:** Calling `consumer.pause()` on a topic without first populating the `TopicPartitionList` with actual assigned partitions causes an error.

**Why it happens:** `pause()` requires the `TopicPartitionList` to contain the partitions that are currently assigned to this consumer.

**How to avoid:** Populate `partition_handles` when `on_assign` callback fires (via `ConsumerConfig::set_on_assign`). Store the assigned `TopicPartitionList` keyed by topic.

### Pitfall 3: Backpressure signal not consumed

**What goes wrong:** `send_with_policy` returns `Err(Backpressure)` when the queue is full, but the caller in the loop ignores the signal and continues polling.

**Why it happens:** The `FuturePausePartition` action is returned as part of `BackpressureAction`, but in `send_with_policy`, it currently maps to `Err(Backpressure)` without actually triggering a pause.

**How to avoid:** Check the `BackpressureAction` returned from `send_with_policy` and call `pause_partition()` if it is `FuturePausePartition`. The current implementation returns `Err(Backpressure)` but the action information is lost. This needs fixing: `send_with_policy` should return `Result<(DispatchOutcome, Option<BackpressureAction>), DispatchError>` or the action should be communicated differently.

**Warning signs:** Messages pile up, consumer is slow, but no pause is triggered even though `PauseOnFullPolicy` is configured.

### Pitfall 4: Python receiver disconnected but consumer still running

**What goes wrong:** If Python drops the `Receiver<OwnedMessage>` for a topic, the sender's `try_send` will succeed but messages go into a closed channel (silently dropped on the floor). The consumer keeps running and dispatching to other topics.

**Why it happens:** `mpsc::Sender::try_send` on a closed channel returns `Err(TrySendError::Closed(_))`, which the dispatcher handles. But if the Python receiver is dropped without closing the sender (e.g., Python keeps the sender alive but stops reading), messages pile up.

**How to avoid:** Ensure Python explicitly closes handlers or the dispatcher tracks when a handler's receiver is dropped. Currently `QueueManager` handles `TrySendError::Closed` but this is only triggered if Python explicitly drops the receiver.

---

## Code Examples

### ConsumerDispatcher struct

```rust
// New in src/dispatcher/mod.rs

use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::TopicPartitionList;
use std::collections::{HashMap, HashSet};
use tokio_stream::StreamExt;

/// Owns ConsumerRunner + Dispatcher and orchestrates the async message loop.
/// Wires consumer stream output to dispatcher input, handles pause/resume.
pub struct ConsumerDispatcher {
    runner: Arc<ConsumerRunner>,
    dispatcher: Dispatcher,
    /// Partition list for each topic, populated via on_assign callback
    partition_handles: parking_lot::Mutex<HashMap<String, TopicPartitionList>>,
    /// Topics currently paused
    paused_topics: parking_lot::Mutex<HashSet<String>>,
    /// Backpressure threshold ratio for resume (0.0 to 1.0)
    resume_threshold: f64,
}

impl ConsumerDispatcher {
    /// Creates a new dispatcher wired to the given runner.
    pub fn new(runner: ConsumerRunner) -> Self {
        Self {
            runner: Arc::new(runner),
            dispatcher: Dispatcher::new(),
            partition_handles: parking_lot::Mutex::new(HashMap::new()),
            paused_topics: parking_lot::Mutex::new(HashSet::new()),
            resume_threshold: 0.5,
        }
    }

    /// Registers a handler for `topic` with bounded queue of `capacity`.
    /// Optionally limits concurrency with `max_concurrency` permits.
    pub fn register_handler(
        &self,
        topic: impl Into<String>,
        capacity: usize,
        max_concurrency: Option<usize>,
    ) -> mpsc::Receiver<OwnedMessage> {
        self.dispatcher.register_handler(topic, capacity, max_concurrency)
    }

    /// Runs the dispatch loop, polling the consumer stream and
    /// dispatching each message through the dispatcher.
    pub async fn run(&self, policy: &dyn BackpressurePolicy) {
        let mut stream = self.runner.stream();
        while let Some(result) = stream.next().await {
            match result {
                Ok(msg) => {
                    let topic = msg.topic.clone();
                    match self.dispatcher.send_with_policy(msg, policy) {
                        Ok(outcome) => {
                            self.check_resume(&topic, outcome.queue_depth);
                        }
                        Err(DispatchError::Backpressure(topic)) => {
                            tracing::warn!("backpressure on topic '{}'", topic);
                            // Pause is triggered inside send_with_policy via the policy
                        }
                        Err(DispatchError::HandlerNotRegistered(topic)) => {
                            tracing::debug!("no handler for topic '{}', skipping", topic);
                        }
                        Err(e) => {
                            tracing::error!("dispatch error: {}", e);
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("consumer error: {}", e);
                    // Non-fatal — continue polling
                }
            }
        }
    }

    /// Checks if a paused topic should be resumed based on current queue depth.
    fn check_resume(&self, topic: &str, current_depth: usize) {
        let capacity = self.dispatcher.queue_manager().get_capacity(topic);
        let threshold = (capacity as f64 * self.resume_threshold) as usize;

        if current_depth < threshold {
            if self.paused_topics.lock().remove(topic) {
                if let Err(e) = self.resume_partition(topic) {
                    tracing::error!("failed to resume topic '{}': {}", topic, e);
                }
            }
        }
    }

    fn pause_partition(&self, topic: &str) -> Result<(), ConsumerError> {
        let handles = self.partition_handles.lock();
        if let Some(tpl) = handles.get(topic) {
            self.runner.consumer().pause(tpl)
        } else {
            Err(ConsumerError::Subscription(format!(
                "no partition handle for topic '{}'",
                topic
            )))
        }
    }

    fn resume_partition(&self, topic: &str) -> Result<(), ConsumerError> {
        let handles = self.partition_handles.lock();
        if let Some(tpl) = handles.get(topic) {
            self.runner.consumer().resume(tpl)
        } else {
            Err(ConsumerError::Subscription(format!(
                "no partition handle for topic '{}'",
                topic
            )))
        }
    }

    /// Returns a reference to the underlying dispatcher for inspection.
    pub fn dispatcher(&self) -> &Dispatcher {
        &self.dispatcher
    }
}
```

### Dispatcher::register_handler with optional semaphore

```rust
// In Dispatcher or QueueManager
pub fn register_handler(
    &self,
    topic: impl Into<String>,
    capacity: usize,
    max_concurrency: Option<usize>,
) -> mpsc::Receiver<OwnedMessage> {
    let semaphore = max_concurrency.map(|n| Arc::new(Semaphore::new(n)));
    self.queue_manager.register_handler_with_semaphore(topic, capacity, semaphore)
}
```

### Pause/resume in send_with_policy

```rust
// Current code returns Err(Backpressure) for FuturePausePartition — this needs
// to be changed so the caller can distinguish pause signal from drop.

impl Dispatcher {
    /// Returns (outcome, pause_signal) where pause_signal is Some(topic) if
    /// the policy returned FuturePausePartition.
    pub(crate) fn send_with_policy_and_signal(
        &self,
        message: OwnedMessage,
        policy: &dyn BackpressurePolicy,
    ) -> (Result<DispatchOutcome, DispatchError>, Option<String>) {
        let topic = message.topic.clone();
        // ... same send logic ...
        match entry.sender.try_send(message) {
            Ok(()) => {
                entry.metadata.queue_depth.fetch_add(1, Ordering::Relaxed);
                (Ok(DispatchOutcome { ... }), None)
            }
            Err(TrySendError::Full(_)) => {
                entry.metadata.inflight.fetch_sub(1, Ordering::Relaxed);
                let action = policy.on_queue_full(&topic, &entry.metadata);
                match action {
                    BackpressureAction::Drop | BackpressureAction::Wait => {
                        (Err(DispatchError::Backpressure(topic.clone())), None)
                    }
                    BackpressureAction::FuturePausePartition(_) => {
                        (Err(DispatchError::Backpressure(topic.clone())), Some(topic))
                    }
                }
            }
            Err(TrySendError::Closed(_)) => {
                (Err(DispatchError::QueueClosed(topic)), None)
            }
        }
    }
}
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Consumer stream not integrated with Dispatcher | `ConsumerDispatcher` orchestrates stream-to-queue dispatch | Phase 8 | Messages now flow from Kafka to per-handler queues |
| Backpressure only returns error | Backpressure returns error + pause signal | Phase 8 | Enables actual pause/resume via rdkafka |
| No concurrency limiting | Optional `Semaphore` per handler | Phase 8 (DISP-15) | Limits concurrent processing per topic |

---

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `StreamConsumer::pause()/resume()` can be called on any assigned partition at any time | DISP-18 | If rdkafka requires specific thread safety guarantees, the pause/resume implementation needs synchronization |
| A2 | `FuturePausePartition` is returned alongside `Err(Backpressure)` without changing the error type | DISP-18 | If user wants a distinct error type, plan must add a new variant |
| A3 | `max_concurrency: Option<usize>` passed at `register_handler` time is sufficient — no dynamic update | DISP-15 | If dynamic update is needed, `HandlerMetadata` needs a setter |
| A4 | Python side holds `Receiver<OwnedMessage>` and calls `recv()` in a loop | DISP-17 | If Python uses a callback model, the integration approach differs |

**If this table is empty:** All claims in this research were verified or cited — no user confirmation needed.

---

## Open Questions

1. **How does Python actually receive messages?** Current `pyconsumer.rs` uses callbacks (`handler.call1(py, (py_msg_clone,))`). Does DISP-16 require a push model (Python pulls from `mpsc::Receiver`) or is the callback model being preserved? The `Dispatcher::register_handler` returns a `Receiver`, but `pyconsumer` uses callbacks. Clarification needed: does Phase 8 change the Python consumer to use the dispatcher's receiver, or is the dispatcher for future Python consumers?

2. **`on_assign` callback for partition handles:** Pause/resume needs actual partition assignments. Does the `ConsumerConfig` already set an `on_assign` callback? The current `runner.rs` does not show this. It needs to be added to populate `partition_handles` in `ConsumerDispatcher`.

3. **DISP-15 Semaphore blocking:** `Semaphore::acquire()` is awaitable but blocks the task. If a handler has `max_concurrency = 5` and all 5 are held, the next dispatch will suspend. Should we use `try_acquire()` with immediate `Backpressure` return, or is some waiting acceptable?

4. **Ack from Python:** The `QueueManager::ack()` method exists but the current `pyconsumer.rs` does not call it. Does Phase 8 need to wire up the ack call from Python, or is that a future phase?

---

## Environment Availability

> Step 2.6: SKIPPED (no external dependencies beyond existing project)

All dependencies (`tokio`, `rdkafka`, `thiserror`, `parking_lot`) are already in the project's `Cargo.toml`.

---

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | `#[tokio::test]` (built-in tokio) |
| Config file | none — existing `Cargo.toml` |
| Quick run command | `cargo test --lib -- dispatcher::consumer_dispatcher` |
| Full suite command | `cargo test` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| DISP-15 | `Semaphore::try_acquire` returns `Backpressure` when no permit | unit | `cargo test --lib -- dispatcher::semaphore` | NO - new test |
| DISP-16 | `ConsumerDispatcher::run()` dispatches messages from stream | unit | `cargo test --lib -- dispatcher::run_dispatches` | NO - new test |
| DISP-16 | Unknown topic returns `HandlerNotRegistered` and continues | unit | `cargo test --lib -- dispatcher::unknown_topic_continues` | NO - new test |
| DISP-17 | `OwnedMessage` has no lifetimes | unit | compile-fail test with `&'a` annotation | NO - new test |
| DISP-18 | `FuturePausePartition` triggers `consumer.pause()` | unit | `cargo test --lib -- dispatcher::pause_on_full` | NO - new test |
| DISP-18 | Resume triggers when queue_depth < threshold | unit | `cargo test --lib -- dispatcher::resume_on_drain` | NO - new test |

### Sampling Rate
- **Per task commit:** `cargo test --lib -- dispatcher::consumer_dispatcher`
- **Per wave merge:** `cargo test`
- **Phase gate:** Full suite green before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `src/dispatcher/consumer_dispatcher.rs` — new file: `ConsumerDispatcher`, `ConsumerDispatcher::run()`, `pause/resume`
- [ ] `src/dispatcher/mod.rs` — add `ConsumerDispatcher` re-export and `Dispatcher::register_handler(capacity, max_concurrency)` overload
- [ ] `src/dispatcher/queue_manager.rs` — add `semaphore: Option<Arc<Semaphore>>` to `HandlerMetadata`
- [ ] `src/dispatcher/backpressure.rs` — confirm `FuturePausePartition` carries `String` topic
- [ ] `tests/consumer_dispatcher_test.rs` — covers DISP-15 through DISP-18

---

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V5 Input Validation | no | Not applicable — Kafka messages are internal domain objects |
| V4 Access Control | no | No authorization in dispatcher |
| V2 Authentication | no | No auth in dispatcher |
| V3 Session Management | no | No session state |

**Security enforcement:** Disabled for this phase (dispatcher is a routing layer with no user-facing attack surface).

---

## Sources

### Primary (HIGH confidence)
- Project existing code: `src/consumer/runner.rs`, `src/dispatcher/mod.rs`, `src/dispatcher/queue_manager.rs`, `src/dispatcher/backpressure.rs`
- `tokio::sync::Semaphore` — built-in Tokio, documented via tokio crate docs
- `rdkafka::consumer::StreamConsumer::pause()` — via `rdkafka` crate API

### Secondary (MEDIUM confidence)
- `tokio_stream::StreamExt::next()` — standard async stream polling pattern

### Tertiary (LOW confidence)
- None

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new dependencies, all verified via existing project `Cargo.toml`
- Architecture: HIGH — `ConsumerDispatcher` pattern is straightforward composition of existing components
- Pitfalls: HIGH — identified via analysis of tokio semaphore semantics and rdkafka pause API

**Research date:** 2026-04-16
**Valid until:** 2026-05-16 (30 days — stable Rust async patterns, rdkafka API is mature)
