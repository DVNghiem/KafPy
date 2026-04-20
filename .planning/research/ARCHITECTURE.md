# Architecture Research: KafPy Module Redesign

**Domain:** PyO3-based Kafka client framework with Rust core / Python API
**Researched:** 2026-04-20
**Confidence:** MEDIUM

*Sources are primarily from Rust standard patterns (The Rust Book Ch.7, Rust API Guidelines, Rust Patterns),
the existing codebase analysis, and PyO3 documentation. Web search was unavailable during research,
so confidence is reduced for ecosystem-wide claims and elevated for codebase-specific observations.*

---

## Executive Summary

KafPy's module structure grew organically across 8 milestones. The current layout is functional but shows cohesion and coupling problems: a 344-line god object at the PyO3 boundary (`pyconsumer.rs`), data gravity problems where `BatchAccumulator` lives in `worker_pool/` instead of `python/`, a `coordinator/` module conflating four distinct responsibilities, and type alias leakage (`HandlerId = String`). The v2.0 refactor should extract a runtime factory from the PyO3 boundary, move domain-specific types to their correct modules, split `coordinator/` by responsibility, and introduce explicit state enums where `bool` flags currently model lifecycle state.

---

## Current Architecture

### System Overview

```
Python API (kafpy/__init__.py)
    │
    ▼
PyO3 Bridge (pyconsumer.rs) ─── Config Types (config.rs)
    │                              │
    │                              ▼
    │                         KafkaMessage (kafka_message.rs)
    │                              │
    ▼                              ▼
┌─────────────────────────────────────────────────────────────┐
│                     Pure-Rust Core                          │
├──────────────┬──────────────┬───────────────┬─────────────┤
│  consumer/   │  dispatcher/ │   python/     │  routing/   │
│  - runner    │  - Dispatcher│  - Handler    │  - chain    │
│  - message   │  - QueueMgr  │  - Executor   │  - routers  │
│  - config    │  - Backpressure│ - context   │             │
├──────────────┴──────────────┴───────────────┴─────────────┤
│  coordinator/ (pub(crate))                                  │
│  - OffsetTracker  - OffsetCommitter  - RetryCoordinator     │
│  - ShutdownCoordinator                                     │
├────────────────────────────────────────────────────────────┤
│  worker_pool/  │  failure/  │  dlq/  │  observability/      │
│                │            │        │  - metrics          │
│                │            │        │  - tracing           │
└────────────────────────────────────────────────────────────┘
```

### Current Module Inventory

| Module | Files | Primary Responsibility | Observed Problem |
|--------|-------|----------------------|-----------------|
| `pyconsumer.rs` | 1 (344 lines) | Assembles entire runtime at PyO3 boundary | God object |
| `consumer/` | 4 | Kafka message ingestion, rdkafka stream | Well-scoped |
| `dispatcher/` | 4 | Per-handler bounded channel dispatch | Well-scoped |
| `python/` | 6 | Python handler invocation, execution results | Missing `BatchAccumulator` (in `worker_pool/`) |
| `routing/` | 10 | Handler routing by pattern/header/key | `HandlerId = String` type alias leaked |
| `coordinator/` | 6 | Offset tracking, commit task, retry, shutdown | Conflates 4 distinct responsibilities |
| `worker_pool/` | 1 (650+ lines) | N worker loop, batch accumulation | `PartitionAccumulator` belongs in `batch/` |
| `failure/` | 4 | Failure taxonomy and classification | `tests.rs` misnamed (not `#[cfg(test)]`) |
| `dlq/` | 3 | Dead-letter queue routing | Well-scoped |
| `observability/` | 6 | Metrics, tracing, runtime introspection | `RuntimeSnapshot`/`WorkerPoolState` belong with runtime |
| `retry/` | 2 | Retry policy and schedule | Well-scoped, but `RetryCoordinator` is in `coordinator/` |
| `errors.rs` | 1 | Only `PyError` | Real errors scattered across `*/error.rs` |

---

## Recommended Project Structure

