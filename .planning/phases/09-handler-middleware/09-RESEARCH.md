# Phase 9: Handler Middleware - Research

**Researched:** 2026-04-29
**Domain:** Rust middleware trait design, middleware composition, Python-Rust bridge for middleware configuration
**Confidence:** HIGH (based on existing codebase patterns)

## Summary

Phase 9 implements a `HandlerMiddleware` trait with before/after/on_error hooks that wraps Python handler invocation in the worker loop. Middleware chains are composed per-handler via the Python `@handler(middleware=[...])` API. Two built-in middleware provide observability: `Logging` (tracing span events) and `Metrics` (latency histogram + throughput counter). The design uses a decorator/wrapper pattern rather than Tower's `Service+Layer` because the handler invocation is an async method on `PythonHandler`, not a trait object. Middleware receive `&ExecutionContext` for rich metadata and return `Option<ExecutionResult>` from the error hook to allow recovery.

**Primary recommendation:** Implement `HandlerMiddleware` as a `Send + Sync` trait with `before(ctx)`, `after(ctx, result, elapsed)`, and `on_error(ctx, error)`. Chain middleware by storing `Vec<Box<dyn HandlerMiddleware>>` on `PythonHandler` and calling them in sequence around the handler invocation. Python middleware classes expose the same hooks via PyO3.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| HandlerMiddleware trait | Rust API/Backend | — | Core abstraction lives in Rust |
| Logging middleware | Rust API/Backend | — | Emits tracing spans, uses existing KafpySpanExt |
| Metrics middleware | Rust API/Backend | — | Records to PrometheusSink via existing HandlerMetrics |
| Middleware chain execution | Rust API/Backend | — | Executed in worker loop around handler.invoke |
| Python middleware binding | PyO3 bridge | — | Python classes implement Rust trait via PyO3 |
| Python `@handler(middleware=)` | Python API | — | Passes middleware list to Rust via PyConsumer.add_handler |

## User Constraints (from CONTEXT.md)

*No CONTEXT.md exists for Phase 9 — skipped.*

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| MIDW-01 | `HandlerMiddleware` trait in Rust with before/after/on_error hooks | Trait design below; hooks match worker loop phases |
| MIDW-02 | Built-in logging middleware (span events on handler start/complete/error) | Reuses existing `KafpySpanExt::kafpy_handler_invoke` spans |
| MIDW-03 | Built-in metrics middleware (latency histogram, throughput counter) | Reuses existing `HandlerMetrics::record_latency` + `ThroughputMetrics::record_throughput` |
| MIDW-04 | Python API `@handler(middleware=[Logging(), Metrics()])` per handler | PyO3 `add_handler` already accepts per-handler config; middleware list extends it |

## Standard Stack

### Core (No new dependencies)
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| Existing `tracing` | 0.1 | Span creation for logging middleware | Already used for `kafpy_handler_invoke` spans |
| Existing `prometheus_client` | 0.24 | Latency histogram + throughput counter | Already used for `kafpy.handler.latency` and `kafpy.message.throughput` |

**No new crates required.**

### Installation
```bash
# No new crates — all dependencies already present
```

## Architecture Patterns

### System Architecture Diagram

```
Worker Loop (worker.rs)
    │
    ├── Extract trace context from message headers
    ├── Construct ExecutionContext
    │
    ├── [MIDDLEWARE CHAIN] ──────────────────────┐
    │   middleware[0].before(ctx)                │
    │   middleware[1].before(ctx)                │
    │   ...                                      │
    │   middleware[N-1].before(ctx)              │
    │         │                                  │
    │         ▼                                  │
    │   PythonHandler.invoke_mode_with_timeout()│
    │         │                                  │
    │   Result + elapsed time                   │
    │         │                                  │
    │   middleware[N-1].after(ctx, result, elap) │
    │   middleware[N-2].after(ctx, result, elap) │
    │   ...                                      │
    │   middleware[0].after(ctx, result, elap)  │
    │                                            │
    │   [on_error path]                         │
    │   middleware[N-1].on_error(ctx, error)     │
    │   middleware[N-2].on_error(ctx, error)     │
    │   ...                                      │
    │   middleware[0].on_error(ctx, error)      │
    └────────────────────────────────────────────┘
    │
    ├── executor.execute(ctx, msg, result)
    ├── RetryCoordinator / OffsetCoordinator
    └── QueueManager.ack / ack
```

