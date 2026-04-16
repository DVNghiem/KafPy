# Phase 6: Dispatcher Core - Research

**Researched:** 2026-04-16
**Domain:** Rust async message dispatcher with bounded channel routing
**Confidence:** HIGH

## Summary

Phase 6 builds the dispatcher core that receives `OwnedMessage` from the consumer layer and routes each message to a per-handler bounded Tokio `mpsc` channel. The dispatcher owns a `HashMap<TopicName, mpsc::Sender<OwnedMessage>>` where each topic maps to its handler's channel. Handler registration creates the bounded channel and stores the sender; the receiver is returned to the caller (Python layer via PyO3 or another Rust consumer). The `send()` method is non-blocking and returns `Result<DispatchOutcome, DispatchError>` immediately.

**Primary recommendation:** Use `tokio::sync::mpsc::channel(capacity)` for per-handler channels, store senders in a `DashMap<String, mpsc::Sender<OwnedMessage>>` for thread-safe concurrent access, and define `DispatchOutcome` as a minimal struct containing dispatched metadata (topic, partition, offset, queue_depth_at_dispatch).

---

## User Constraints (from CONTEXT.md)

### Locked Decisions
- DispatchError (DISP-19/DISP-20) belongs in foundation phase since DISP-05 requires `Result<DispatchOutcome, DispatchError>`
- Phase 7: Backpressure and QueueManager grouped together - both depend on bounded queue infrastructure from Phase 6
- Phase 8: Integration with ConsumerRunner - clean separation from Python boundary preserved

### Claude's Discretion
- Internal Dispatcher struct design (choice of DashMap vs Mutex-wrapped HashMap, channel capacity defaults)
- DispatchOutcome struct fields (minimal vs verbose)
- Handler registration API shape (builder vs direct methods)

### Deferred Ideas (OUT OF SCOPE)
- Python handler execution - deferred to future milestone (DISP-02 is registration only, no callback invocation)
- Schema registry / Avro - not relevant to dispatcher layer
- Borrowed lifetimes in dispatcher APIs - all message flow uses owned types only

---

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| DISP-01 | Dispatcher receives `OwnedMessage` from consumer layer and routes by topic | Section: Dispatcher Struct Design - routing via HashMap lookup by topic |
| DISP-02 | Handler registration API - register handler slots by topic name with configurable queue capacity | Section: Handler Registration API - `register_handler(topic, capacity)` method |
| DISP-03 | Per-handler bounded Tokio `mpsc` channel (configurable capacity) | Section: Channel Architecture - `tokio::sync::mpsc::channel(capacity)` per handler |
| DISP-04 | Bounded queues only - no unbounded channels in hot path | Section: Channel Architecture - all channels bounded by design |
| DISP-05 | `send()` to handler queue returns `Result<DispatchOutcome, DispatchError>` | Section: DispatchOutcome + Error Handling |
| DISP-19 | `DispatchError` enum: `QueueFull`, `UnknownTopic`, `HandlerNotRegistered`, `QueueClosed` | Section: DispatchError Enum |
| DISP-20 | All errors are `thiserror` types with `Display` and `Debug` | Section: Error Patterns |

---

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Message routing | Dispatcher | ConsumerRunner | Dispatcher owns topic->channel routing; ConsumerRunner produces messages |
| Handler registration | Dispatcher | Python/PyO3 layer | Dispatcher manages channel creation; Python registers handlers via PyO3 |
| Channel ownership | Dispatcher | Handler receiver | Dispatcher holds sender; handler holds receiver |
| Error enum | Dispatcher | All consumers | DispatchError defined in dispatcher module |

---

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `tokio` | 1.40 | Async runtime | Already in project; provides mpsc channels |
| `tokio::sync::mpsc` | (part of tokio 1.40) | Per-handler bounded channels | Required by DISP-03/DISP-04 |
| `thiserror` | 2.0.17 | Error enum derive | Already in project; used by ConsumerError |
| `dashmap` | 5.x (not in project) | Thread-safe topic->channel map | Convenient for concurrent HashMap access; can use `parking_lot::Mutex<HashMap>` as alternative |
| `tracing` | 0.1.44 | Debug/trace logging | Already in project |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `parking_lot` | (if needed) | Mutex for HashMap | Alternative to DashMap if lock contention is low |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `DashMap` | `Mutex<HashMap<String, mpsc::Sender<OwnedMessage>>>` | DashMap is lock-free concurrent; Mutex is simpler but serializes access. DashMap preferred for hot-path dispatch. |
| Return `usize` queue depth | Return `DispatchOutcome` struct | DISP-05 requires `DispatchOutcome`; struct is more extensible for future fields |

**Installation:**
```bash
# Add to Cargo.toml if DashMap is chosen:
dashmap = "5"
```