```
src/
├── lib.rs                    # Re-exports public API, PyO3 module init
│
├── config.rs                # ConsumerConfig, ProducerConfig (PyO3-facing)
├── kafka_message.rs          # KafkaMessage (PyO3 wrapper)
├── produce.rs                # PyProducer (PyO3)
│
├── consumer/                 # Pure-Rust Kafka consumer core
│   ├── mod.rs
│   ├── config.rs            # ConsumerConfigBuilder, AutoOffsetReset
│   ├── error.rs             # ConsumerError
│   ├── message.rs           # OwnedMessage, MessageTimestamp, MessageRef
│   └── runner.rs            # ConsumerRunner, ConsumerStream, ConsumerTask
│
├── dispatcher/               # Message routing to per-handler channels
│   ├── mod.rs               # Dispatcher, ConsumerDispatcher, DispatchOutcome
│   ├── error.rs             # DispatchError
│   ├── queue_manager.rs     # QueueManager
│   └── backpressure.rs      # BackpressurePolicy, BackpressureAction
│
├── python/                   # Python execution lane
│   ├── mod.rs               # Re-exports
│   ├── handler.rs           # PythonHandler, HandlerMode
│   ├── executor.rs          # Executor trait, ExecutorOutcome, DefaultExecutor
│   ├── context.rs           # ExecutionContext
│   ├── execution_result.rs  # ExecutionResult, BatchExecutionResult
│   └── batch.rs             # BatchAccumulator  ← MOVED from worker_pool/
│
├── routing/                  # Zero-copy routing infrastructure
│   ├── mod.rs               # Re-exports
│   ├── context.rs           # RoutingContext (zero-copy), HandlerId (newtype)
│   ├── decision.rs          # RoutingDecision, RejectReason
│   ├── chain.rs             # RoutingChain
│   ├── router.rs            # Router trait
│   ├── topic_pattern.rs     # TopicPatternRouter
│   ├── header.rs            # HeaderRouter
│   ├── key.rs               # KeyRouter
│   ├── python_router.rs     # PythonRouter
│   └── config.rs            # RoutingConfig
│
├── batch/                    # Batch accumulation (general, not Python-specific)
│   └── accumulator.rs      # PartitionAccumulator  ← MOVED from worker_pool/
│
├── runtime/                  # Runtime assembly at PyO3 boundary  ← NEW
│   ├── mod.rs               # RuntimeBuilder, build_consumer_runtime()
│   └── snapshot_task.rs     # RuntimeSnapshotTask (currently in observability/)
│
├── worker_pool/              # Tokio worker pool
│   ├── mod.rs               # WorkerPool, worker loop
│   └── state.rs             # WorkerPoolState, WorkerState  ← MOVED from observability/
│
├── offset/                   # Offset management subsystem  ← SPLIT from coordinator/
│   ├── mod.rs               # Re-exports
│   ├── tracker.rs           # OffsetTracker, PartitionState
│   ├── committer.rs         # OffsetCommitter, CommitConfig, TopicPartition
│   └── coordinator.rs       # OffsetCoordinator trait
│
├── retry/                    # Retry policy subsystem
│   ├── mod.rs               # Re-exports
│   ├── policy.rs            # RetryPolicy, RetrySchedule
│   └── coordinator.rs       # RetryCoordinator  ← MOVED from coordinator/
│
├── shutdown/                 # Shutdown coordination  ← MOVED from coordinator/
│   ├── mod.rs               # ShutdownCoordinator, ShutdownPhase
│   └── error.rs             # ShutdownError
│
├── dlq/                      # Dead-letter queue routing
│   ├── mod.rs               # Re-exports
│   ├── metadata.rs          # DlqMetadata
│   ├── router.rs            # DlqRouter trait, DefaultDlqRouter
│   └── produce.rs           # SharedDlqProducer
│
├── failure/                  # Failure classification
│   ├── mod.rs               # Re-exports
│   ├── reason.rs            # FailureReason, FailureCategory, TerminalKind
│   ├── classifier.rs        # FailureClassifier trait
│   └── logging.rs           # Failure logging
│
├── observability/            # Metrics, tracing, runtime introspection
│   ├── mod.rs               # ObservabilityConfig, MetricsSink
│   ├── metrics.rs           # HandlerMetrics, MetricLabels, MetricsSink trait
│   ├── prometheus.rs        # Prometheus adapter
│   ├── tracing.rs           # KafpySpanExt, LogTracer
│   └── kafka_metrics.rs     # KafkaMetrics, consumer lag
│
├── error.rs                  # Crate-level error re-exports  ← REPLACES errors.rs
│
├── logging.rs                # Logger initialization
│
└── benchmark/                # Benchmark infrastructure
    └── ...                   # (unchanged from v1.9)
```