**Key insight:** Middleware are called in reverse order on `after` and `on_error` (inner-to-outer), matching standard decorator composition. `before` executes in natural order (outer-to-inner). This is the same pattern as Go's `net/http` middleware and Python's WSGI middleware.

### Recommended Project Structure
```
src/
├── middleware/
│   ├── mod.rs              # HandlerMiddleware trait, MiddlewareChain
│   ├── traits.rs           # HandlerMiddleware trait definition
│   ├── chain.rs           # MiddlewareChain (Vec<Box<dyn HandlerMiddleware>>)
│   ├── logging.rs         # Built-in Logging middleware
│   └── metrics.rs         # Built-in Metrics middleware
```

### Pattern 1: HandlerMiddleware Trait
**What:** `Send + Sync` trait with three optional hooks called around handler invocation
**When to use:** Any cross-cutting concern (logging, metrics, retries, tracing)
**Example:**
```rust
// Source: KafPy codebase — src/python/handler.rs worker loop pattern + MIDW-01 design
use crate::python::context::ExecutionContext;
use crate::python::execution_result::ExecutionResult;
use std::time::Duration;

/// User-implemented middleware trait for cross-cutting handler concerns.
/// All methods are optional — default implementations are no-ops.
/// Middleware are composed in a chain; after/on_error run in reverse order.
pub trait HandlerMiddleware: Send + Sync {
    /// Called before the handler is invoked.
    /// Use for: pre-processing, span creation, resource acquisition.
    fn before(&self, _ctx: &ExecutionContext) {}

    /// Called after the handler succeeds or fails.
    /// Use for: logging, metrics recording, cleanup.
    /// `elapsed` is the total wall-clock time since before() was called.
    fn after(&self, _ctx: &ExecutionContext, _result: &ExecutionResult, _elapsed: Duration) {}

    /// Called when the handler invocation returns an error result.
    /// Use for: error-specific logging, alerting, error-metric increment.
    /// Default implementation calls `after` for consistent middleware that handle both success and error.
    fn on_error(&self, _ctx: &ExecutionContext, _result: &ExecutionResult) {}
}
```

**Anti-pattern to avoid:** Middleware that mutate the `ExecutionResult` in-place. Use the `on_error` hook for error-specific handling, not result mutation.

### Pattern 2: MiddlewareChain Composition
**What:** `MiddlewareChain` holds ordered middleware and delegates through them
**When to use:** When multiple middleware are registered per handler
**Example:**
```rust
// Source: MIDW-01 design based on existing worker.rs span pattern
pub struct MiddlewareChain {
    middleware: Vec<Box<dyn HandlerMiddleware>>,
}

impl MiddlewareChain {
    pub fn new() -> Self {
        Self { middleware: Vec::new() }
    }

    pub fn add(mut self, m: Box<dyn HandlerMiddleware>) -> Self {
        self.middleware.push(m);
        self
    }

    /// Call before() on all middleware in order.
    pub fn before_all(&self, ctx: &ExecutionContext) {
        for m in &self.middleware {
            m.before(ctx);
        }
    }

    /// Call after() on all middleware in reverse order.
    pub fn after_all(&self, ctx: &ExecutionContext, result: &ExecutionResult, elapsed: Duration) {
        for m in self.middleware.iter().rev() {
            m.after(ctx, result, elapsed);
        }
    }

    /// Call on_error() on all middleware in reverse order.
    pub fn on_error_all(&self, ctx: &ExecutionContext, result: &ExecutionResult) {
        for m in self.middleware.iter().rev() {
            m.on_error(ctx, result);
        }
    }
}
```