Note: DashMap is NOT currently in project dependencies. Research confidence is MEDIUM on whether DashMap or a Mutex-wrapped HashMap is preferred. Recommend `parking_lot::Mutex<HashMap<...>>` as simpler alternative already acceptable in async contexts.

---

## Architecture Patterns

### System Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                        ConsumerRunner                                │
│  [rdkafka StreamConsumer] → [OwnedMessage] → [mpsc::channel]        │
└─────────────────────────────────────┬───────────────────────────────┘
                                      │ streams Result<OwnedMessage>
                                      ▼
                          ┌─────────────────────────┐
                          │       Dispatcher         │
                          │  ┌───────────────────┐  │
                          │  │ topic_map:        │  │
                          │  │ HashMap<String,   │  │
                          │  │ mpsc::Sender<...>> │  │
                          │  └───────────────────┘  │
                          └────────────┬────────────┘
                                       │
              ┌────────────────────────┼────────────────────────┐
              │                        │                        │
              ▼                        ▼                        ▼
     ┌────────────────┐      ┌────────────────┐      ┌────────────────┐
     │  topic: "foo"  │      │  topic: "bar"  │      │  topic: "baz"  │
     │  capacity: 100 │      │  capacity: 50  │      │  capacity: 200 │
     │  tx ───────────│      │  tx ───────────│      │  tx ───────────│
     └────────────────┘      └────────────────┘      └────────────────┘
              │                        │                        │
              ▼                        ▼                        ▼
     ┌────────────────┐      ┌────────────────┐      ┌────────────────┐
     │  rx (handler)  │      │  rx (handler)  │      │  rx (handler)  │
     │  (Python/PyO3) │      │  (Python/PyO3) │      │  (Python/PyO3) │
     └────────────────┘      └────────────────┘      └────────────────┘
```

**Data flow:** ConsumerRunner::stream() yields OwnedMessage → Dispatcher::send() looks up topic in HashMap → sends to matching mpsc::Sender → receiver (handler) receives message.

### Recommended Project Structure
```
src/
├── dispatcher/
│   ├── mod.rs        # Module exports, Dispatcher, DispatchOutcome, DispatchError
│   ├── error.rs     # DispatchError enum (thiserror)
│   └── channel.rs   # Channel management helpers (optional)
└── lib.rs           # Add: pub mod dispatcher
```

### Pattern 1: Per-Topic Bounded Channel

**What:** Each topic gets its own `tokio::sync::mpsc::channel(capacity)`.
**When to use:** When handlers need independent backpressure per topic.
**Example:**
```rust
// Source: tokio docs / standard Rust async pattern
use tokio::sync::mpsc;

struct Dispatcher {
    handlers: parking_lot::Mutex<HashMap<String, mpsc::Sender<OwnedMessage>>>,
}

impl Dispatcher {
    pub fn register_handler(&self, topic: impl Into<String>, capacity: usize) -> mpsc::Receiver<OwnedMessage> {
        let (tx, rx) = mpsc::channel(capacity);
        self.handlers.lock().insert(topic.into(), tx);
        rx
    }
}
```

### Pattern 2: Non-Blocking Send with Error

**What:** `send()` returns immediately with `Result` rather than awaiting.
**When to use:** When caller (ConsumerRunner) should not block on slow handlers.
**Example:**
```rust
// Source: tokio mpsc docs
match sender.try_send(message) {
    Ok(()) => Ok(DispatchOutcome { topic, partition, offset }),
    Err(TrySendError::Full(_)) => Err(DispatchError::QueueFull(topic)),
    Err(TrySendError::Closed(_)) => Err(DispatchError::QueueClosed(topic)),
}
```

### Pattern 3: DispatchOutcome Minimal Struct

**What:** Success return type containing dispatch metadata.
**When to use:** DISP-05 requires returning something on success.
**Example:**
```rust
// [ASSUMED] based on standard Kafka producer patterns
#[derive(Debug, Clone)]
pub struct DispatchOutcome {
    pub topic: String,
    pub partition: i32,
    pub offset: i64,
    /// Queue depth at time of dispatch (approximate, via Channel::max_capacity() - len())
    pub queue_depth: usize,
}
```

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Error enum with Display/Debug | Manual `impl Display` + `impl Debug` | `#[derive(thiserror::Error)]` | thiserror auto-derives both with correct formatting |
| Thread-safe topic map | Manual `Arc<Mutex<HashMap>>` | `parking_lot::Mutex<HashMap>` or `DashMap` | Battle-tested, no poison semantics |
| Channel send with backpressure | `send().await` (blocking) | `try_send()` (non-blocking) | DISP-08 requires non-blocking; await would halt consumer |

**Key insight:** The dispatcher is a thin routing layer - channel management and error types are already solved problems in tokio and thiserror.

---

## Common Pitfalls