### Structure Rationale

- **`batch/`**: `PartitionAccumulator` is a general accumulation primitive with no dependency on Python or worker pool. It belongs in its own module. `BatchAccumulator` goes in `python/` because it accumulates messages for Python handler modes.

- **`runtime/`**: The composition logic currently in `pyconsumer.rs` is the most critical extraction. A `RuntimeBuilder` lets pure Rust code be tested independently of PyO3.

- **`offset/` + `retry/` + `shutdown/`**: Split from `coordinator/` by responsibility boundary. Offset management and retry coordination are orthogonal concerns that happen to share a `pub(crate)` visibility. Separating them into distinct modules makes each easier to reason about independently.

- **`HandlerId` newtype**: Prevents accidental interchange with `String` or topic names. Type safety at the routing layer prevents subtle bugs.

- **`WorkerPoolState` moved to `worker_pool/`**: State that describes the worker pool belongs with the worker pool, not in `observability/`. The observability module should consume this state, not define it.

---

## Architectural Patterns

### Pattern 1: Thin PyO3 Boundary via Runtime Factory

**What:** Extract all composition logic from `pyconsumer.rs` into a `RuntimeBuilder` in `runtime/`. The PyO3 `#[pyclass] Consumer` struct holds only config and callback storage; all heavy lifting happens in Rust.

**When to use:** When a PyO3 bridge file exceeds ~100 lines or constructs more than 3 infrastructure components.

**Trade-offs:**
- Pro: Pure-Rust code becomes testable without Python
- Pro: Runtime composition changes do not break the PyO3 API
- Pro: Changes to internal wiring do not require recompiling PyO3 bindings
- Con: Adds an indirection layer (worth it for testability)

**Example:**
```rust
// runtime/mod.rs
pub struct RuntimeBuilder {
    config: ConsumerConfig,
    handlers: Arc<Mutex<HashMap<String, Arc<Py<PyAny>>>>>,
    shutdown_token: CancellationToken,
    // all infrastructure handles
}

impl RuntimeBuilder {
    pub fn build(self) -> Result<ConsumerRuntime, BuildError> {
        let rust_config = self.build_rust_config()?;
        let runner = ConsumerRunner::new(rust_config.clone(), None)?;
        let runner_arc = Arc::new(runner);
        let offset_tracker = Arc::new(OffsetTracker::new());
        offset_tracker.set_runner(Arc::clone(&runner_arc));
        let dispatcher = ConsumerDispatcher::new((*runner_arc).clone());
        // ... wire all components
        Ok(ConsumerRuntime { runner_arc, dispatcher, pool, committer })
    }
}

pub struct ConsumerRuntime {
    runner_arc: Arc<ConsumerRunner>,
    dispatcher: ConsumerDispatcher,
    pool: WorkerPool,
    committer_handle: JoinHandle<()>,
}

// pyconsumer.rs — thin PyO3 wrapper (target: ~60 lines)
#[pyclass]
pub struct Consumer {
    config: ConsumerConfig,
    handlers: Arc<Mutex<HashMap<String, Arc<Py<PyAny>>>>>,
    shutdown_token: CancellationToken,
}

#[pymethods]
impl Consumer {
    pub fn start(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let handlers = Arc::clone(&self.handlers);
        let shutdown_token = self.shutdown_token.clone();
        let config = self.config.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let runtime = RuntimeBuilder::new(config, handlers, shutdown_token)
                .build()
                .map_err(|e| PyErr::new::<PyRuntimeError, _>(e.to_string()))?;
            runtime.run().await;
            Ok(())
        })
    }

    pub fn stop(&self) {
        self.shutdown_token.cancel();
    }
}
```

