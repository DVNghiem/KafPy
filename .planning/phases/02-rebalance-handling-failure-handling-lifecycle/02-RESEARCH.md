# Phase 2: Rebalance Handling, Failure Handling & Lifecycle - Research

**Researched:** 2026-04-29
**Phase:** 02-rebalance-handling-failure-handling-lifecycle
**Requirements:** CORE-06, CORE-08, MSG-03, OFF-04, FAIL-01 through FAIL-07, LIFE-01 through LIFE-05, CONF-03
**Confidence:** MEDIUM-HIGH

## Summary

Phase 2 builds production-hardening on top of the Phase 1 core engine. The existing codebase already has substantial skeleton code: `FailureReason`/`FailureCategory` taxonomy (FAIL-01), `DefaultFailureClassifier` (FAIL-02), `RetryPolicy`/`RetrySchedule` with capped exponential backoff + jitter (FAIL-03/FAIL-04), `RetryCoordinator` state machine (FAIL-03/FAIL-04), `DlqRouter`/`DefaultDlqRouter` (FAIL-07), `DlqMetadata` envelope (FAIL-06), `SharedDlqProducer` (FAIL-05), `ShutdownCoordinator` 4-phase lifecycle (LIFE-01/LIFE-02), and `OffsetTracker` with `has_terminal` blocking (OFF-04). What remains is wiring this skeleton into the consumer loop, implementing the rebalance listener via rdkafka's `ConsumerContext` trait, wiring pause/resume (CORE-08), routing key-based fallback (MSG-03), completing SIGTERM handling (LIFE-04), and connecting rebalance-safe partition revocation (LIFE-05).

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|--------------|----------------|-----------|
| Rebalance listener (CORE-06) | API/Backend | — | rdkafka ConsumerContext callbacks fire in consumer thread |
| Pause/resume partitions (CORE-08) | API/Backend | — | rdkafka pause()/resume() on StreamConsumer |
| Key-based routing fallback (MSG-03) | API/Backend | — | RoutingChain already has KeyRouter slot; needs wiring in builder |
| Terminal failure blocking (OFF-04) | API/Backend | — | OffsetTracker.should_commit() already gates on has_terminal |
| Failure classification (FAIL-01/02) | API/Backend | — | DefaultFailureClassifier already exists; needs integration |
| Retry with backoff (FAIL-03/04) | API/Backend | — | RetryPolicy/RetrySchedule already exist; needs worker loop integration |
| DLQ routing (FAIL-05/06/07) | API/Backend | — | SharedDlqProducer/DlqRouter already exist; needs trigger wiring |
| Graceful shutdown (LIFE-01/02/03) | API/Backend | — | ShutdownCoordinator already exists; needs signal + drain ordering |
| SIGTERM handling (LIFE-04) | API/Backend | — | tokio signal handling in Runtime::run() entry point |
| Rebalance-safe partition handling (LIFE-05) | API/Backend | — | Commit before revoke in ConsumerContext callback |
| RetryConfig dataclass (CONF-03) | Frontend Server | — | kafpy/config.py already has RetryConfig; needs to_rust() wiring |

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
*(No CONTEXT.md found for this phase — all Phase 2 requirements are in scope)*

### Claude's Discretion
*(All Phase 2 requirements are in scope)*