### Pattern 3: Logging Middleware (MIDW-02)
**What:** Emits `tracing` span events on handler start/complete/error with trace context fields
**When to use:** Built-in observability for every handler invocation
**Example:**
```rust
// Source: existing worker.rs lines 134-156 + KafpySpanExt::kafpy_handler_invoke
use crate::observability::tracing::KafpySpanExt;

/// Built-in logging middleware — MIDW-02.
///
/// Emits span events using existing kafpy_handler_invoke span fields.
/// Uses Python logging (via kafpy logger) so output goes to user's configured handler.
/// Zero-cost when no tracing subscriber is configured.
pub struct Logging;

impl HandlerMiddleware for Logging {
    fn before(&self, ctx: &ExecutionContext) {
        tracing::info_span!(
            "kafpy.middleware.logging",
            handler_id = %ctx.topic,
            topic = %ctx.topic,
            partition = ctx.partition,
            offset = ctx.offset,
        )
        .in_scope(|| {
            tracing::info!(
                handler_id = %ctx.topic,
                topic = %ctx.topic,
                partition = ctx.partition,
                offset = ctx.offset,
                "handler middleware: before"
            );
        });
    }

    fn after(&self, ctx: &ExecutionContext, result: &ExecutionResult, elapsed: Duration) {
        tracing::info!(
            handler_id = %ctx.topic,
            topic = %ctx.topic,
            partition = ctx.partition,
            offset = ctx.offset,
            elapsed_ms = elapsed.as_millis() as u64,
            result = %result.error_type_label(),
            "handler middleware: after"
        );
    }

    fn on_error(&self, ctx: &ExecutionContext, result: &ExecutionResult) {
        tracing::error!(
            handler_id = %ctx.topic,
            topic = %ctx.topic,
            partition = ctx.partition,
            offset = ctx.offset,
            error_type = %result.error_type_label(),
            "handler middleware: error"
        );
    }
}
```

### Pattern 4: Metrics Middleware (MIDW-03)
**What:** Records latency histogram and throughput counter per handler invocation
**When to use:** Built-in Prometheus metrics for every handler
**Example:**
```rust
// Source: existing metrics.rs HandlerMetrics + ThroughputMetrics
use crate::observability::metrics::{HandlerMetrics, MetricLabels, SharedPrometheusSink, ThroughputMetrics};

/// Built-in metrics middleware — MIDW-03.
///
/// Records:
/// - kafpy.handler.latency (histogram, seconds)
/// - kafpy.message.throughput (counter, per message)
/// Uses existing HandlerMetrics and ThroughputMetrics types.
pub struct Metrics {
    sink: SharedPrometheusSink,
}

impl Metrics {
    pub fn new(sink: SharedPrometheusSink) -> Self {
        Self { sink }
    }
}

impl HandlerMiddleware for Metrics {
    fn after(&self, ctx: &ExecutionContext, result: &ExecutionResult, elapsed: Duration) {
        let labels = MetricLabels::new()
            .insert("handler_id", ctx.topic.as_str())
            .insert("topic", ctx.topic.as_str());

        HandlerMetrics::record_latency(&self.sink, &labels, elapsed);

        // Only count throughput on success
        if result.is_ok() {
            ThroughputMetrics::record_throughput(
                &self.sink,
                ctx.topic.as_str(),
                ctx.topic.as_str(),
                "SingleSync", // mode is available via ctx if needed
            );
        }
    }
}
```