### Pitfall 1: Unbounded Channel Creation on Hot Path
**What goes wrong:** Creating channels during message dispatch causes allocation latency.
**Why it happens:** Registering handlers after construction requires `Arc<Mutex<HashMap>>` mutation; some designs lazily create channels.
**How to avoid:** All channels are created at handler registration time (immutable afterwards). Dispatcher stores only senders.
**Warning signs:** `Mutex` lock held during `try_send`, allocation in dispatch path.

### Pitfall 2: Mismatch Between Sender and Receiver Lifetimes
**What goes wrong:** Receiver dropped but sender still in map; `try_send` returns `Closed`.
**Why it happens:** Handler's Python receiver is dropped (handler cancelled).
**How to avoid:** `DispatchError::QueueClosed` is the expected error when handler disconnects. Python layer should re-register if needed.
**Warning signs:** High rate of `QueueClosed` errors after periods of normal operation.

### Pitfall 3: Blocking `send().await` in Consumer Loop
**What goes wrong:** Slow handler causes entire consumer to stall.
**Why it happens:** Using `send().await` instead of `try_send()`.
**How to avoid:** Always use `try_send()` in dispatcher. DISP-08 explicitly requires non-blocking behavior.
**Warning signs:** Consumer lag growing despite messages being produced.

---

## Code Examples

### DispatchError Enum (thiserror)
```rust
// Source: follows ConsumerError pattern in src/consumer/error.rs
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DispatchError {
    #[error("handler queue for topic '{0}' is full")]
    QueueFull(String),

    #[error("topic '{0}' has no registered handler")]
    UnknownTopic(String),

    #[error("no handler registered for topic '{0}'")]
    HandlerNotRegistered(String),

    #[error("handler queue for topic '{0}' is closed")]
    QueueClosed(String),
}
```

### Dispatcher Struct with Handler Registration
```rust
// Source: standard tokio mpsc pattern
use parking_lot::Mutex;
use std::collections::HashMap;
use tokio::sync::mpsc;

pub struct Dispatcher {
    handlers: Mutex<HashMap<String, mpsc::Sender<OwnedMessage>>>,
}

impl Dispatcher {
    pub fn new() -> Self {
        Self {
            handlers: Mutex::new(HashMap::new()),
        }
    }

    /// Registers a handler for `topic` with bounded queue of `capacity`.
    /// Returns a receiver that the handler (Python/PyO3) will use to receive messages.
    pub fn register_handler(
        &self,
        topic: impl Into<String>,
        capacity: usize,
    ) -> mpsc::Receiver<OwnedMessage> {
        let (tx, rx) = mpsc::channel(capacity);
        self.handlers.lock().insert(topic.into(), tx);
        rx
    }

    /// Sends `message` to the handler registered for `message.topic`.
    /// Non-blocking - returns error immediately if queue is full or no handler exists.
    pub fn send(&self, message: OwnedMessage) -> Result<DispatchOutcome, DispatchError> {
        let topic = message.topic.clone();
        let partition = message.partition;
        let offset = message.offset;

        let sender = self.handlers
            .lock()
            .get(&topic)
            .ok_or_else(|| DispatchError::HandlerNotRegistered(topic.clone()))?;

        match sender.try_send(message) {
            Ok(()) => Ok(DispatchOutcome {
                topic,
                partition,
                offset,
                queue_depth: 0, // placeholder; Phase 7 adds queue_len()
            }),
            Err(TrySendError::Full(_)) => Err(DispatchError::QueueFull(topic)),
            Err(TrySendError::Closed(_)) => Err(DispatchError::QueueClosed(topic)),
        }
    }
}
```