### Deferred Ideas (OUT OF SCOPE)
- Batch handler support (PY-05) — Phase 4
- Observability metrics (OBS-01 through OBS-07) — Phase 4
- Schema registry integration (ADV-03) — out of scope per ROADMAP.md
</user_constraints>

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| CORE-06 | Rebalance listener with on_partitions_revoked/assigned callbacks | rdkafka ConsumerContext trait; ExistingConsumerContext pattern |
| CORE-08 | Pause/resume partitions for flow control | rdkafka Consumer::pause/resume; already in ConsumerRunner |
| MSG-03 | Key-based routing as fallback after topic routing | RoutingChain has KeyRouter slot (position 3); KeyRouter already implemented |
| OFF-04 | Terminal failure blocking (poison messages block partition progress) | OffsetTracker.has_terminal + should_commit() gating already implemented |
| FAIL-01 | Failure classification: Retryable, Terminal, NonRetryable | FailureReason/FailureCategory already implemented |
| FAIL-02 | Default failure classifier mapping Python exceptions | DefaultFailureClassifier already implemented |
| FAIL-03 | Capped exponential backoff with jitter for retries | RetryPolicy/RetrySchedule already implemented |
| FAIL-04 | Maximum retry attempts configuration | RetryPolicy.max_attempts already in config |
| FAIL-05 | DLQ handoff after retries exhausted or terminal failure | SharedDlqProducer.fire-and-forget channel already exists |
| FAIL-06 | DLQ metadata envelope | DlqMetadata already implemented |
| FAIL-07 | Default DLQ router: dlq_topic_prefix + original_topic | DefaultDlqRouter already implemented |
| LIFE-01 | Graceful shutdown with bounded drain timeout | ShutdownCoordinator 4-phase already implemented |
| LIFE-02 | Queue draining before shutdown | WorkerPool::shutdown() with drain timeout already implemented |
| LIFE-03 | Failed messages flushed to DLQ on shutdown | OffsetTracker.flush_failed_to_dlq() already implemented |
| LIFE-04 | SIGTERM handling matching Kubernetes pod termination | tokio signal::ctrl_c + ShutdownCoordinator wiring needed |
| LIFE-05 | Rebalance-safe partition handling | ConsumerContext rebalance callbacks + commit-before-revoke needed |
| CONF-03 | RetryConfig: max_attempts, base_delay, max_delay, jitter_factor | RetryConfig dataclass already in kafpy/config.py |
</phase_requirements>

---

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| rdkafka | 0.38 | Kafka client, consumer group, rebalance | Primary Kafka binding |
| tokio | 1.40 | Async runtime, signal handling | Required for async consumer loop |
| tokio-util | 0.7.17 | CancellationToken, watch channels | Shutdown coordination, commit signaling |
| parking_lot | 0.12 | Fast mutex for state | Phase 1 established parking_lot::Mutex |
| pyo3 | 0.27.2 | Python extension module | PyO3 already in use |
| rand | 0.8 | Jitter randomness | Already in retry/policy.rs |
| chrono | 0.4 | Timestamps for DLQ metadata | Already in use for DlqMetadata |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| signal-hook-registry | (via tokio) | SIGTERM handling | LIFE-04 only |
| thiserror | 2.0.17 | Error definitions | Already in use |
| tracing | 0.1 | Structured logging | Already in use |

**Installation:** N/A (existing Cargo.toml)

**Version verification:**
- rdkafka: 0.38.0 — `target/doc/rdkafka/index.html` confirmed rdkafka v0.38.0 in generated docs
- tokio: 1.40.0 — confirmed via Cargo.lock
- rand: 0.8.5 — confirmed via Cargo.lock

---

## Architecture Patterns

### System Architecture Diagram

```
Kafka Cluster
    │
    ▼
rdkafka StreamConsumer (subscribe → recv loop)
    │  ▲
    │  │ rebalance callbacks via CustomConsumerContext
    │  │
    │  ▼
ConsumerDispatcher (route OwnedMessage → handler queues)
    │
    ▼
WorkerPool (N Tokio workers poll mpsc::Receiver)
    │
    ├──▶ PythonHandler (spawn_blocking → Python callback)
    │        │
    │        │ on failure → RetryCoordinator.record_failure()
    │        │              ├── retryable → RetrySchedule.next_delay() → delay → retry
    │        │              ├── terminal/non-retryable → SharedDlqProducer.produce_async()
    │        │                                                        │
    │        │◀────── OffsetTracker.mark_failed() + has_terminal=true
    │        │
    │        └──▶ OffsetTracker.ack() → BTreeSet advance
    │
    └──▶ OffsetCommitter (watch channel → periodic store_offset + commit)

Shutdown Flow:
SIGTERM → ShutdownCoordinator.begin_draining()
  → dispatcher_cancel → ConsumerDispatcher stops recv
  → worker_cancel → WorkerPool.drain(drain_timeout)
  → flush_failed_to_dlq() → DLQ messages produced
  → committer_cancel → final offsets committed
  → ShutdownCoordinator.set_done()
```

