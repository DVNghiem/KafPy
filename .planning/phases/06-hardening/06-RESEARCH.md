# Phase 6: Hardening - Research

**Researched:** 2026-04-29
**Domain:** Rust error handling, thiserror derive patterns, rich error context injection
**Confidence:** HIGH

## Summary

Phase 6 (Hardening) requires three things: (1) richer error messages with context, (2) Debug impls on all error-context structs, and (3) confirmed build-time validation via the existing builder pattern. The codebase already has a solid foundation -- `thiserror` 2.0.17 is in use, several error types already derive Debug, and `ConsumerConfigBuilder::build()` already returns `Result<T, BuildError>`. The primary work is adding context fields to error variants that currently just hold raw strings, and adding `Debug` impls to any structs used as error context that don't have them yet.

**Primary recommendation:** Replace `#[error("...")]` string-only variants in `ConsumerError`, `DispatchError`, and `PyError` with structured fields carrying topic/partition/offset/broker data. Add `Debug` to remaining structs used in error paths (notably the consumer `Context` types, `DispatchOutcome`, and `RoutingContext`).

---

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Rich error messages (LH-05) | API/Backend | -- | Error types live in Rust core modules |
| Debug impls (LH-06) | API/Backend | -- | All error structs are Rust types |
| Build-time validation (LH-07) | API/Backend | -- | ConsumerConfigBuilder::build() does this already |

---

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| thiserror | 2.0.17 | Derive `Error + Debug` on error enums | [VERIFIED: Cargo.toml line 24] |
| rdkafka | 0.38 | Kafka client, exposes KafkaError | [VERIFIED: Cargo.toml lines 40-43] |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| anyhow | 1.0 | Application-level error context | NOT for library error types (per rust/coding-style.md) |
| tracing | 0.1 | Structured logging in errors | Span context on error paths |

---

## Architecture Patterns

### System Architecture Diagram

```
Python Consumer Process
├── PyConsumer (pyconsumer.rs)
│   ├── start() → RuntimeBuilder::build()
│   └── add_handler() → stores HandlerMetadata
│
├── RuntimeBuilder assembles:
│   ├── Consumer (rdkafka BaseConsumer)
│   ├── ConsumerDispatcher (routes to handler channels)
│   ├── WorkerPool (N Tokio tasks)
│   └── OffsetTracker + RetryCoordinator + DlqProducer
│
└── Error flow:
    ├── Kafka connect/broker error → KafkaError → ConsumerError::Kafka
    ├── Deserialization error → OwnedMessage → DispatchError::Deserialization(topic, partition, offset, bytes_preview)
    ├── Handler timeout → HandlerMetadata.name → TimeoutError(handler_name, timeout_ms)
    └── Config build error → BuildError → returned from build()
```

### Recommended Project Structure
No structural changes needed -- error types live in existing modules.

---

## Pattern 1: thiserror with Contextual Fields

**What:** Use `#[error("...")]` with named fields so error messages include structured context.

**When to use:** When an error variant needs to carry multiple pieces of contextual data (topic, partition, offset, etc.)

**Example:**
```rust
// CURRENT (string-only, poor context):
#[derive(Error, Debug)]
pub enum ConsumerError {
    #[error("serialization error: {0}")]
    Serialization(String),
}

// TARGET (structured fields, rich context):
#[derive(Error, Debug)]
pub enum ConsumerError {
    #[error("deserialization failed for topic '{topic}' partition {partition} offset {offset}: {source}")]
    Deserialization {
        topic: String,
        partition: i32,
        offset: i64,
        #[source]
        source: serde_json::Error,
    },
}
```

### thiserror v2.0 Transparent Errors
```rust
// For wrapping inner errors without adding a new message:
#[error(transparent)]
Kafka(#[from] rdkafka::error::KafkaError),
```

### thiserror v2.0 Multiple Source Fields
```rust
#[derive(Error, Debug)]
pub enum ComplexError {
    #[error("handler timeout for '{handler}' after {timeout_ms}ms")]
    Timeout {
        handler: String,
        timeout_ms: u64,
        #[source]
        source: tokio::time::error::Elapsed,
    },
}
```

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Error type with Display | Manual `impl Display` | `#[derive(thiserror::Error)]` | thiserror auto-derives Error + Display + Debug when possible |
| Error with source chain | Manual `impl Error::source` | `#[source]` or `#[from]` | thiserror handles source chaining automatically |
| Rich error context in messages | `format!("error: {x}")` in strings | Named fields in `#[error()]` | `thiserror` formats named fields directly |