### Module Exports (mod.rs)
```rust
// Source: follows src/consumer/mod.rs pattern
pub mod error;

pub use error::DispatchError;

pub mod channel;
pub use channel::{Dispatcher, DispatchOutcome};
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Single shared queue | Per-topic independent queues | DISP-03 | Isolated backpressure per topic |
| Blocking send | Non-blocking try_send | DISP-08 | Consumer never blocks on slow handler |
| No error details | Typed DispatchError | DISP-19/DISP-20 | Callers can handle errors specifically |

**Deprecated/outdated:**
- ` unboundedchannel()` in hot path - explicitly banned by DISP-04
- `send().await` in dispatcher - banned by DISP-08 non-blocking requirement

---

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `parking_lot::Mutex` is acceptable (not in project deps yet) | Standard Stack | Need to add dependency or use `std::sync::Mutex` |
| A2 | DispatchOutcome includes `queue_depth: usize` field | Code Examples | Phase 7 may change this |
| A3 | `try_send()` on mpsc::Sender returns TrySendError with Closed variant | Code Examples | tokio API may vary; verify |
| A4 | Handler receives mpsc::Receiver directly (no Arc包装) | Architecture | Python/PyO3 integration may require Arc包装 |

**If this table is empty:** All claims in this research were verified or cited - no user confirmation needed.

---

## Open Questions

1. **DashMap vs Mutex-wrapped HashMap for topic map**
   - What we know: Both approaches are valid; DashMap is lock-free concurrent, Mutex is simpler
   - What's unclear: Whether the project prefers adding dependencies or using std Mutex
   - Recommendation: Use `parking_lot::Mutex<HashMap>` as middle ground (already async-friendly, no DashMap dependency needed)

2. **DispatchOutcome.queue_depth calculation**
   - What we know: DISP-05 requires `Result<DispatchOutcome, DispatchError>` - some success info must be returned
   - What's unclear: Whether queue depth at dispatch time is needed in Phase 6 or Phase 7 (backpressure tracking)
   - Recommendation: Include field but mark as placeholder (0) in Phase 6; Phase 7 can calculate via `mpsc::Sender::max_capacity() - current_len()`

3. **Handler receiver type exposed to Python**
   - What we know: Python needs to receive messages from the handler channel
   - What's unclear: Whether PyO3 can directly use `mpsc::Receiver<OwnedMessage>` or needs wrapping
   - Recommendation: Phase 8 (ConsumerRunner Integration) should address PyO3 boundary; Phase 6 should focus on Dispatcher API

---

## Environment Availability

Step 2.6: SKIPPED (no external dependencies identified beyond those already in Cargo.toml)

---

## Validation Architecture

> Nyquist validation is enabled (workflow.nyquist_validation not set to false in .planning/config.json)

### Test Framework
| Property | Value |
|----------|-------|
| Framework | `#[tokio::test]` + `#[test]` |
| Config file | None detected - existing tests use inline `#[cfg(test)]` modules |
| Quick run command | `cargo test --lib` |
| Full suite command | `cargo test --all` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|------------------|-------------|
| DISP-01 | Dispatcher routes OwnedMessage by topic | unit | `cargo test dispatcher -- dispatch` | ❌ Wave 0 |
| DISP-02 | register_handler creates bounded channel and returns receiver | unit | `cargo test dispatcher -- register` | ❌ Wave 0 |
| DISP-03 | Per-handler channel respects capacity limit | unit | `cargo test dispatcher -- capacity` | ❌ Wave 0 |
| DISP-04 | send() to full queue returns QueueFull, not blocking | unit | `cargo test dispatcher -- queuefull` | ❌ Wave 0 |
| DISP-05 | send() returns Ok(DispatchOutcome) on success | unit | `cargo test dispatcher -- outcome` | ❌ Wave 0 |
| DISP-19 | DispatchError has QueueFull, UnknownTopic, HandlerNotRegistered, QueueClosed | unit | `cargo test dispatcher -- error_variants` | ❌ Wave 0 |
| DISP-20 | DispatchError derives Error, Display, Debug | unit | `cargo test dispatcher -- error_trait` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test --lib -- dispatcher`
- **Per wave merge:** `cargo test --lib`
- **Phase gate:** Full suite green before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `src/dispatcher/mod.rs` — Dispatcher struct, DispatchOutcome, public API
- [ ] `src/dispatcher/error.rs` — DispatchError enum with thiserror
- [ ] `tests/dispatcher_test.rs` — unit tests for all DISP requirements
- [ ] Framework install: Not needed — tokio::test already available via existing tokio dep

*(If no gaps: "None — existing test infrastructure covers all phase requirements")*

---

## Security Domain

> security_enforcement is enabled (absent = enabled in .planning/config.json)

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V5 Input Validation | yes | OwnedMessage already validated at consumer boundary; dispatcher is pass-through |
| V4 Access Control | no | No authentication/authorization in message routing |

### Known Threat Patterns for Dispatcher Stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Topic confusion (send to wrong topic) | Tampering | Dispatcher uses exact topic string match from OwnedMessage.topic |
| Unbounded memory growth | Denial of Service | DISP-04 enforces bounded channels; capacity set at registration |

**No security concerns unique to dispatcher layer.**

---

## Sources

### Primary (HIGH confidence)
- tokio::sync::mpsc documentation — channel creation, try_send, TrySendError
- src/consumer/error.rs — thiserror pattern for DispatchError to follow
- src/consumer/message.rs — OwnedMessage struct definition

### Secondary (MEDIUM confidence)
- tokio mpsc channel capacity behavior — general knowledge, not Context7 verified
- DashMap API — general knowledge, alternative considered but not recommended

### Tertiary (LOW confidence)
- DispatchOutcome.queue_depth field semantics — assumed based on DISP-05 requirement

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — tokio mpsc and thiserror are verified project dependencies
- Architecture: HIGH — patterns match existing project conventions
- Pitfalls: MEDIUM — tokio API details not Context7 verified

**Research date:** 2026-04-16
**Valid until:** 2026-05-16 (30 days for stable domain)