### Pattern 5: Python Middleware Class Binding
**What:** Python class implements `HandlerMiddleware` via PyO3
**When to use:** User-defined middleware in Python
**Example:**
```python
# Source: MIDW-04 design — Python @handler(middleware=[...]) API
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from .context import ExecutionContext

class BaseMiddleware:
    """Base class for user-defined middleware.

    Subclass and override before(), after(), on_error().
    """

    def before(self, ctx: ExecutionContext) -> None:
        pass

    def after(self, ctx: ExecutionContext, result: str, elapsed_ms: float) -> None:
        pass

    def on_error(self, ctx: ExecutionContext, result: str) -> None:
        pass


class Logging(BaseMiddleware):
    """Built-in logging middleware — emits tracing events."""

    def before(self, ctx: ExecutionContext) -> None:
        # Calls Rust Logging middleware's before()
        pass

    def after(self, ctx: ExecutionContext, result: str, elapsed_ms: float) -> None:
        pass

    def on_error(self, ctx: ExecutionContext, result: str) -> None:
        pass


class Metrics(BaseMiddleware):
    """Built-in metrics middleware — records latency histogram + throughput."""

    def before(self, ctx: ExecutionContext) -> None:
        pass

    def after(self, ctx: ExecutionContext, result: str, elapsed_ms: float) -> None:
        pass

    def on_error(self, ctx: ExecutionContext, result: str) -> None:
        pass
```

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Span creation | Hand-roll span fields manually | Reuse `KafpySpanExt::kafpy_handler_invoke` | Consistent field names, trace context integration |
| Metrics recording | Duplicate prometheus_client calls | Reuse `HandlerMetrics::record_latency` + `ThroughputMetrics::record_throughput` | Already registered in `SharedPrometheusSink::new()` |
| Latency buckets | Define custom bucket arrays | Use existing `LATENCY_BUCKETS` from metrics.rs | Pre-defined for handler latency distribution |

**Key insight:** Existing observability infrastructure (spans, HandlerMetrics, ThroughputMetrics) is pre-registered in `SharedPrometheusSink::new()`. Middleware reuse these, not re-register them.

## Common Pitfalls

### Pitfall 1: Middleware order confusion on after/on_error
**What goes wrong:** `after()` runs in reverse order (inner-to-outer) but developers expect natural order.
**Why it happens:** Decorator pattern — inner middleware wraps the invocation, so its `after` fires first.
**How to avoid:** Document clearly; the reverse order is intentional and matches standard middleware semantics.
**Warning signs:** Metrics showing "inner middleware runs last" or "logging shows wrong order".

### Pitfall 2: Middleware blocking the poll cycle
**What goes wrong:** Synchronous middleware (especially logging to external systems) blocks the Tokio thread.
**Why it happens:** Middleware run inside the async worker loop context.
**How to avoid:** Enforce that middleware MUST be async-safe (no blocking calls). The existing rules already prohibit blocking middleware in async context.

### Pitfall 3: Middleware that panics breaks the chain
**What goes wrong:** If one middleware panics in `before()`, subsequent middleware are never called.
**Why it happens:** No error recovery in chain execution.
**How to avoid:** Wrap each middleware call in a `std::panic::catch_unwind` — middleware errors should be logged, not propagate.

### Pitfall 4: Metrics cardinality explosion
**What goes wrong:** Adding high-cardinality labels (e.g., offset) to metrics causes memory explosion.
**Why it happens:** `MetricLabels::insert()` sorts lexicographically but does not prevent high-cardinality values.
**How to avoid:** Only use `handler_id`, `topic`, `mode`, `error_type` as label keys — never `offset` or `partition`.

### Pitfall 5: GIL boundary crossing in middleware
**What goes wrong:** Middleware that call Python code while the GIL is held causes deadlocks.
**Why it happens:** `invoke_mode_with_timeout` uses `spawn_blocking` or Rayon pool — GIL is released.
**How to avoid:** Middleware run in async Tokio context; they MUST NOT call Python APIs directly.

## Code Examples

