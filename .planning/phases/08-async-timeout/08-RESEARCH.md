# Phase 8: Async Timeout - Research

**Researched:** 2026-04-29
**Domain:** Async handler timeout enforcement, DLQ metadata enrichment, Prometheus timeout metrics
**Confidence:** HIGH

## Summary

Phase 8 implements async handler timeout enforcement: Python handlers that exceed their configured timeout are aborted, routed to DLQ with timeout metadata, and counted in Prometheus. The core mechanism (`invoke_mode_with_timeout` wrapping `invoke_mode` in `tokio::time::timeout`) is already implemented in `handler.rs:280-316`. This phase wires the Python API (TMOUT-01), enriches the DLQ envelope with timeout fields (TMOUT-02), and emits a timeout counter to Prometheus (TMOUT-03).

**Primary recommendation:** TMOUT-01 is nearly complete - the Rust-side mechanism exists and the Python `add_handler` accepts `timeout_ms`. The planner should verify the Python decorator API (`@handler(timeout=X)`) if it exists, otherwise confirm `add_handler` is the intended wire. TMOUT-02 requires adding `timeout_duration` and `last_processed_offset` to `DlqMetadata` and propagating those fields from `PythonHandler` through the failure path. TMOUT-03 requires registering and emitting `handler_timeout_total` counter.

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

- **D-01:** Use `abort() on JoinHandle` - Tokio's spawn returns JoinHandle, call abort() when timeout fires. Simple, native to Tokio, no additional dependencies.
- **D-02:** DLQ envelope includes `timeout_duration` (u64 seconds) and `last_processed_offset` (Option<u64>) when timeout triggers. No extra timing fields - keep envelope lean.
- **D-03:** Timeout metric is a counter per handler: `handler_timeout_total{topic, handler_name}`. No gauge or histogram - just count.

### Claude's Discretion

- How to track active timeouts (e.g., per-message timer management) - planner decides
- JoinHandle storage and cleanup strategy - planner decides
</user_constraints>

---

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| TMOUT-01 | `@handler(topic, timeout=X)` Python API - wire existing `invoke_mode_with_timeout` | `PythonHandler::invoke_mode_with_timeout` already exists (`handler.rs:280-316`); Python `add_handler` accepts `timeout_ms` (`pyconsumer.rs:61-70`). API path confirmed. |
| TMOUT-02 | Timeout metadata propagated to DLQ envelope (`timeout_duration`, `last_processed_offset`) | `DlqMetadata` struct needs two new fields (`src/dlq/metadata.rs`); `handle_execution_failure` creates the envelope (`src/worker_pool/mod.rs:64-72`). Integration point identified. |
| TMOUT-03 | Timeout metric (count per handler) emitted to Prometheus | `SharedPrometheusSink` pattern confirmed (`src/observability/metrics.rs:150-166`); counter registration + emission pattern confirmed. Label format: `topic`, `handler_name`. |

---

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Timeout enforcement | API/Backend | Browser/Client | Tokio-level timeout wrapping of handler invocation - purely server-side |
| DLQ metadata enrichment | API/Backend | - | `DlqMetadata` struct and `handle_execution_failure` - both in Rust core |
| Prometheus metric emission | API/Backend | - | `SharedPrometheusSink` + `MetricsSink` trait - Rust observability |
| Python handler timeout config | API/Backend | - | PyO3 `add_handler` accepting `timeout_ms` - PyO3 boundary |

---

## Standard Stack

No new dependencies. All required types are already in the crate.

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `tokio::time::timeout` | tokio 1.x (via `tokio-util`) | Wraps handler invocation with deadline | Already used in `handler.rs:287` |
| `prometheus_client` | already in `Cargo.toml` | Prometheus metrics sink and counter | Already used for `kafpy.handler.invocation`, `kafpy.dlq.messages` etc. |
| `DlqMetadata` | existing | DLQ envelope struct | Already in `src/dlq/metadata.rs` |
| `SharedPrometheusSink` | existing | Shared metrics recorder | Already in `src/observability/metrics.rs` |

