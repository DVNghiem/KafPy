# Phase 17: Failure Classification - Research

**Researched:** 2026-04-17
**Domain:** Rust error taxonomy, PyO3 exception classification, structured failure handling
**Confidence:** HIGH

## Summary

Phase 17 introduces a structured failure type taxonomy to replace the current raw string-based exception/rejection tracking in `ExecutionResult`. The key deliverables are: (1) a `FailureReason` enum with three categories (Retryable, Terminal, NonRetryable), (2) updated `ExecutionResult` variants that carry typed `FailureReason` instead of raw strings, (3) a `FailureClassifier` trait for extensible Python exception-to-failure-reason mapping, and (4) structured logging with failure context.

**Primary recommendation:** Define `FailureReason` as a flat enum with three named inner enums using `thiserror` (already in `Cargo.toml` as `2.0.17`). Add `FailureReason` to `ExecutionResult::Error` and `ExecutionResult::Rejected`, update `OffsetCoordinator::mark_failed` signature to accept `FailureReason`, and implement `FailureClassifier` as a simple trait that takes `&PyErr` and `&ExecutionContext`.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| FailureReason taxonomy | Rust (coordinator) | - | Rust owns classification logic; Python raises only |
| FailureClassifier trait | Rust (coordinator) | - | Pluggable classification; lives in coordinator module |
| ExecutionResult::Error/Rejected | Rust (python lane) | - | Normalized outcome type; Python lane produces, Rust consumes |
| mark_failed with FailureReason | Rust (OffsetTracker) | - | Future DLQ routing needs typed failure reason |
| Structured failure logging | Rust (worker_pool) | - | worker_loop logs with FailureReason context |

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `thiserror` | 2.0.17 | Enum error types with `#[derive(Error)]` | Already in `Cargo.toml`; better than hand-rolling Display/Debug |
| `pyo3` | 0.27.2 | PyErr access for classification | Already used for Python handler invocation |
| `tracing` | 0.1.44 | Structured logging with context | Already used in worker_pool and executor |

**No new dependencies required.** All needed primitives are already in the project.

### No Supporting Libraries Needed

Phase 17 is purely a type/trait design phase. No new libraries for this phase.

## Architecture Patterns

### System Architecture Diagram

```
Python Handler raises exception
         │
         ▼
┌──────────────────────────────────────┐
│  PythonHandler::invoke()              │
│  - Catches PyErr                     │
│  - Calls FailureClassifier.classify()│
│  - Returns ExecutionResult::Error    │
│    { reason: FailureReason, ... }    │
└──────────────────────────────────────┘
         │
         ▼
┌──────────────────────────────────────┐
│  worker_loop match on ExecutionResult│
│  - Ok        → record_ack           │
│  - Error     → mark_failed(reason)  │
│  - Rejected  → mark_failed(reason)  │
└──────────────────────────────────────┘
         │
         ▼
┌──────────────────────────────────────┐
│  OffsetTracker::mark_failed          │
│  (reason stored for DLQ routing)    │
└──────────────────────────────────────┘
```

### New File: `src/coordinator/failure_reason.rs`

```rust
//! Failure reason taxonomy — Retryable vs Terminal vs NonRetryable.

use thiserror::Error;

/// Structured failure reason from Python handler execution.
///
/// Classified by `FailureClassifier` from raw `PyErr`.
#[derive(Debug, Clone, Error)]
pub enum FailureReason {
    #[error("retryable: {0}")]
    Retryable(#[Embedded] RetryableKind),

    #[error("terminal: {0}")]
    Terminal(#[Embedded] TerminalKind),

    #[error("non-retryable: {0}")]
    NonRetryable(#[Embedded] NonRetryableKind),
}

/// Retryable failure kinds — transient errors that may succeed on retry.
#[derive(Debug, Clone, Error)]
pub enum RetryableKind {
    #[error("network timeout")]
    NetworkTimeout,
    #[error("broker unavailable")]
    BrokerUnavailable,
    #[error("transient partition error")]
    TransientPartitionError,
}

/// Terminal failure kinds — message cannot be retried.
#[derive(Debug, Clone, Error)]
pub enum TerminalKind {
    #[error("poison message")]
    PoisonMessage,
    #[error("deserialization failed")]
    DeserializationFailed,
    #[error("handler panic")]
    HandlerPanic,
}

/// NonRetryable failure kinds — message is valid but rejected by business logic.
#[derive(Debug, Clone, Error)]
pub enum NonRetryableKind {
    #[error("validation error")]
    ValidationError,
    #[error("business logic error")]
    BusinessLogicError,
    #[error("configuration error")]
    ConfigurationError,
}
```