---

## Common Pitfalls

### Pitfall 1: Stringly-typed Error Variants
**What goes wrong:** `ConsumerError::Serialization(String)` produces error messages like "serialization error: foo" with no context about which topic/partition/offset caused the failure.
**Why it happens:** Early error designs used `String` for simplicity; context fields were not anticipated.
**How to avoid:** When adding a new error variant, always ask: what context would a debugging a production incident need?
**Warning signs:** `#[error("...{0}")]` with no structured fields.

### Pitfall 2: Missing Debug on Context Structs Used in Errors
**What goes wrong:** A struct like `DispatchOutcome { topic, partition, offset, queue_depth }` is used as error context but has no `Debug` impl, making it hard to inspect in logs.
**Why it happens:** `#[derive(Debug)]` is omitted on struct definitions.
**How to avoid:** Audit all structs used as error context fields -- they need `Debug`.
**Warning signs:** `struct Foo { ... }` without `#[derive(Debug)]`.

### Pitfall 3: `#[error(transparent)]` used for non-opaque errors
**What goes wrong:** `#[error(transparent)]` forwards Display/Error to the inner type with no added message. If the inner type's message is already opaque (e.g., rdkafka's KafkaError), this may not add enough context.
**How to avoid:** Use transparent only when you genuinely want to forward without modification. For Kafka errors, prefer adding broker/operation context.

### Pitfall 4: thiserror v2.0 Breaking Change from v1.x
**What goes wrong:** `thiserror` v2.0 has breaking changes from v1.x (notably around `source` attribute changes).
**How to avoid:** The codebase is already on 2.0.17 -- ensure any new code follows v2.0 patterns.
**Evidence:** [VERIFIED: Cargo.toml line 24]

---

## Code Examples

### Rich Deserialization Error with Source and Context
```rust
// src/consumer/error.rs
#[derive(Error, Debug)]
pub enum ConsumerError {
    // Replace:
    // #[error("serialization error: {0}")]
    // Serialization(String),

    // With:
    #[error("deserialization failed for message at {topic}:{partition}@{offset}: {source}")]
    Deserialization {
        topic: String,
        partition: i32,
        offset: i64,
        #[source]
        source: serde_json::Error,
    },
}
```

### Rich Timeout Error with Handler Name and Duration
```rust
// In the handler execution path (worker.rs or python/executor.rs):
#[derive(Error, Debug)]
#[error("handler '{handler}' timed out after {timeout_ms}ms")]
pub struct HandlerTimeoutError {
    pub handler: String,
    pub timeout_ms: u64,
    #[source]
    pub source: tokio::time::error::Elapsed,
}
```

### Rich Connection Error with Broker Address
```rust
// In consumer/error.rs or runtime/builder.rs:
#[derive(Error, Debug)]
pub enum ConnectionError {
    #[error("failed to connect to broker '{broker}': {source}")]
    BrokerConnection {
        broker: String,
        #[source]
        source: rdkafka::error::KafkaError,
    },

    #[error("topic '{topic}' not found at broker '{broker}'")]
    TopicNotFound {
        broker: String,
        topic: String,
    },
}
```

### Debug on DispatchOutcome (error context struct)
```rust
// src/dispatcher/mod.rs -- already has Debug via #[derive]
#[derive(Debug, Clone)]
pub struct DispatchOutcome {
    pub topic: String,
    pub partition: i32,
    pub offset: i64,
    pub queue_depth: usize,
}
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `#[error("{0}")]` with raw String | Named fields in `#[error()]` | thiserror 2.0 (2024) | Structured context in error messages |
| Manual Error impl | `#[derive(thiserror::Error)]` | thiserror 0.1 (2019) | Eliminates boilerplate |
| Opaque error wrapping | `#[error(transparent)]` | thiserror 1.0+ | Clean source chain forwarding |

---

## Runtime State Inventory

> Not applicable -- Phase 6 is a code improvement phase, not a rename/migration.

---

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Deserialization errors originate in the consumer core (not PyO3 boundary) | LH-05 | Would need error enrichment at Python layer instead |
| A2 | Handler name is available in HandlerMetadata at timeout detection point | LH-05 | Timeout error would only show topic, not handler name |
| A3 | `serde_json::Error` is the primary deserialization error source | LH-05 | May need to also support msgpack/other decoders |
| A4 | ConsumerError::Kafka wraps rdkafka::error::KafkaError via `#[from]` | LH-05 | Connection error context would need different pattern |