### Integrating middleware into PythonHandler.invoke_mode_with_timeout
```rust
// Source: MIDW-01 integration point — wraps handler.invoke_mode_with_timeout in middleware chain
// Existing pattern in worker.rs lines 149-186, refactored to use middleware chain

pub async fn invoke_mode_with_timeout(
    &self,
    ctx: &ExecutionContext,
    message: OwnedMessage,
) -> ExecutionResult {
    let start = std::time::Instant::now();

    // Call before() on all middleware in order
    if let Some(ref chain) = self.middleware_chain {
        chain.before_all(ctx);
    }

    let result = match self.handler_timeout {
        Some(timeout) => {
            match tokio::time::timeout(timeout, self.invoke_mode(ctx, message)).await {
                Ok(result) => result,
                Err(_) => ExecutionResult::Timeout {
                    info: TimeoutInfo {
                        timeout_ms: timeout.as_millis() as u64,
                        last_processed_offset: None,
                    },
                },
            }
        }
        None => self.invoke_mode(ctx, message).await,
    };

    let elapsed = start.elapsed();

    // Call after() on all middleware in reverse order
    if let Some(ref chain) = self.middleware_chain {
        if result.is_ok() {
            chain.after_all(ctx, &result, elapsed);
        } else {
            chain.on_error_all(ctx, &result);
        }
    }

    result
}
```

### PyO3 binding for Python middleware list
```rust
// Source: MIDW-04 — extending PyConsumer.add_handler to accept middleware list
// Based on existing HandlerMetadata pattern in pyconsumer.rs

#[pyclass(name = "HandlerMetadata")]
pub struct HandlerMetadata {
    pub callback: Arc<Py<PyAny>>,
    pub mode: HandlerMode,
    pub batch_max_size: Option<usize>,
    pub batch_max_wait_ms: Option<u64>,
    pub timeout_ms: Option<u64>,
    pub concurrency: Option<usize>,
    pub middleware_chain: Option<Vec<Box<dyn HandlerMiddleware>>>,
}

// add_handler signature extends to accept middleware list
#[pyo3(signature = (topic, callback, mode=None, batch_max_size=None, batch_max_wait_ms=None, timeout_ms=None, concurrency=None, middleware=None))]
pub fn add_handler(
    &mut self,
    topic: String,
    callback: Bound<'_, PyAny>,
    mode: Option<String>,
    batch_max_size: Option<usize>,
    batch_max_wait_ms: Option<u64>,
    timeout_ms: Option<u64>,
    concurrency: Option<usize>,
    middleware: Option<Vec<Py<PyAny>>>, // Python middleware instances
) {
    // Convert Py<PyAny> list to Vec<Box<dyn HandlerMiddleware>>
    let chain = middleware.map(|instances| {
        instances
            .into_iter()
            .map(|inst| Box::new(PythonMiddleware::new(inst)) as Box<dyn HandlerMiddleware>)
            .collect()
    });
    // ... rest of existing logic
}
```

### Python @handler decorator with middleware parameter
```python
# Source: MIDW-04 design — extending kafpy/runtime.py KafPy class
# Based on existing @app.handler decorator pattern in kafpy/__init__.py

def handler(
    self,
    topic: str,
    *,
    middleware: list[object] | None = None,
) -> Callable:
    """Decorator to register a handler for a topic.

    Args:
        topic: The Kafka topic to handle.
        middleware: List of middleware instances (e.g., [Logging(), Metrics()]).
                   Each must implement before(), after(), on_error().
    """
    def decorator(func: Callable) -> Callable:
        self._consumer.add_handler(
            topic,
            func,
            middleware=middleware,  # passed to Rust via PyConsumer.add_handler
        )
        return func
    return decorator
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Inline metrics/logging in worker.rs | Middleware chain wrapping invoke() | Phase 9 | Separates observability from execution |
| Per-handler config on Consumer | Per-handler middleware list on each handler | Phase 9 | More flexible — different handlers can have different middleware |
| Fixed span creation in worker | LoggingMiddleware owns its spans | Phase 9 | Users can swap logging middleware |

**Deprecated/outdated:**
- None in this phase.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Middleware chain is stored as `Option<Vec<Box<dyn HandlerMiddleware>>` on `PythonHandler` | Standard Stack | If PyO3 can't serialize trait objects, need alternative approach (e.g., enum dispatch) |
| A2 | Python middleware instances passed as `Vec<Py<PyAny>>` can be wrapped in a PyO3 adapter implementing `HandlerMiddleware` | Python API | If GIL lifetime issues prevent wrapping PyAny in Box<dyn HandlerMiddleware>, need to convert at add_handler time |
| A3 | Metrics middleware reuses existing `HandlerMetrics::record_latency` and `ThroughputMetrics::record_throughput` with existing metric names | Built-in Metrics | If new metric names are needed (e.g., `kafpy.middleware.latency`), need to register them first |

**If this table is empty:** All claims in this research were verified or cited — no user confirmation needed.

## Open Questions

1. **Python middleware lifetime across GIL boundary**
   - What we know: `Py<PyAny>` is `Send + Sync` when the GIL is not held, which is the case for the async worker loop. `PythonMiddleware::new(inst: Py<PyAny>)` can store the instance.
   - What's unclear: Whether calling Python middleware methods from Rust requires `Python::with_gil` or if we can use `spawn_blocking` for the GIL-acquire/release cycle per hook call.
   - Recommendation: Use `Python::with_gil` inside each hook call within a `spawn_blocking` task — same pattern as `PythonHandler::invoke`.

2. **Built-in middleware singleton vs. instance-per-handler**
   - What we know: Python API `@handler(middleware=[Logging(), Metrics()])` creates new instances per decorator call.
   - What's unclear: Whether built-in Rust `Logging` and `Metrics` should be `Arc` singletons (shared across handlers) or new instances per handler.
   - Recommendation: `Metrics` holds `SharedPrometheusSink` (already `Arc`), so instance per handler is cheap. `Logging` is stateless — can be ` &'static Logging` singleton.