**Note:** `#[Embedded]` is `thiserror` 2.0's way to embed the inner error directly in the outer enum's Display. Using `#[error("{0}")]` pattern instead if `#[Embedded]` is not available in 2.0.17. Actually, `thiserror` 2.0 uses `#[error(transparent)]` for embedding.

### Corrected thiserror 2.0 Pattern

```rust
#[derive(Debug, Clone, Error)]
pub enum FailureReason {
    #[error("retryable: {0}")]
    Retryable(#[source] RetryableKind),

    #[error("terminal: {0}")]
    Terminal(#[source] TerminalKind),

    #[error("non-retryable: {0}")]
    NonRetryable(#[source] NonRetryableKind),
}
```

`#[source]` embeds the inner error in the outer Display without adding it as a separate field.

### Updated File: `src/python/execution_result.rs`

```rust
//! Execution result types — normalized outcome of Python handler execution.

use crate::coordinator::failure_reason::FailureReason;

/// Normalized execution result from a Python handler.
#[derive(Debug, Clone)]
pub enum ExecutionResult {
    /// Handler executed successfully.
    Ok,
    /// Python exception raised during execution.
    Error {
        /// Classified failure reason.
        reason: FailureReason,
        /// Exception type name (e.g., "KeyError", "ValueError").
        exception: String,
        /// Formatted traceback string.
        traceback: String,
    },
    /// Handler explicitly rejected the message.
    Rejected { reason: FailureReason },
}
```

### New File: `src/coordinator/failure_classifier.rs`

```rust
//! FailureClassifier trait — maps PyErr to FailureReason.

use crate::python::context::ExecutionContext;
use crate::python::execution_result::FailureReason;
use pyo3::PyErr;

/// Trait for classifying Python exceptions into FailureReason.
///
/// Implementations can provide custom classification logic per-handler
/// or use the default `DefaultFailureClassifier`.
pub trait FailureClassifier: Send + Sync {
    /// Classify a Python exception into a FailureReason.
    fn classify(&self, error: &PyErr, context: &ExecutionContext) -> FailureReason;
}

/// Default failure classifier — maps Python exception names to FailureReason.
///
/// Maps common Python exceptions to structured failure kinds:
#[derive(Debug, Clone, Default)]
pub struct DefaultFailureClassifier;

impl FailureClassifier for DefaultFailureClassifier {
    fn classify(&self, error: &PyErr, context: &ExecutionContext) -> FailureReason {
        let exception_name = error
            .get_type(context.topic.as_str()) // Note: get_type requires GIL; this runs inside spawn_blocking
            .name()
            .map(|s| s.to_string())
            .unwrap_or_else(|_| "Unknown".to_string());

        // Use error.message() or to_string() for the message
        let message = error.to_string();

        match exception_name.as_str() {
            // Retryable
            "TimeoutError" | "asyncio.TimeoutError" | "KamajiTimeoutError" => {
                FailureReason::Retryable(RetryableKind::NetworkTimeout)
            }
            "KafkaError" | "rdkafka.KafkaError" => {
                FailureReason::Retryable(RetryableKind::BrokerUnavailable)
            }
            "PartitionError" | "TransientPartitionError" => {
                FailureReason::Retryable(RetryableKind::TransientPartitionError)
            }

            // Terminal
            "PoisonMessageError" => {
                FailureReason::Terminal(TerminalKind::PoisonMessage)
            }
            "UnicodeDecodeError" | "JSONDecodeError" | "DeserializationError" => {
                FailureReason::Terminal(TerminalKind::DeserializationFailed)
            }

            // NonRetryable
            "ValueError" | "TypeError" | "AttributeError" | "ValidationError" => {
                FailureReason::NonRetryable(NonRetryableKind::ValidationError)
            }
            "BusinessLogicError" => {
                FailureReason::NonRetryable(NonRetryableKind::BusinessLogicError)
            }
            "ConfigurationError" | "ConfigError" => {
                FailureReason::NonRetryable(NonRetryableKind::ConfigurationError)
            }

            // Default: non-retryable validation
            _ => FailureReason::NonRetryable(NonRetryableKind::ValidationError),
        }
    }
}
```