---

## Open Questions

1. **Where does bytes preview for deserialization errors come from?**
   - What we know: `OwnedMessage` has `payload: Option<Vec<u8>>` in `consumer/message.rs`
   - What's unclear: Maximum bytes to include in error message (10, 100, 256?)
   - Recommendation: Limit to first 128 bytes, hex-encoded, e.g., `bytes_preview: [0x7b, 0x22, ...]`

2. **Should DispatchError variants get structured fields or use existing string fields?**
   - What we know: `DispatchError::UnknownTopic(String)` only stores topic name
   - What's unclear: Whether adding partition/offset to UnknownTopic variant is worth the churn
   - Recommendation: Yes -- add `(topic: String, partition: i32, offset: i64)` to UnknownTopic

3. **PyError variants are string-only -- should they be structured too?**
   - What we know: `PyError::Consumer(String)` wraps ConsumerError as string
   - What's unclear: Whether Python users need rich PyError or if ConsumerError improvement is sufficient
   - Recommendation: Add context fields to PyError too since it surfaces at the PyO3 boundary

---

## Environment Availability

Step 2.6: SKIPPED (no external dependencies -- pure Rust code changes)

---

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust standard `#[test]` + `#[cfg(test)]` |
| Config file | N/A -- inline test modules |
| Quick run command | `cargo test --lib -- --test-threads=1` |
| Full suite command | `cargo test` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| LH-05 | Error messages include broker/topic/partition/offset | Unit | `cargo test error` | tests/ |
| LH-06 | All public error structs implement Debug | Compile-time | `cargo test --lib` | src/*/mod.rs |
| LH-07 | Missing required fields produces build error | Unit | `cargo test build_error` | tests/ |

### Sampling Rate
- **Per task commit:** `cargo test --lib -- error`
- **Per wave merge:** `cargo test`
- **Phase gate:** Full suite green before `/gsd-verify-work`

### Wave 0 Gaps
- `tests/hardening_error_test.rs` -- error message validation tests (LH-05)
- `tests/debug_impl_test.rs` -- compile-time Debug impl assertions (LH-06)
- `tests/build_validation_test.rs` -- build-time field validation tests (LH-07)

---

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V5 Input Validation | yes | Deserialization errors with bytes preview help diagnose bad input |
| V4 Access Control | no | Not auth-related |
| V2 Authentication | no | Not auth-related |

### Known Threat Patterns for this stack

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Malformed Kafka message causing deserialization panic | Denial | Handler timeout + retry exhaustion route to DLQ |
| Connection string leaking broker address in logs | Information Disclosure | Error messages redact credentials if embedded in broker URL |

---

## Sources

### Primary (HIGH confidence)
- [Context7: thiserror 2.0.18 docs](https://docs.rs/thiserror/2.0.18/thiserror/index.html) - `#[error(transparent)]`, `#[source]`, named field patterns
- [Cargo.toml](file:///home/nghiem/project/KafPy/Cargo.toml) - thiserror version 2.0.17 confirmed
- [src/config.rs](file:///home/nghiem/project/KafPy/src/config.rs) - existing BuildError pattern with thiserror
- [src/consumer/config.rs](file:///home/nghiem/project/KafPy/src/consumer/config.rs) - existing BuildError pattern
- [src/consumer/error.rs](file:///home/nghiem/project/KafPy/src/consumer/error.rs) - existing ConsumerError with KafkaError::from

### Secondary (MEDIUM confidence)
- [Rust rules: coding-style.md](file:///home/nghiem/.claude/rules/rust/coding-style.md) - thiserror vs anyhow guidance
- [Rust rules: error handling](file:///home/nghiem/.claude/rules/rust/coding-style.md) - Error handling best practices

### Tertiary (LOW confidence)
- [WebSearch: rdkafka Rust error handling patterns](https://docs.rs/rdkafka/0.38) - KafkaError source patterns (marked for validation)

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - thiserror 2.0.17 confirmed in Cargo.toml, all error files reviewed
- Architecture: HIGH - error types already in correct modules, no restructuring needed
- Pitfalls: HIGH - stringly-typed variants identified by direct code review

**Research date:** 2026-04-29
**Valid until:** 2026-05-29 (thiserror v2 stable, no upcoming breaking changes expected)