### Recommended Project Structure

```
src/
├── consumer/
│   ├── mod.rs
│   ├── runner.rs         # ConsumerRunner (existing)
│   ├── context.rs         # NEW: CustomConsumerContext for rebalance
│   ├── config.rs
│   ├── message.rs
│   └── error.rs
├── coordinator/
│   ├── mod.rs             # (existing)
│   ├── shutdown.rs        # ShutdownCoordinator (existing)
│   └── offset_coordinator.rs
├── failure/
│   ├── mod.rs             # (existing)
│   ├── reason.rs          # FailureReason/FailureCategory (existing)
│   └── classifier.rs      # DefaultFailureClassifier (existing)
├── retry/
│   ├── mod.rs             # (existing)
│   ├── policy.rs          # RetryPolicy/RetrySchedule (existing)
│   └── retry_coordinator.rs # RetryCoordinator (existing)
├── dlq/
│   ├── mod.rs             # (existing)
│   ├── router.rs         # DlqRouter/DefaultDlqRouter (existing)
│   ├── metadata.rs       # DlqMetadata (existing)
│   └── produce.rs         # SharedDlqProducer (existing)
├── shutdown/
│   └── shutdown.rs       # (existing)
├── runtime/
│   ├── mod.rs
│   └── builder.rs         # RuntimeBuilder (existing, needs rebalance wiring)
└── lib.rs
```

### Pattern 1: rdkafka ConsumerContext Rebalance Callbacks

**What:** Custom `ConsumerContext` override for `partitions_revoked` and `partitions_assigned` callbacks.

**When to use:** CORE-06, LIFE-05 — must commit pending offsets before partition revocation.

**Example:**
```rust
// src/consumer/context.rs — NEW
use rdkafka::consumer::{ConsumerContext, RebalanceProtocol};
use rdkafka::{ClientConfig, Consumer, TopicPartitionList};
use std::sync::Arc;

pub struct KafPyConsumerContext {
    pub offset_tracker: Arc<dyn OffsetCoordinator>,
    pub dlq_router: Arc<dyn DlqRouter>,
    pub dlq_producer: Arc<SharedDlqProducer>,
    pub pause_state: parking_lot::Mutex<HashMap<String, bool>>,
}

impl ConsumerContext for KafPyConsumerContext {
    fn partitions_revoked(
        &self,
        consumer: &dyn Consumer,
        partitions: &TopicPartitionList,
    ) {
        // LIFE-05: Commit ALL pending offsets for revoked partitions BEFORE losing them
        // PITFALLS-4.3: Never commit during rebalance for EAGER strategies
        // For cooperative-sticky (cooperative-sticky): commit is safe here
        for (topic, partition) in self.iter_partitions(partitions) {
            if let Some(offset) = self.offset_tracker.highest_contiguous(topic, partition) {
                let _ = consumer.store_offset(topic, partition, offset);
                // Sync commit for revoked partitions — must be synchronous
                let _ = consumer.commit_consumer_state(rdkafka::consumer::CommitMode::Sync);
            }
        }
    }

    fn partitions_assigned(
        &self,
        consumer: &dyn Consumer,
        partitions: &TopicPartitionList,
    ) {
        // Seek to committed offset for each assigned partition
        for (topic, partition) in self.iter_partitions(partitions) {
            if let Some(committed) = self.offset_tracker.committed_offset(topic, partition) {
                // PITFALLS-4.1: seek to committed_offset + 1
                let _ = consumer.seek(topic, partition, committed + 1, 0);
            }
        }
    }
}
```

**Key insight:** Using `BaseConsumer` + `ConsumerContext` subtype requires `CustomConsumerContext` to wrap `StreamConsumer`. The rebalance callbacks fire synchronously in the rdkafka client thread — never do blocking I/O or Python calls here.

### Pattern 2: SIGTERM Handling with tokio

**What:** Install a SIGTERM handler that triggers the ShutdownCoordinator flow from async context.

**When to use:** LIFE-04 — Kubernetes pod termination requires graceful shutdown within terminationGracePeriod.