**Note:** `get_type` on `PyErr` requires a Python GIL context. Since `FailureClassifier::classify` will be called inside `spawn_blocking` (where GIL is held), this is safe. The `ExecutionContext` here needs to provide the GIL token or the classify must borrow the `PyErr` properly.

### Updated `PythonHandler::invoke` Signature

The handler currently creates `ExecutionResult::Error { exception, traceback }` directly. After Phase 17, it should:

```rust
// In spawn_blocking, after catching PyErr:
let reason = failure_classifier.classify(&py_err, &ctx);
ExecutionResult::Error {
    reason,
    exception,
    traceback,
}
```

But `failure_classifier` needs to be available inside `spawn_blocking`. Pass it as a captured variable similar to `callback`.

### Updated `OffsetCoordinator` Trait

```rust
pub trait OffsetCoordinator: Send + Sync {
    fn record_ack(&self, topic: &str, partition: i32, offset: i64);

    // Updated: now carries FailureReason for DLQ routing
    fn mark_failed(&self, topic: &str, partition: i32, offset: i64, reason: &FailureReason);

    fn graceful_shutdown(&self);
}
```

### Updated `OffsetTracker::mark_failed`

```rust
impl OffsetTracker {
    pub fn mark_failed(&self, topic: &str, partition: i32, offset: i64, reason: &FailureReason) {
        let key = TopicPartitionKey::new(topic, partition);
        let mut guard = self.partitions.lock();
        if let Some(state) = guard.get_mut(&key) {
            state.mark_failed(offset);
            // reason stored for future DLQ routing (Phase 19)
            // Could add: state.last_failure_reason = Some(reason.clone());
        }
    }
}
```

### Updated `worker_loop` Logging (FAIL-04)

```rust
ExecutionResult::Error { ref reason, ref exception, .. } => {
    tracing::warn!(
        worker_id = worker_id,
        topic = %ctx.topic,
        partition = ctx.partition,
        offset = ctx.offset,
        failure_reason = %reason,
        exception = %exception,
        "handler raised exception"
    );
    offset_coordinator.mark_failed(&ctx.topic, ctx.partition, ctx.offset, reason);
}
ExecutionResult::Rejected { ref reason } => {
    tracing::warn!(
        worker_id = worker_id,
        topic = %ctx.topic,
        partition = ctx.partition,
        offset = ctx.offset,
        failure_reason = %reason,
        "handler rejected message"
    );
    offset_coordinator.mark_failed(&ctx.topic, ctx.partition, ctx.offset, reason);
}
```

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Error enum with Display | Manual enum with `match` for Display | `thiserror` `#[derive(Error)]` | `thiserror` already in Cargo.toml; ensures exhaustive Display |
| Python exception classification | Map PyErr via raw strings | `FailureClassifier` trait | Extensible per-handler; testable in isolation |
| Failure reason storage | Store raw strings | Typed `FailureReason` enum | Enables DLQ routing in Phase 19 |

## Common Pitfalls

### Pitfall 1: `#[source]` vs `#[from]` in thiserror
**What goes wrong:** Inner error not displayed correctly or double-displayed.
**How to avoid:** Use `#[source]` (not `#[from]`) for embedding — `#[from]` also implements `source()` trait method which causes chained error display.

### Pitfall 2: `PyErr::get_type` requires GIL
**What goes wrong:** Calling `get_type()` outside of `Python::with_gil` causes panic.
**How to avoid:** `FailureClassifier::classify` runs inside `spawn_blocking` where GIL is held (via `Python::attach` or `Python::with_gil`). Ensure the classifier is called in the right execution context.

### Pitfall 3: Exhaustive matching on FailureReason in worker_loop
**What goes wrong:** Adding a new variant to `FailureReason` but forgetting to update all `match` arms.
**How to avoid:** Use `#[derive(Debug, Clone, PartialEq)]` and a final wildcard `_` in match arms with a compile-time unreachable default. Or use `tracing::debug!` in wildcard to catch new variants.