3. **Middleware execution on error path**
   - What we know: `on_error` is called when result is not `Ok`. `after` is called only on success.
   - What's unclear: Should `on_error` be called before or after `executor.execute()`?
   - Recommendation: `on_error` should be called after the result is known but before `executor.execute()`, matching the `after` semantics.

## Environment Availability

Step 2.6: SKIPPED (no external dependencies — phase is purely Rust code + Python API changes with existing infrastructure)

## Validation Architecture

*Validation architecture section skipped — `workflow.nyquist_validation` key not found in `.planning/config.json` and phase has no automated test requirements in REQUIREMENTS.md.*

## Security Domain

Applicable ASVS categories for this phase:

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | No | — |
| V3 Session Management | No | — |
| V4 Access Control | No | — |
| V5 Input Validation | Yes | `ExecutionContext` fields (topic, partition, offset) are already validated at deserialization boundary |
| V6 Cryptography | No | — |

**No security-sensitive changes in this phase.** Middleware operate on in-memory `ExecutionContext` and `ExecutionResult` — no external input parsing, no secrets, no network calls.

## Sources

### Primary (HIGH confidence)
- `src/python/handler.rs` — existing PythonHandler.invoke_mode_with_timeout pattern
- `src/worker_pool/worker.rs` — existing worker loop span + metrics recording (lines 129-186)
- `src/observability/metrics.rs` — existing `HandlerMetrics::record_latency`, `ThroughputMetrics::record_throughput`, `LATENCY_BUCKETS`, `MetricLabels`
- `src/observability/tracing.rs` — existing `KafpySpanExt::kafpy_handler_invoke`
- `src/python/context.rs` — `ExecutionContext` structure
- `src/python/execution_result.rs` — `ExecutionResult` enum and `TimeoutInfo`
- `src/pyconsumer.rs` — `PyConsumer::add_handler` with `HandlerMetadata`

### Secondary (MEDIUM confidence)
- Tower `Layer` trait documentation — middleware composition pattern reference (docs.rs/tower/latest/tower/layer/trait.Layer.html)

### Tertiary (LOW confidence)
- None.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all required infrastructure already present in codebase
- Architecture: HIGH — pattern derived directly from existing worker loop + span instrumentation
- Pitfalls: MEDIUM — middleware ordering and GIL boundary concerns are inferred from existing patterns, not verified with a prototype

**Research date:** 2026-04-29
**Valid until:** 2026-05-29 (30 days — stable phase, no fast-moving dependencies)