**Example:**
```rust
// src/runtime/mod.rs — add to Runtime::run() entry point
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::CancellationToken;

pub async fn run_with_sigterm(mut runtime: Runtime) {
    // Spawn SIGTERM handler
    let sigterm_received = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let sigterm_flag = Arc::clone(&sigterm_received);

    let rt_handle = tokio::runtime::Handle::current();
    std::thread::spawn(move || {
        // Use signal-hook-registry for cross-platform SIGTERM
        // In practice, use signal-hook crate or tokio's unix signals
        while !sigterm_flag.load(std::sync::atomic::Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        rt_handle.spawn(async {
            runtime.shutdown().await;
        });
    });

    runtime.run().await;
}
```

**Better approach (using signal-hook):**
```rust
use signal_hook::SigId;
use signal_hook::iterator::Signals;

async fn install_sigterm_handler(coordinator: Arc<ShutdownCoordinator>) {
    let mut signals = Signals::new([SignalKind::terminate()]).unwrap();
    tokio::spawn(async move {
        if let Some(sig) = signals.next().await {
            tracing::info!(signal = ?sig, "received SIGTERM");
            coordinator.begin_draining();
        }
    });
}
```

**Key insight:** Kubernetes sends SIGTERM → process has terminationGracePeriod (default 30s) to drain → SIGKILL if not exited. `drain_timeout_secs` in `ShutdownCoordinator` should be set to `terminationGracePeriod - 5s` to allow SIGKILL safety margin.

### Pattern 3: Terminal Failure Blocks Partition (OFF-04)

**What:** When `FailureReason.category() == Terminal`, set `has_terminal = true` in `PartitionState` and gate `should_commit()` on it.

**Already implemented in Phase 1:**
```rust
// src/offset/offset_tracker.rs — should_commit()
pub fn should_commit(&self, topic: &str, partition: i32) -> bool {
    guard.get(&key).is_some_and(|s| {
        if s.has_terminal { return false; }  // D-01: terminal blocks
        !s.pending_offsets.is_empty() || s.committed_offset >= 0
    })
}
```

```rust
// src/offset/offset_coordinator.rs — mark_failed()
fn mark_failed(&self, topic: &str, partition: i32, offset: i64, reason: &FailureReason) {
    if let Some(state) = guard.get_mut(&key) {
        state.mark_failed(offset);
        if reason.category() == FailureCategory::Terminal {
            state.has_terminal = true;  // D-03: set once, never clear
        }
    }
}
```

**Key insight:** This is per-partition — partition A's poison message does NOT block partition B.

### Pattern 4: Failure Flow in Worker Loop

**What:** When a Python handler raises an exception, the worker loop must classify, record retry state, and either delay-retry or DLQ-route.

**Wiring needed in worker_loop (not yet connected):**
```rust
// In worker_loop.rs — after Python handler returns error
let failure_reason = failure_classifier.classify(&py_err, &ctx);
let (should_retry, should_dlq, delay) =
    retry_coordinator.record_failure(topic, partition, offset, &failure_reason);

if should_dlq {
    // Route to DLQ
    let attempt = retry_coordinator.attempt_count(topic, partition, offset);
    let metadata = DlqMetadata::new(
        topic.clone(), partition, offset,
        failure_reason.to_string(), attempt as u32,
        now, now,
    );
    let tp = dlq_router.route(&metadata);
    dlq_producer.produce_async(tp.topic, tp.partition, payload, key, &metadata);
    offset_coordinator.mark_failed(topic, partition, offset, &failure_reason);
} else if should_retry {
    // Sleep then re-queue (or re-dispatch synchronously)
    tokio::time::sleep(delay).await;
    // Re-add to handler queue or dispatch synchronously
}
```

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Exponential backoff with jitter | Hand-roll delay formula | `RetryPolicy::schedule().next_delay(attempt)` | Already implemented; rand jitter correctly avoids thundering herd |
| DLQ metadata envelope | Build custom header format | `DlqMetadata` + `SharedDlqProducer::metadata_to_headers()` | Already implemented with dlq.* headers |
| Failure classification | Try/except string matching | `DefaultFailureClassifier` | Maps exception type names via debug repr; extensible via trait |
| Graceful shutdown phases | Build ad-hoc shutdown state | `ShutdownCoordinator` 4-phase enum | Already implemented with panic on invalid transitions |