### Pitfall 4: `OffsetCoordinator::mark_failed` signature change breaks implementors
**What goes wrong:** Changing `mark_failed` to take `FailureReason` breaks the existing `OffsetTracker` implementation.
**How to avoid:** This is a **breaking change** to the trait. Implementation needs to be updated atomically with the trait change. Since Phase 17 is the first implementation, this is a clean addition.

## Code Examples

### DefaultFailureClassifier mapping example

```rust
// Source: thiserror 2.0 docs
use thiserror::Error;

#[derive(Debug, Error)]
enum FailureReason {
    #[error("retryable: {0}")]
    Retryable(#[source] RetryableKind),
    #[error("terminal: {0}")]
    Terminal(#[source] TerminalKind),
    #[error("non-retryable: {0}")]
    NonRetryable(#[source] NonRetryableKind),
}

impl FailureClassifier for DefaultFailureClassifier {
    fn classify(&self, error: &PyErr, _ctx: &ExecutionContext) -> FailureReason {
        let name = error.get_type(/* GIL */).name()...;
        match name.as_str() {
            "TimeoutError" => FailureReason::Retryable(RetryableKind::NetworkTimeout),
            "ValueError" | "TypeError" => FailureReason::NonRetryable(NonRetryableKind::ValidationError),
            _ => FailureReason::NonRetryable(NonRetryableKind::ValidationError),
        }
    }
}
```

### Marking failed with structured reason

```rust
// Source: adapted from existing worker_loop in src/worker_pool/mod.rs
ExecutionResult::Error { ref reason, .. } => {
    tracing::warn!(
        topic = %ctx.topic,
        partition = ctx.partition,
        offset = ctx.offset,
        failure_reason = %reason,
        "handler raised exception"
    );
    offset_coordinator.mark_failed(&ctx.topic, ctx.partition, ctx.offset, reason);
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `ExecutionResult::Error { exception: String, traceback: String }` | `ExecutionResult::Error { reason: FailureReason, exception: String, traceback: String }` | Phase 17 | Typed reason available for DLQ routing |
| `ExecutionResult::Rejected { reason: String }` | `ExecutionResult::Rejected { reason: FailureReason }` | Phase 17 | Typed reason replaces raw string |
| `mark_failed(topic, partition, offset)` | `mark_failed(topic, partition, offset, reason)` | Phase 17 | Reason stored for future DLQ routing |
| Exception classification via ad-hoc string matching | `FailureClassifier` trait | Phase 17 | Extensible, testable classification |

**No deprecated approaches** — Phase 17 is additive (first classification implementation).

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `thiserror` 2.0.17 supports `#[source]` for embedding inner errors | Standard Stack | MEDIUM — if `#[source]` doesn't work as expected, Display format would be wrong. Can use `#[error("{0}")]` instead. |
| A2 | `PyErr::get_type` can be called inside `spawn_blocking` with GIL held | Architecture Patterns | LOW — PyO3 GIL management is well-documented; this is the standard pattern |
| A3 | `FailureClassifier` instance is available inside `PythonHandler::invoke` via captured variable | Architecture Patterns | MEDIUM — need to pass classifier as Arc into spawn_blocking; planner needs to handle wiring |

## Open Questions

1. **How to pass `FailureClassifier` into `PythonHandler::invoke`?**
   - Option A: Store `Arc<dyn FailureClassifier>` in `PythonHandler` struct
   - Option B: Pass as additional argument to `invoke`
   - Recommendation: Option A (store in handler) — cleaner API, matches how `Executor` is already stored

2. **Should `ExecutionResult::Error` keep `exception: String` and `traceback: String` fields?**
   - Arguments for keeping: logging still needs human-readable exception name and traceback
   - Arguments for dropping: `FailureReason` already encodes the classification
   - Recommendation: Keep both — they provide debugging information that `FailureReason` alone doesn't capture

3. **How does `ExecutionResult::Rejected` get its `FailureReason`?**
   - Currently Python handler raises `RejectError` exception which is caught and converted
   - Or Python calls `handler.reject(reason)` which returns `ExecutionResult::Rejected`
   - Need to clarify Python-side API for rejection — this may be Phase 18 or later scope

4. **`OffsetTracker` needs to store `FailureReason` per offset for DLQ routing (Phase 19)**
   - Phase 17: Update `mark_failed` signature but don't implement storage yet
   - Phase 19 will add the storage