### Pattern 2: State Machine as Enum (Make Illegal States Unrepresentable)

**What:** Replace `bool`/`Option` flags with explicit state enums. `PartitionState` does this well; `ShutdownCoordinator` and `RetryCoordinator` should follow.

**When to use:** For any component with lifecycle states that have invariants.

**Current problem observed:**
```rust
// coordinator/shutdown.rs — current
pub struct ShutdownCoordinator {
    phase: parking_lot::Mutex<ShutdownPhase>,
    drain_timeout: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShutdownPhase {
    is_drain Initiated: bool,
    is_committed: bool,
}
```

**Recommended — explicit enum:**
```rust
// shutdown/mod.rs — refactored
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownPhase {
    Running,
    DrainInitiated,
    Committed,
    Exited,
}

pub struct ShutdownCoordinator {
    phase: parking_lot::Mutex<ShutdownPhase>,
    drain_timeout: Duration,
}

impl ShutdownCoordinator {
    pub fn phase(&self) -> ShutdownPhase {
        *self.phase.lock()
    }

    pub fn initiate_drain(&self) -> Result<(), ShutdownError> {
        let mut phase = self.phase.lock();
        match *phase {
            ShutdownPhase::Running => {
                *phase = ShutdownPhase::DrainInitiated;
                Ok(())
            }
            _ => Err(ShutdownError::InvalidTransition),
        }
    }
}
```

**Same pattern for `RetryCoordinator`:**
```rust
// retry/coordinator.rs — refactored
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryState {
    NotStarted,
    InProgress { attempt: u32, next_retry_at: Instant },
    Terminal { reason: FailureReason },
    Committed,
}
```

### Pattern 3: Domain-Module Boundary via `pub(crate)` and Sealed Traits

**What:** Use `pub(crate)` for module-internal types that must cross module boundaries but not Python boundaries. Use sealed traits for extension points.

**When to use:** For traits like `Router`, `BackpressurePolicy`, `Executor` that have internal implementations but external consumers within the crate.

**Example:**
```rust
// routing/router.rs
mod private {
    pub trait Sealed {}
}

pub trait Router: private::Sealed + Send + Sync {
    fn route(&self, ctx: &RoutingContext<'_>) -> RoutingDecision;
}

pub struct TopicPatternRouter { /* ... */ }
mod private {
    impl Sealed for TopicPatternRouter {}
}
impl Router for TopicPatternRouter { /* ... */ }
```

### Pattern 4: Newtype for Primitive Type Aliases

**What:** Wrap `String` type aliases like `HandlerId` in a dedicated struct to prevent mixing values with different semantic meanings.

**When to use:** When the same primitive type is used for multiple distinct concepts.

**Current problem observed:**
```rust
// routing/context.rs — current
pub type HandlerId = String;
// Used as: fn route_with_chain(..., handler_id: String)
// String is used for both topics and handler IDs throughout the codebase
```

**Recommended — newtype:**
```rust
// routing/context.rs — refactored
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HandlerId(String);

impl HandlerId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for HandlerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for HandlerId {
    fn from(s: String) -> Self { Self(s) }
}

impl From<&str> for HandlerId {
    fn from(s: &str) -> Self { Self(s.to_string()) }
}
```

### Pattern 5: Unified Error Domain

**What:** Each domain module defines its error enum with `#[derive(Error)]` from `thiserror`. A top-level `error.rs` re-exports them all for discoverability and Python-facing use.

**When to use:** Always. Errors should be domain-typed.

**Example:**
```rust
// error.rs — crate-level
//! KafPy error types.
//!
//! ## Error hierarchy
//!
//! - [`PyError`] — errors that cross the PyO3 boundary (Python-callable)
//! - [`ConsumerError`] — consumer and message errors
//! - [`DispatchError`] — dispatcher and queue errors
//! - [`CoordinatorError`] — offset and retry coordinator errors

pub use consumer::ConsumerError;
pub use dispatcher::DispatchError;
pub use coordinator::CoordinatorError;
pub use retry::RetryError;
pub use dlq::DlqError;

#[derive(Error, Debug)]
pub enum PyError {
    #[error("consumer error: {0}")]
    Consumer(#[from] ConsumerError),
    #[error("producer error: {0}")]
    Producer(String),
    #[error("configuration error: {0}")]
    Config(String),
}
```