---

## Architecture Patterns

### System Architecture Diagram

```
Python Consumer Code
    |
    v
PyConsumer::add_handler(topic, callback, timeout_ms=X)
    |  (pyconsumer.rs:63-85)
    |  Creates HandlerMetadata{timeout_ms: X, ...}
    v
RuntimeBuilder::build()
    |  (runtime/builder.rs:171-203)
    |  Resolves per-handler timeout via meta.timeout_ms or config default
    |  Creates PythonHandler{handler_timeout: Some(Duration), ...}
    v
WorkerPool::worker_loop()
    |  (worker_pool/worker.rs:50-282)
    |  Calls handler.invoke_mode_with_timeout(&ctx, msg)
    |  handler.rs:280-316
    |    tokio::time::timeout(handler_timeout, handler.invoke_mode(...))
    |      On timeout -> ExecutionResult::Error{Terminal(HandlerPanic)}
    |  On error -> handle_execution_failure() (worker_pool/mod.rs:40-101)
    |    Creates DlqMetadata{timeout_duration, last_processed_offset}
    |    Calls dlq_producer.produce_async()
    |  Metrics emission:
    |    HANDLER_METRICS.record_error() for general errors
    |    NEW: TimeoutMetrics::record_timeout() for timeout-specific counter
    v
Prometheus / DLQ Kafka Topic
```

### Recommended Project Structure

No new files required. Modifications to existing files:

```
src/
├── dlq/metadata.rs          # Add timeout_duration, last_processed_offset fields
├── observability/metrics.rs  # Add TimeoutMetrics::record_timeout()
├── python/handler.rs         # Propagate timeout_duration to result (optional)
├── worker_pool/mod.rs        # Pass timeout info to DlqMetadata
└── worker_pool/worker.rs     # Emit timeout metric on timeout branch
```

### Pattern 1: Timeout Wrapping Existing Handler Invocation

**What:** `invoke_mode_with_timeout` (already implemented in `handler.rs:280-316`)

**When to use:** When a handler needs a deadline - wraps any async invocation in `tokio::time::timeout`.

**Example:**
```rust
// handler.rs:280-316 (already exists)
pub async fn invoke_mode_with_timeout(
    &self,
    ctx: &ExecutionContext,
    message: OwnedMessage,
) -> ExecutionResult {
    match self.handler_timeout {
        Some(timeout) => {
            match tokio::time::timeout(timeout, self.invoke_mode(ctx, message)).await {
                Ok(result) => result,
                Err(_) => {
                    tracing::error!(... "handler timed out after {}ms", ...);
                    ExecutionResult::Error {
                        reason: FailureReason::Terminal(TerminalKind::HandlerPanic),
                        exception: "HandlerTimeout".to_string(),
                        traceback: format!("handler timed out after {}ms...", ...),
                    }
                }
            }
        }
        None => self.invoke_mode(ctx, message).await,
    }
}
```

**Existing in codebase:** YES - `handler.rs:280-316`

### Pattern 2: DLQ Metadata Enrichment with Failure Context

**What:** Adding structured metadata to DLQ envelope when specific failure types occur.

**When to use:** When DLQ messages need debugging context beyond standard envelope fields.

**Example:**
```rust
// worker_pool/mod.rs:64-72 (existing pattern to extend)
let metadata = DlqMetadata::new(
    ctx.topic.clone(),
    ctx.partition,
    ctx.offset,
    reason.to_string(),
    retry_coordinator.attempt_count(&ctx.topic, ctx.partition, ctx.offset) as u32,
    chrono::Utc::now(),
    chrono::Utc::now(),
    // NEW: timeout_duration and last_processed_offset
    timeout_duration,
    last_processed_offset,
);
```

**Integration point:** `handle_execution_failure` in `src/worker_pool/mod.rs:64-72`

### Pattern 3: Prometheus Counter Per Handler