---

## Common Pitfalls

### Pitfall 1: Commit During Rebalance Callback (PITFALLS-4.3) — Critical

**What goes wrong:** Calling `commit()` inside `partitions_revoked` callback for EAGER partition assignment strategies causes `ILLEGAL_GENERATION` errors.

**Why it happens:** With EAGER strategy, partition revocation happens before assignment. Committing during this window conflicts with the rebalance protocol.

**How to avoid:** Use `cooperative-sticky` partition assignment strategy, which allows commit during rebalance callback. Alternatively, defer commit to the consume loop (not the callback).

**Warning signs:** `KafkaError::InvalidAssignmentState` or `ILLEGAL_GENERATION` in logs.

### Pitfall 2: Off-by-One on Seek After Partition Assignment (PITFALLS-4.1) — High

**What goes wrong:** After partition reassignment, consumer seeks to the last committed offset instead of `committed_offset + 1`, causing the same message to be re-delivered.

**Why it happens:** Kafka commit semantics: committed offset N means "I have processed 0..=N". The next message to read is N+1.

**How to avoid:** Always seek to `highest_contiguous + 1`, not `highest_contiguous`.

### Pitfall 3: GIL Blocking in Rebalance Callback — Critical

**What goes wrong:** If a rebalance callback calls Python code (e.g., via a user-provided callback), it will block the rdkafka client thread, freezing the consumer.

**Why it happens:** rdkafka callbacks run in the client's internal thread, not in Tokio. The GIL is not held there.

**How to avoid:** Rebalance callbacks must ONLY do Rust work: commit offsets, update shared state. Never call `Python::attach` or `spawn_blocking` from rebalance callbacks. Use a channel to signal the Tokio task to do Python work asynchronously.

### Pitfall 4: SIGTERM Handler Installed Outside Tokio Context — High

**What goes wrong:** Installing SIGTERM handlers in a `std::thread::spawn` that tries to call `Handle::current()` on a Tokio runtime that was dropped.

**Why it happens:** The Tokio runtime Handle is scoped to the runtime lifetime. A detached thread cannot safely access it after `Runtime::run()` completes.

**How to avoid:** Install SIGTERM handler BEFORE entering the Tokio runtime context, or use `signal_hook` with async signal handling that is scoped to the runtime.

### Pitfall 5: drain_timeout Exceeds terminationGracePeriod — High

**What goes wrong:** If `drain_timeout_secs` is set equal to or greater than Kubernetes' `terminationGracePeriod`, the process may be force-killed while still draining.

**How to avoid:** `drain_timeout_secs` should be `terminationGracePeriod - 5s` to provide a safety margin for SIGKILL.

---

## Code Examples

### Rebalance Listener Registration

```rust
// src/consumer/runner.rs — ConsumerRunner::new() with rebalance context
use rdkafka::consumer::{ConsumerContext, StreamConsumer};
use rdkafka::ClientConfig;

pub struct ConsumerRunner {
    consumer: Arc<StreamConsumer>,  // StreamConsumer<CustomConsumerContext>
    shutdown_tx: broadcast::Sender<()>,
    coordinator: Option<Arc<ShutdownCoordinator>>,
}

// Building a consumer with custom context
let context = CustomConsumerContext::new(
    offset_tracker.clone(),
    dlq_router.clone(),
    dlq_producer.clone(),
);
let consumer: StreamConsumer = config
    .into_rdkafka_config()
    .create_with_context(context)
    .map_err(ConsumerError::Kafka)?;
```

### RetryConfig Python Binding (CONF-03)

```python
# kafpy/config.py — RetryConfig (already exists)
@dataclass(frozen=True)
class RetryConfig:
    max_attempts: int = 3
    base_delay: float = 0.1
    max_delay: float | None = None
    jitter_factor: float | None = None

# ConsumerConfig.to_rust() — wiring already in place
def to_rust(self) -> object:
    py_retry_policy = None
    if self.retry_policy is not None:
        py_retry_policy = _kafpy.PyRetryPolicy(
            max_attempts=self.retry_policy.max_attempts,
            base_delay_ms=int(self.retry_policy.base_delay * 1000),
            max_delay_ms=int(self.retry_policy.max_delay * 1000) if ...,
            jitter_factor=self.retry_policy.jitter_factor if ...,
        )
```