---

## PyO3 Boundary Preservation

### Boundary Principles

1. **Thin wrapper rule:** PyO3 files should only:
   - Convert Python types to Rust types at entry
   - Convert Rust types to Python types at exit
   - Delegate to pure-Rust factories/builders for all composition

2. **`pub(crate)` for internal APIs:** Any type used only within the Rust crate should be `pub(crate)`, not `pub`. This prevents Python users from depending on internal details and breaking refactoring.

3. **`Arc<dyn Trait>` for dynamic dispatch at boundary:** Use `Arc<dyn Executor>`, `Arc<dyn Router>` etc. at the PyO3 boundary. Concrete types are built inside the Rust crate.

4. **`Py<PyAny>` for Python callbacks:** Store as `Arc<Py<PyAny>>` — GIL-independent, sendable across threads, cloneable for worker pool.

5. **`spawn_blocking` at boundary crossings:** All Python callable invocations go through `spawn_blocking` to minimize GIL hold time. Never hold the GIL across Rust-side orchestration.

### What Stays Python-Facing (`pub`)

| Type | Why Public |
|------|-----------|
| `KafkaMessage` | Python users construct and receive these |
| `ConsumerConfig`, `ProducerConfig` | Python users configure with these |
| `PyProducer` | Python users call produce() on this |
| `Consumer` | Python users call add_handler/start/stop |
| `get_runtime_snapshot`, `register_status_callback` | Python-callable via `#[pyfunction]` |

### What Becomes `pub(crate)` or Private

| Type | Why Internal |
|------|-------------|
| `ConsumerRunner` | Internal orchestration |
| `Dispatcher`, `QueueManager` | Internal routing |
| `OffsetTracker`, `OffsetCommitter` | Internal offset management |
| `RetryCoordinator` | Internal retry state machine |
| `RoutingChain`, all routers | Internal routing infrastructure |
| `ShutdownCoordinator` | Internal lifecycle management |

---

## Refactoring Sequence

### Phase Order Rationale

The refactoring should proceed from leaves toward the boundary, to avoid breaking changes mid-refactor:

1. **`batch/` extraction** — no dependencies on anything being moved
2. **`offset/` split** — only depends on `consumer/`
3. **`shutdown/` split** — only depends on `coordinator/` types being stable
4. **`retry/coordinator.rs` move** — depends on offset types
5. **`HandlerId` newtype** — low risk, ripple through routing + dispatcher + worker_pool
6. **`BatchAccumulator` move to `python/`** — depends on batch module existing
7. **`runtime/` extraction** — depends on all above being stable
8. **Error consolidation into `error.rs`** — last, since it touches everything

### Phase 1: `batch/` Module Extraction

Move `PartitionAccumulator` from `worker_pool/mod.rs` to new `src/batch/accumulator.rs`.

```
worker_pool/mod.rs (before)
    ├── PartitionAccumulator     ← MOVE
    ├── BatchAccumulator          ← MOVE to python/
    └── WorkerPool                ← STAY

src/batch/
    └── accumulator.rs            ← PartitionAccumulator
```

### Phase 2: `offset/` Module Split

Split `coordinator/` into `offset/` module.

```
coordinator/ (before)
    ├── offset_tracker.rs        → offset/tracker.rs
    ├── commit_task.rs           → offset/committer.rs
    ├── offset_coordinator.rs    → offset/coordinator.rs
    ├── retry_coordinator.rs     → retry/coordinator.rs (Phase 4)
    └── shutdown.rs              → shutdown/mod.rs (Phase 3)
```

### Phase 3: `shutdown/` Module Split

Move `ShutdownCoordinator` from `coordinator/` to new `src/shutdown/` module.

### Phase 4: `retry/` Module Completion

Move `RetryCoordinator` from `coordinator/` to `retry/coordinator.rs` (already next to `policy.rs`).

### Phase 5: `HandlerId` Newtype