**What:** Register a counter family, then increment with labels per handler.

**When to use:** For cardinality-controlled metrics where labels identify the handler/topic.

**Example:**
```rust
// observability/metrics.rs (existing pattern to follow)
// In SharedPrometheusSink::new():
sink.register_counter("kafpy.handler.timeout", "Handler timeout count");

// In metrics emission:
pub struct TimeoutMetrics;
impl TimeoutMetrics {
    pub fn record_timeout(sink: &dyn MetricsSink, topic: &str, handler_name: &str) {
        let labels = MetricLabels::new()
            .insert("topic", topic)
            .insert("handler_name", handler_name);
        sink.record_counter("kafpy.handler.timeout", &labels.as_slice());
    }
}
```

**Existing pattern:** `ThroughputMetrics::record_throughput` (`metrics.rs:276-285`) and `DlqMetrics::record_dlq_message` (`metrics.rs:317-328`)

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Timeout enforcement | Custom timer management with channels | `tokio::time::timeout` | Native to Tokio, properly cancellation-safe, no extra dependencies |
| DLQ metadata | Manual header construction for timeout info | Extend `DlqMetadata` struct | Type-safe, consistent with existing envelope pattern |
| Prometheus metrics | Custom metric collection | `SharedPrometheusSink` + `MetricsSink` trait | Already integrated, thread-safe, same pattern as all other metrics |

**Key insight:** The codebase already has all three patterns implemented partially. This phase extends them rather than creating new ones.

---

## Common Pitfalls

### Pitfall 1: Timeout result carries no timeout context downstream
**What goes wrong:** When `invoke_mode_with_timeout` returns `ExecutionResult::Error{Terminal(HandlerPanic)}`, the failure path in `handle_execution_failure` cannot distinguish a timeout from any other panic. This means TMOUT-02 DLQ metadata cannot be populated with the correct `timeout_duration` value.

**Why it happens:** `ExecutionResult` has no field for "which timeout was exceeded". The error traceback contains the message but is not structured for programmatic extraction.

**How to avoid:** Propagate the timeout duration through a structured channel. Options:
1. Extend `ExecutionResult::Error` with an optional `timeout_info: Option<TimeoutInfo>` field (where `TimeoutInfo { duration_secs: u64, last_offset: Option<u64> }`)
2. Store `timeout_duration` on `ExecutionContext` before calling `invoke_mode_with_timeout`, then retrieve it on timeout
3. Have `PythonHandler` store its own `handler_timeout` and query it from `handle_execution_failure` via the handler reference

**Recommendation:** Option 1 is cleanest - structured data flows through the existing result channel without changing the call graph.

### Pitfall 2: Confusing per-handler timeout vs. global config timeout
**What goes wrong:** `ConsumerConfig.handler_timeout_ms` is the global default, but `HandlerMetadata.timeout_ms` is per-handler. The resolution logic in `runtime/builder.rs:178-181` uses `meta.timeout_ms.or(default_handler_timeout)`. If the Python API doesn't expose per-handler timeout clearly, users may not understand the precedence.

**How to avoid:** Document the precedence: per-handler `timeout_ms` in `add_handler` > global `handler_timeout_ms` in `ConsumerConfig` > no timeout.

### Pitfall 3: Metric cardinality explosion
**What goes wrong:** Using `topic` + `handler_name` as labels is safe since both are bounded by the user's Kafka topology. However, if `handler_name` is constructed from user-provided strings without validation, a malicious or buggy user could create unbounded label cardinality.

**How to avoid:** The existing code uses `handler.name()` which comes from the topic string (`PythonHandler::name` is set to `topic.clone()` in `builder.rs:197`). Topic names are already controlled by Kafka configuration, so cardinality is bounded. No action needed.

---

## Runtime State Inventory

> This phase does not involve rename/refactor/migration - this section is N/A.

---

## Code Examples