### Pause/Resume in ConsumerRunner (CORE-08)

```rust
// src/consumer/runner.rs — already has pause/resume methods
pub fn pause(&self, tpl: &rdkafka::TopicPartitionList) -> Result<(), ConsumerError> {
    self.consumer.pause(tpl).map_err(ConsumerError::Kafka)
}

pub fn resume(&self, tpl: &rdkafka::TopicPartitionList) -> Result<(), ConsumerError> {
    self.consumer.resume(tpl).map_err(ConsumerError::Kafka)
}
```

**Usage:** Wrap in `tokio::time::timeout` for stuck handler recovery (backpressure from CORE-08 flow control).

### Key-Based Routing as Fallback (MSG-03)

```rust
// src/runtime/builder.rs — RoutingChain construction
let chain = RoutingChain::new()
    .with_topic_router(topic_router)
    .with_header_router(header_router)
    .with_key_router(key_router)  // MSG-03: key router as fallback
    .with_default_handler(default_handler);

// KeyRouter already implemented in src/routing/key.rs
// KeyRule supports: Exact, Prefix, ExactStr, PrefixStr
let key_router = KeyRouter::new(vec![
    KeyRule::exact_str("order-123", HandlerId::new("order-handler")),
    KeyRule::prefix_str("user-", HandlerId::new("user-handler")),
]);
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Unconditional retry | Categorized retry (Retryable/Terminal/NonRetryable) | Phase 2 | Prevents infinite retry on poison messages |
| No DLQ | DLQ with metadata envelope | Phase 2 | Failed messages recoverable |
| Shutdown via broadcast | 4-phase ShutdownCoordinator | Phase 1 | Deterministic shutdown order |
| EAGER partition strategy | cooperative-sticky recommended | Phase 2 | Safe rebalance commits |

**Deprecated/outdated:**
- `enable_auto_commit = true` with manual `commit()` — confusing; Phase 1 builder defaults to `false`
- Using `HashMap` presence for retry state — replaced with explicit `RetryState` enum (makes illegal states unrepresentable)

---

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `partitions_revoked` callback can synchronously commit offsets for cooperative-sticky strategy | Pattern 1 | If not, need to defer commit to consume loop |
| A2 | `signal-hook` crate is available for cross-platform SIGTERM handling | Pattern 2 | Alternative: tokio's unix signal only works on Unix |
| A3 | Worker loop already handles `Result<..., PyErr>` from Python handler execution | Pattern 4 | Worker loop needs refactoring to integrate RetryCoordinator |
| A4 | `has_terminal = true` is set idempotently (multiple calls don't change state) | Pattern 3 | If not, terminal could reset if retried |

**If this table is empty:** All claims in this research were verified or cited — no user confirmation needed.

---

## Open Questions

1. **SIGTERM handler installation path**
   - What we know: tokio supports `tokio::signal::unix::signal(SignalKind::terminate())` on Unix; no Windows support needed for Kubernetes
   - What's unclear: best integration point — in `Runtime::run()` or separate `spawn()` before calling `pool.run()`
   - Recommendation: Install handler in `RuntimeBuilder::build()` as a spawned task; the handler signals the ShutdownCoordinator

2. **Cooperative-sticky vs EAGER as default**
   - What we know: cooperative-sticky allows safe commit during rebalance
   - What's unclear: performance tradeoffs; whether EAGER is ever preferred
   - Recommendation: Default to `cooperative-sticky` for Phase 2; document the trade-off

3. **DLQ message payload on shutdown flush**
   - What we know: `OffsetTracker.flush_failed_to_dlq()` currently produces empty payload because `OffsetTracker` doesn't store original message
   - What's unclear: whether original payload is available elsewhere at shutdown time
   - Recommendation: Accept empty payload as limitation; document that `flush_failed_to_dlq` on shutdown is best-effort

4. **Custom ConsumerContext integration with StreamConsumer**
   - What we know: `StreamConsumer` is `BaseConsumer<DefaultConsumerContext>`; creating a custom context requires `create_with_context()`
   - What's unclear: exact type signature needed for `StreamConsumer<CustomConsumerContext>`
   - Recommendation: Use `typealias` or verify with rdkafka docs; `CustomConsumerContext` must implement `ConsumerContext` + `Debug`

5. **Key-based routing chain position**
   - What we know: `RoutingChain` has 5 slots: topic, header, key, python, default
   - What's unclear: whether key-based routing (MSG-03) should come BEFORE header-based routing or as fallback AFTER topic
   - Recommendation: Key as fallback after topic (chain position 3), per existing RoutingChain precedence order documented in chain.rs

---

## Environment Availability

> Step 2.6: SKIPPED — no external dependencies beyond project code. All required libraries are in Cargo.toml.

---

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust `#[test]` + `#[tokio::test]` (standard Cargo test) |
| Config file | None — see Wave 0 |
| Quick run command | `cargo test --lib` |
| Full suite command | `cargo test --all` |