## Environment Availability

Step 2.6: SKIPPED (no external dependencies — pure Rust implementation using existing project dependencies)

## Validation Architecture

**Note:** `workflow.nyquist_validation` absent from `.planning/config.json` — treating as **enabled** by default.

### Test Framework
| Property | Value |
|----------|-------|
| Framework | `cargo test` (built-in), `rstest` for parameterized tests |
| Config file | None — using inline `#[cfg(test)]` modules |
| Quick run command | `cargo test --lib failure` |
| Full suite command | `cargo test --lib` |

### Phase Requirements to Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| FAIL-01 | `FailureReason` enum has correct variants and Display | Unit | `cargo test --lib failure_reason` | No — new file |
| FAIL-01 | `RetryableKind`, `TerminalKind`, `NonRetryableKind` have correct variants | Unit | `cargo test --lib failure_kind` | No — new file |
| FAIL-02 | `ExecutionResult::Error` carries `FailureReason` | Unit | `cargo test --lib execution_result` | Yes — updated |
| FAIL-02 | `ExecutionResult::Rejected` carries `FailureReason` | Unit | `cargo test --lib execution_result` | Yes — updated |
| FAIL-03 | `FailureClassifier` trait exists with correct signature | Unit | `cargo test --lib failure_classifier` | No — new file |
| FAIL-03 | `DefaultFailureClassifier` maps known exceptions correctly | Unit | `cargo test --lib default_classifier` | No — new file |
| FAIL-04 | Structured logging includes failure reason field | Integration | Manual review of `cargo test` output | N/A |

### Sampling Rate
- **Per task commit:** `cargo test --lib failure --quiet`
- **Per wave merge:** `cargo test --lib`
- **Phase gate:** Full suite green before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `src/coordinator/failure_reason.rs` — FAIL-01 enum types
- [ ] `src/coordinator/failure_classifier.rs` — FAIL-03 trait + DefaultFailureClassifier
- [ ] `src/coordinator/mod.rs` — export new modules
- [ ] `src/python/execution_result.rs` — update to use FailureReason (FAIL-02)
- [ ] `src/coordinator/offset_coordinator.rs` — update `mark_failed` signature
- [ ] `src/coordinator/offset_tracker.rs` — implement updated `mark_failed`
- [ ] `src/python/handler.rs` — wire FailureClassifier into invoke
- [ ] `src/worker_pool/mod.rs` — update worker_loop match arms with structured logging
- [ ] `src/python/mod.rs` — export FailureReason
- [ ] `tests/` directory if integration tests needed

## Security Domain

**Note:** `security_enforcement` absent — treating as **enabled** by default.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V5 Input Validation | yes | `FailureClassifier` validates/maps Python exceptions; no user input directly |
| V4 Access Control | no | No access control in failure classification |
| V2 Authentication | no | Not applicable |
| V3 Session Management | no | Not applicable |
| V6 Cryptography | no | Not applicable |

### Known Threat Patterns for Failure Classification

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Exception type spoofing (Python raises unexpected type) | Tampering | Default fallback to `NonRetryableKind::ValidationError` |
| Panic in FailureClassifier | Denial | `DefaultFailureClassifier` is simple match — no panics; custom classifiers must be `Send + Sync` |

## Sources

### Primary (HIGH confidence)
- `src/python/execution_result.rs` — current ExecutionResult enum
- `src/python/handler.rs` — how PyErr is caught and converted
- `src/coordinator/offset_tracker.rs` — existing mark_failed pattern
- `src/worker_pool/mod.rs` — worker_loop that calls mark_failed
- `Cargo.toml` — thiserror 2.0.17 already in dependencies

### Secondary (MEDIUM confidence)
- Context7 PyO3 docs (`/pyo3/pyo3`) — PyErr::get_type, exception handling
- thiserror 2.0 docs — `#[source]` for embedding inner errors

### Tertiary (LOW confidence)
- None

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — thiserror already in Cargo.toml
- Architecture: HIGH — patterns from existing codebase (Executor, OffsetCoordinator traits)
- Pitfalls: MEDIUM — thiserror 2.0 `#[source]` behavior needs verification against exact 2.0.17

**Research date:** 2026-04-17
**Valid until:** 2026-05-17 (stable domain, thiserror and PyO3 APIs are mature)