### TMOUT-01: Python `add_handler` timeout_ms parameter (EXISTING)
```python
# kafpy/consumer.py:32-54
def add_handler(
    self,
    topic: str,
    handler: Callable[[object], None],
    *,
    mode: str | None = None,
    batch_max_size: int | None = None,
    batch_max_wait_ms: int | None = None,
    timeout_ms: int | None = None,  # <-- already present
    concurrency: int | None = None,
) -> None:
    self._consumer.add_handler(topic, handler, mode, batch_max_size, batch_max_wait_ms, timeout_ms, concurrency)
```

The `timeout_ms` is already wired through `PyConsumer::add_handler` -> `HandlerMetadata::timeout_ms` -> `PythonHandler::handler_timeout` -> `invoke_mode_with_timeout`. TMOUT-01 verification task: confirm Python decorator `@handler(timeout=X)` also wires this (or if not, document `add_handler` as the API).

### TMOUT-02: DlqMetadata new fields needed
```rust
// src/dlq/metadata.rs
// ADD these fields:
pub struct DlqMetadata {
    // ... existing fields ...
    pub timeout_duration: Option<u64>,        // seconds, when timeout triggered
    pub last_processed_offset: Option<u64>,  // offset when timeout fired
}
```

### TMOUT-03: TimeoutMetrics counter (follows existing pattern)
```rust
// src/observability/metrics.rs
// In SharedPrometheusSink::new(), add:
// sink.register_counter("kafpy.handler.timeout", "Handler timeout count");

pub struct TimeoutMetrics;
impl TimeoutMetrics {
    pub fn record_timeout(sink: &dyn MetricsSink, topic: &str, handler_name: &str) {
        let labels = MetricLabels::new()
            .insert("topic", topic)
            .insert("handler_name", handler_name);
        sink.record_counter("kafpy.handler.timeout", &labels.as_slice());
    }
}
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|---------------|--------|
| No handler timeout | `tokio::time::timeout` wrapping `invoke_mode` | Phase 8 (this phase) | Handlers can now be bounded in execution time |
| Generic `HandlerPanic` for all errors | Structured timeout info in result/metadata | Phase 8 (this phase) | DLQ debugging can distinguish timeout from other failures |
| No timeout metrics | `kafpy.handler.timeout_total` counter | Phase 8 (this phase) | Observability for timeout frequency per handler |

**Deprecated/outdated:**
- None identified for this phase.

---

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | The existing `handler_timeout` field on `PythonHandler` and `invoke_mode_with_timeout` mechanism in `handler.rs:280-316` is functionally complete and wired to the Python layer | TMOUT-01 | If not wired, TMOUT-01 requires completing the wire rather than just verifying it |
| A2 | The Python decorator `@handler(timeout=X)` does NOT already exist separately from `add_handler(timeout_ms=X)` | TMOUT-01 | If it already exists, the verification task is simpler |
| A3 | `last_processed_offset` in TMOUT-02 means the offset of the last successfully processed message before the timeout (i.e., committed offset minus 1), not the offset of the timed-out message itself | TMOUT-02 | If it means the timed-out offset, metadata construction changes |

**If this table is empty:** All claims were verified via code inspection - see sources.

---

## Open Questions

1. **Python decorator vs. method API for TMOUT-01**
   - What we know: `add_handler(topic, callback, timeout_ms=X)` exists and is wired
   - What's unclear: Does a `@handler(topic, timeout=X)` Python decorator exist? If not, should one be created?
   - Recommendation: Planner should verify the Python handler registration API. If only `add_handler` exists, document it as the timeout API. If `@handler` decorator exists, verify it also accepts `timeout=`.

2. **last_processed_offset semantics**
   - What we know: D-02 says `last_processed_offset: Option<u64>` should be in the DLQ envelope when timeout triggers
   - What's unclear: Does this mean "the offset of the last message the handler completed before this timeout" (which would be `original_offset - 1` if no batching), or is there a way to track a "last processed" offset per partition?
   - Recommendation: Clarify with user. Likely interpretation is `original_offset - 1` for single-message handlers, None for batch.

3. **JoinHandle abort and cleanup (D-01, Claude's discretion)**
   - What we know: D-01 says "Use `abort()` on JoinHandle" when timeout fires. `invoke_mode_with_timeout` currently uses `tokio::time::timeout` which just drops the future on timeout - it does NOT call `abort()`.
   - What's unclear: Does the current implementation need to call `abort()` explicitly, or is the timeout sufficient to cancel the task?
   - Recommendation: The current `tokio::time::timeout` does NOT abort the inner future. For Python GIL-holding threads (via `spawn_blocking`), the thread continues running even after the timeout. Planner should evaluate whether `spawn_blocking` tasks need explicit `abort()`. This is D-01 territory (Claude's discretion).

---

## Environment Availability

Step 2.6: SKIPPED (no external dependencies beyond existing Rust crate dependencies).

---

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | `#[tokio::test]` for async tests, `#[test]` for sync |
| Config file | `Cargo.toml` (existing) |
| Quick run command | `cargo test --lib -- timeout` |
| Full suite command | `cargo test --lib` |