### Phase Requirements to Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|--------------|
| CORE-06 | on_partitions_revoked commits offsets before revocation | unit | `cargo test rebalance` | no |
| CORE-06 | on_partitions_assigned seeks to committed+1 | unit | `cargo test rebalance` | no |
| CORE-08 | pause() stops recv for specified partitions | unit | `cargo test pause` | no |
| CORE-08 | resume() restarts recv for specified partitions | unit | `cargo test pause` | no |
| MSG-03 | KeyRouter.route() returns Route on key match | unit | `cargo test key_router` | YES: `src/routing/key.rs` |
| MSG-03 | KeyRouter.route() defers on no match | unit | `cargo test key_router` | YES: `src/routing/key.rs` |
| OFF-04 | should_commit returns false when has_terminal=true | unit | `cargo test terminal` | YES: `src/offset/offset_tracker.rs` |
| OFF-04 | has_terminal set once, never cleared | unit | `cargo test terminal` | YES: `src/offset/offset_tracker.rs` |
| FAIL-01 | FailureReason.category() returns correct category | unit | `cargo test failure_category` | YES: `src/failure/tests.rs` |
| FAIL-02 | DefaultFailureClassifier maps TimeoutError to Retryable | unit | `cargo test classify` | YES: `src/failure/classifier.rs` |
| FAIL-03 | RetrySchedule.next_delay() grows exponentially with jitter | unit | `cargo test retry_schedule` | YES: `src/retry/policy.rs` |
| FAIL-04 | RetryCoordinator records attempt count correctly | unit | `cargo test retry_coordinator` | YES: `src/retry/retry_coordinator.rs` |
| FAIL-05 | SharedDlqProducer produces asynchronously | unit | `cargo test dlq_producer` | YES: `src/dlq/produce.rs` |
| FAIL-06 | DlqMetadata contains all required fields | unit | `cargo test dlq_metadata` | YES: `src/dlq/metadata.rs` |
| FAIL-07 | DefaultDlqRouter computes topic = prefix + original | unit | `cargo test dlq_router` | YES: `src/dlq/router.rs` |
| LIFE-01 | ShutdownCoordinator transitions: Running→Draining→Finalizing→Done | unit | `cargo test shutdown` | YES: `src/shutdown/shutdown.rs` |
| LIFE-02 | WorkerPool.shutdown() drains within timeout | unit | `cargo test shutdown` | YES: `src/worker_pool/pool.rs` |
| LIFE-03 | flush_failed_to_dlq() called during shutdown | unit | `cargo test shutdown` | YES: `src/offset/offset_tracker.rs` |
| LIFE-04 | SIGTERM triggers begin_draining() | integration | manual SIGTERM test | no |
| LIFE-05 | Revoked partitions commit before loss | unit | `cargo test rebalance` | no |
| CONF-03 | RetryConfig.to_rust() produces correct RetryPolicy | unit | `cargo test retry_config` | YES: `src/pyconfig.rs` |