Replace `pub type HandlerId = String` with a newtype. Ripple changes through:
- `routing/context.rs` — define `HandlerId` struct
- `routing/chain.rs` — update field type
- `routing/decision.rs` — update `RoutingDecision::Route`
- `dispatcher/mod.rs` — update `send_to_handler_by_id`
- `worker_pool/mod.rs` — any handler-id comparisons

### Phase 6: `BatchAccumulator` Move

Move `BatchAccumulator` from `worker_pool/mod.rs` to `python/batch.rs`.

### Phase 7: `runtime/` Extraction

Extract `RuntimeBuilder` from `pyconsumer.rs`. This is the most impactful change but also the safest — it is purely additive until the old `start()` implementation is removed.

### Phase 8: Error Consolidation

Create unified `error.rs` re-exporting all domain errors. Remove `errors.rs`.

---

## Anti-Patterns to Avoid

### Anti-Pattern 1: PyO3 File as God Object

**What people do:** Put all composition logic in `#[pymethods]` functions.
**Why it's wrong:** Untestable without Python, forces recompile on runtime changes, mixes boundary concerns with business logic.
**Do this instead:** Factory in `runtime/` module, thin wrapper in PyO3 file.

### Anti-Pattern 2: `bool` Flags for State

**What people do:** `has_terminal: bool`, `is_shutdown: bool`.
**Why it's wrong:** Can represent invalid combinations. No exhaustive matching.
**Do this instead:** Use an enum with explicit states. Match exhaustively.

### Anti-Pattern 3: Type Aliases for Distinct Primitives

**What people do:** `type HandlerId = String;` and `type Topic = String;` — both `String`.
**Why it's wrong:** Accidentally interchangeable. No compiler enforcement.
**Do this instead:** Newtype wrappers. `struct HandlerId(String)` is distinct from `struct Topic(String)`.

### Anti-Pattern 4: Scattered Error Types

**What people do:** `consumer/error.rs`, `dispatcher/error.rs`, `coordinator/error.rs`, and a top-level `errors.rs` that only has `PyError`.
**Why it's wrong:** No single place to understand the crate's error universe.
**Do this instead:** Define domain errors in submodules for encapsulation. Re-export Python-facing errors in a top-level `error.rs`.

### Anti-Pattern 5: Test Helpers in Misnamed Files

**What people do:** Creating a `failure/tests.rs` file that contains test helpers and benchmarks.
**Why it's wrong:** In Rust, `tests.rs` implies `#[cfg(test)]`. Helper types belong in `tests/` directory or inline `#[cfg(test)]` blocks.
**Do this instead:** Move helpers to `tests/` directory at crate root, or inline them as `#[cfg(test)] mod test_helpers`.

---

## Confidence Assessment

| Area | Level | Reason |
|------|-------|--------|
| Module redesign | MEDIUM | Based on existing codebase analysis and Rust patterns; no external web sources accessible for verification |
| PyO3 boundary | MEDIUM | PyO3 docs not accessible via WebFetch; patterns are well-established in training data |
| State machine patterns | HIGH | Directly observed in codebase (`PartitionState`, `ShutdownCoordinator`) and matches Rust idioms |
| Refactoring sequence | MEDIUM | Practical approach based on god object identification; actual implementation order may vary based on build errors encountered |

---

## Sources

- [The Rust Programming Language — Ch.7: Packages, Crates, and Modules](https://doc.rust-lang.org/stable/book/ch07-00-managing-growing-projects-with-packages-crates-and-modules.html) (verified via WebFetch)
- Rust API Guidelines — Structuring (training data)
- Rust Patterns — God object elimination, newtype pattern, state machine as enum (training data)
- Existing codebase analysis: `src/lib.rs`, `src/pyconsumer.rs`, `src/coordinator/mod.rs`, `src/dispatcher/mod.rs`, `src/routing/context.rs`, `src/worker_pool/mod.rs`, `src/consumer/mod.rs`
- User rules: `~/.claude/rules/rust/patterns.md`, `~/.claude/rules/rust/coding-style.md`

---

*Architecture research for: KafPy v2.0 Code Quality Refactor*
*Researched: 2026-04-20*