### Phase Requirements -> Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| TMOUT-01 | `add_handler(timeout_ms=X)` sets handler timeout | unit | `cargo test handler_timeout --lib` | YES (existing invoke_mode_with_timeout tests) |
| TMOUT-02 | DLQ metadata contains timeout fields when timeout triggers | unit | `cargo test dlq_metadata --lib` | NO (new test needed) |
| TMOUT-03 | `kafpy.handler.timeout_total` counter increments on timeout | unit | `cargo test timeout_metrics --lib` | NO (new test needed) |

### Sampling Rate
- **Per task commit:** `cargo test --lib`
- **Per wave merge:** `cargo test`
- **Phase gate:** Full suite green before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `tests/timeout_test.rs` - covers TMOUT-01 (verify existing mechanism), TMOUT-02 (DlqMetadata fields), TMOUT-03 (timeout counter)
- [ ] `src/observability/metrics.rs` - add `TimeoutMetrics::record_timeout()` (Wave 0 baseline for TMOUT-03)
- [ ] `src/dlq/metadata.rs` - add timeout fields (Wave 0 baseline for TMOUT-02)

*(If no gaps: "None - existing test infrastructure covers all phase requirements")*

---

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V5 Input Validation | yes | `timeout_ms` is u64 - validated at PyO3 boundary (`pyconsumer.rs:61-70`) |
| V4 Access Control | no | Not applicable to timeout enforcement |

### Known Threat Patterns for Stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Unbounded timeout value causing resource exhaustion | Denial of Service | Timeout values are bounded by u64 ms max; handler_timeout is Duration (sensible defaults exist) |
| Metric cardinality explosion from label values | Information Disclosure | Label keys are fixed (`topic`, `handler_name`); values come from controlled Kafka config (topic names) |

---

## Sources

### Primary (HIGH confidence)
- `src/python/handler.rs` lines 280-316 - `invoke_mode_with_timeout` implementation
- `src/observability/metrics.rs` lines 150-166 - `SharedPrometheusSink::new()` counter registration pattern
- `src/worker_pool/mod.rs` lines 40-101 - `handle_execution_failure` DLQ metadata creation
- `src/dlq/metadata.rs` - `DlqMetadata` struct definition
- `src/pyconsumer.rs` lines 61-85 - Python `add_handler` with `timeout_ms`

### Secondary (MEDIUM confidence)
- `src/runtime/builder.rs` lines 171-203 - timeout resolution (per-handler vs. global)
- `src/config.rs` lines 42, 97, 123 - `handler_timeout_ms` in config

### Tertiary (LOW confidence)
- None - all claims verified via code inspection

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - all required primitives already in codebase
- Architecture: HIGH - existing patterns fully map to requirements
- Pitfalls: MEDIUM - timeout result propagation needs planner decision on approach

**Research date:** 2026-04-29
**Valid until:** 2026-05-29 (30 days - stable phase domain)