### Sampling Rate
- **Per task commit:** `cargo test --lib`
- **Per wave merge:** `cargo test --all`
- **Phase gate:** Full suite green before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `src/consumer/context.rs` — CustomConsumerContext with rebalance callbacks; covers CORE-06, LIFE-05
- [ ] `tests/test_rebalance.rs` — Rebalance unit tests; covers CORE-06, LIFE-05
- [ ] `tests/test_sigterm.rs` — SIGTERM handling integration test; covers LIFE-04
- [ ] `tests/test_retry_integration.rs` — RetryCoordinator wiring tests; covers FAIL-03, FAIL-04
- [ ] `tests/test_dlq_integration.rs` — DLQ routing end-to-end tests; covers FAIL-05, FAIL-06, FAIL-07
- [ ] Framework install: None — existing `cargo test` infrastructure covers all phase requirements

*(If no gaps: "None — existing test infrastructure covers all phase requirements")*

---

## Security Domain

> Required when `security_enforcement` is enabled (absent = enabled). Omit only if explicitly `false` in config.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|------------------|
| V2 Authentication | no | Not applicable — Kafka auth handled by rdkafka config |
| V3 Session Management | no | Consumer group session via rdkafka session.timeout.ms |
| V4 Access Control | no | Kafka ACLs managed externally |
| V5 Input Validation | yes | Python exception string parsing in DefaultFailureClassifier |
| V6 Cryptography | no | TLS/SASL handled by rdkafka; no custom crypto |

### Known Threat Patterns for rdkafka + Tokio

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Deserialization failure (poison message) | Information Disclosure | Terminal failure classification; DLQ with sanitized metadata |
| Infinite retry loop (misconfigured RetryPolicy) | Denial of Service | max_attempts cap; terminal categorization |
| DLQ topic full (backpressure on DLQ) | Denial of Service | Fire-and-forget channel with bounded capacity; warning log on drop |
| SIGTERM while mid-commit | Data Inconsistency | ShutdownCoordinator ordering: drain before final commit |

---

## Sources

### Primary (HIGH confidence)
- `src/consumer/runner.rs` — ConsumerRunner with pause/resume, rebalance API
- `src/offset/offset_tracker.rs` — BTreeSet algorithm, has_terminal, should_commit gating
- `src/retry/policy.rs` — RetryPolicy, RetrySchedule with jitter
- `src/retry/retry_coordinator.rs` — RetryState enum, record_failure state machine
- `src/failure/reason.rs` — FailureReason/FailureCategory taxonomy
- `src/failure/classifier.rs` — DefaultFailureClassifier implementation
- `src/dlq/router.rs` — DlqRouter trait, DefaultDlqRouter
- `src/dlq/metadata.rs` — DlqMetadata envelope
- `src/dlq/produce.rs` — SharedDlqProducer fire-and-forget
- `src/shutdown/shutdown.rs` — ShutdownCoordinator 4-phase lifecycle
- `src/worker_pool/pool.rs` — WorkerPool shutdown with drain timeout
- `src/routing/key.rs` — KeyRouter with Exact/Prefix modes
- `src/routing/chain.rs` — RoutingChain precedence order
- `kafpy/config.py` — Python RetryConfig dataclass
- `src/pyconfig.rs` — PyRetryPolicy PyO3 binding
- `target/doc/rdkafka/` — rdkafka 0.38.0 API docs

### Secondary (MEDIUM confidence)
- `src/runtime/builder.rs` — RuntimeBuilder assembly order (understanding of wiring gaps)
- `src/offset/offset_coordinator.rs` — OffsetCoordinator trait impl
- `src/consumer/config.rs` — ConsumerConfigBuilder

### Tertiary (LOW confidence)
- rdkafka `partitions_revoked`/`partitions_assigned` semantics — verified via generated HTML docs but exact thread-safety constraints require runtime testing

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all libraries already in Cargo.toml; versions confirmed
- Architecture: MEDIUM-HIGH — skeleton code exists for all modules; integration points need verification
- Pitfalls: MEDIUM — PITFALLS documented from Phase 1 research; rebalance callback pitfalls verified with rdkafka docs

**Research date:** 2026-04-29
**Valid until:** 2026-05-29 (30 days — standard stack stable)
