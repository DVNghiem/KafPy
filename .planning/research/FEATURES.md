# Code Quality Refactoring Research: KafPy

**Domain:** Rust/PyO3 Kafka Framework Refactoring
**Researched:** 2026-04-20
**Confidence:** HIGH

## Executive Summary

KafPy is a well-architected Rust-first Kafka framework with idiomatic Python bindings via PyO3. After 7 milestones of feature development, the codebase shows classic growing-pains patterns: a 1300+ line worker_pool module with severe duplication, ConsumerDispatcher exceeding 500 lines with mixed responsibilities, and no explicit state modeling for what is effectively a state machine. The good news: module boundaries between consumer/dispatcher/worker_pool/coordinator are sound. The refactoring surface is intra-module, not inter-module.

---

## Code Smell Catalog (Rust/PyO3 Specific)

### Critical: Duplication in `worker_pool/mod.rs`

**Location:** Lines 279-469 (worker_loop) and throughout batch_worker_loop

**Duplication #1: Error vs Rejected arms**

The `ExecutionResult::Error` (lines 279-380) and `ExecutionResult::Rejected` (lines 382-469) branches contain **identical logic** for retry scheduling and DLQ routing. Only the initial tracing message differs:

```rust
// ERROR arm (lines 307-332) - copied verbatim to Rejected arm (lines 395-420)
let (should_retry, should_dlq, delay) = retry_coordinator.record_failure(
    &ctx.topic, ctx.partition, ctx.offset, reason,
);
offset_coordinator.mark_failed(&ctx.topic, ctx.partition, ctx.offset, reason);
if should_retry {
    if let Some(d) = delay {
        tokio::time::sleep(d).await;
        active_message = Some(msg);
        continue;
    }
}
if should_dlq {
    // Lines 337-373: DLQ routing code - IDENTICAL in Rejected arm
    let metadata = DlqMetadata::new(...);
    let dlq_span = tracing::Span::current().kafpy_dlq_route(...);
    let tp = dlq_span.in_scope(|| dlq_router.route(&metadata));
    dlq_producer.produce_async(...);
    queue_manager.ack(&msg.topic, 1);
}
```

**Fix:** Extract to `handle_failure_result(msg, ctx, result, ...)` async function that takes the result variant and routes accordingly.

**Duplication #2: Batch flush boilerplate**

Every flush site in `batch_worker_loop` repeats the same pattern (lines 572-607, 626-658, 668-702, 712-746, 757-790, 804-837):

```rust
let topic = batch[0].topic.clone();
let partition = batch[0].partition;
let ctx = ExecutionContext::new(topic.clone(), partition, batch[0].offset, worker_id);
let span = tracing::Span::current().kafpy_handler_invoke(...);
let result = span.in_scope(|| async { handler.invoke_mode_batch(&ctx, batch.clone()).await });
handle_batch_result_inline(result, batch, &topic, partition, &ctx, ...).await;
```

**Fix:** Extract to `flush_partition_batch(accumulator, partition, handler, ...)` helper.

**Duplication #3: Message-to-PyDict conversion**

The `invoke()`, `invoke_batch()`, and `invoke_async()` methods in `python/handler.rs` all repeat the same PyDict construction from `OwnedMessage` fields. The trace context injection is identical across all four methods.

**Fix:** Extract to `message_to_pydict(msg: &OwnedMessage, trace_context: &HashMap<String, String>, py: Python) -> Py<PyDict>`.

### Critical: God Objects

**`worker_pool/mod.rs` (1309 lines)**

This file contains 6+ distinct responsibilities:
1. `PartitionAccumulator` (lines 49-89) - timer + buffer per partition
2. `BatchAccumulator` (lines 96-189) - orchestrates multiple PartitionAccumulators
3. `worker_loop` async function (lines 201-513) - single-message processing
4. `batch_worker_loop` async function (lines 528-845) - batch processing
5. `handle_batch_result_inline` (lines 852-994) - batch result routing
6. `WorkerPool` struct (lines 1001-1139) - worker management

**Fix:** Split into `worker_pool/accumulator.rs`, `worker_pool/loop.rs`, `worker_pool/batch_loop.rs`, `worker_pool/pool.rs`. The `handle_batch_result_inline` function itself at 142 lines is a god function.

**`dispatcher/mod.rs` (650 lines)**

Contains `Dispatcher`, `ConsumerDispatcher`, a `fake()` test helper, and 100+ lines of tests. The `ConsumerDispatcher` struct handles:
- Consumer stream consumption
- Routing chain evaluation
- Backpressure signal handling
- Topic pause/resume
- Partition handle management

**Fix:** Extract `pause/resume` logic into a `PauseManager` component. Extract partition handle management into `PartitionHandles`.

**`lib.rs` (165 lines)**

The `run_scenario_py` function (lines 86-134) has 50+ lines of scenario deserialization with a match on scenario_name, each arm doing the same `serde_json::from_str` pattern.

### High: Implicit State Machines

**`worker_loop`'s `active_message: Option<OwnedMessage>`**

Lines 216-483 use `Option<OwnedMessage>` as a state machine without an explicit state type:
- `None` = idle, polling for messages
- `Some(msg)` = processing a message

**Rust-idiomatic fix:** Define an explicit state enum:

```rust
enum WorkerState {
    Idle,
    Processing {
        msg: OwnedMessage,
        retry_delay: Option<Duration>,
    },
    ShuttingDown,
}
```

**`batch_worker_loop`'s `backpressure_active: bool`**

Lines 550-663 manage backpressure state with a boolean flag. This should be a proper state enum with transition validation.

### High: Naming Issues

| Issue | Location | Problem |
|-------|----------|---------|
| `OwnedMessage` in `consumer/mod.rs` | Used globally | Topic-based names suggest origin, not identity |
| `HandlerId` in routing | `context.rs` | Often equals topic name, confusing |
| `fake()` test helper | `dispatcher/mod.rs` | Should be in test module |
| `PartitionAccumulator` | `worker_pool/mod.rs` | Inner accumulator type is confusingly named |
| `NoopSink` | `worker_pool/mod.rs` | Re-defined here, also exists elsewhere? |

### High: Missing Error Type Cohesion

**`error.rs` proliferation**

Error types are scattered across modules:
- `consumer/error.rs`
- `dispatcher/error.rs`
- `coordinator/error.rs`
- `errors.rs` (root level)

This is appropriate for crate boundaries, but `dispatcher/error.rs` defines `DispatchError` while `ConsumerDispatcher` in `dispatcher/mod.rs` uses `crate::consumer::error::ConsumerError` directly. A `CoordinatorError` should exist in `coordinator/error.rs`.

### Medium: PyO3-Specific Patterns

**Repeated `Python::attach` patterns**

Every PyO3 bridge method does `Python::attach(|py| { ... })`. The `python/handler.rs` has 4 separate methods each with their own GIL closure. Consider if there's a pattern for reducing boilerplate, but be cautious not to over-abstract the PyO3 API.

**PyDict construction is repeated 4 times**

`invoke()`, `invoke_batch()`, `invoke_async()`, `invoke_batch_async()` all build PyDict messages. The single-message conversion is duplicated across all four.

### Medium: Underscore-prefixed Parameters

Lines 170, 247, 323, 369-371 in `python/handler.rs` have `_worker_id`, `_topic`, `_partition` — these indicate intentionally unused values. Better to remove the bindings or use `()` pattern matching.

### Medium: Long Functions

| Function | Lines | Threshold (50) |
|----------|-------|---------------|
| `worker_loop` | 312 | 6x over |
| `batch_worker_loop` | 315 | 6x over |
| `handle_batch_result_inline` | 142 | 3x over |
| `ConsumerDispatcher::run` | 66 | 1.3x over |
| `send_with_policy_and_signal` | 57 | 1.1x over |

---

## Refactoring Strategies

### Strategy 1: Extract-Refactor-Swap (Safe for God Objects)

**For `worker_pool/mod.rs`:**

1. Create new files: `worker_pool/accumulator.rs`, `worker_pool/loop.rs`, `worker_pool/batch_loop.rs`
2. Move `PartitionAccumulator` and `BatchAccumulator` to `accumulator.rs`
3. Move `worker_loop` to `loop.rs` (keep signature identical)
4. Move `batch_worker_loop` and `handle_batch_result_inline` to `batch_loop.rs`
5. `mod.rs` becomes: `pub mod accumulator; pub mod loop; pub mod batch_loop; pub use accumulator::*; pub use loop::*; pub use batch_loop::*;`
6. Verify `cargo check` passes at each step

**For `dispatcher/mod.rs`:**

1. Extract `DispatchOutcome`, `DispatchError` to `dispatcher/outcome.rs` (already in `dispatcher/mod.rs`)
2. Extract pause/resume methods to `dispatcher/pause.rs`
3. Move tests to `dispatcher/tests.rs`

### Strategy 2: Template Method for Duplicated Logic

The Error/Rejected arms share identical retry/DLQ logic. Extract:

```rust
async fn handle_execution_failure(
    msg: OwnedMessage,
    ctx: &ExecutionContext,
    reason: &FailureReason,
    retry_coordinator: &Arc<RetryCoordinator>,
    offset_coordinator: &Arc<dyn OffsetCoordinator>,
    dlq_router: &Arc<dyn DlqRouter>,
    dlq_producer: &Arc<SharedDlqProducer>,
    queue_manager: &Arc<QueueManager>,
    worker_id: usize,
) -> Option<OwnedMessage> {
    // Returns Some(msg) if retry scheduled, None if processed
}
```

Then Error and Rejected arms both call:
```rust
active_message = handle_execution_failure(msg, &ctx, reason, ...).await;
```

### Strategy 3: State Machine Extraction

Replace `Option<OwnedMessage>` with explicit state:

```rust
struct Worker {
    state: WorkerState,
    // ... dependencies
}

enum WorkerState {
    Idle,
    Active { msg: OwnedMessage },
    Retrying { msg: OwnedMessage, delay: Duration },
    Shutdown,
}
```

### Strategy 4: Use Newtype Pattern for HandlerId

Currently `HandlerId = String` in `routing/context.rs`. Topic and handler_id are often the same value but conceptually distinct. Create:

```rust
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct HandlerId(String);

impl HandlerId {
    pub fn as_str(&self) -> &str { &self.0 }
}
```

This prevents accidentally passing a topic name where a handler_id is expected.

### Strategy 5: Extract Message-to-PyDict Conversion

In `python/handler.rs`, extract repeated PyDict construction:

```rust
fn build_message_dict(
    msg: &OwnedMessage,
    trace_context: &HashMap<String, String>,
    py: Python,
) -> &PyDict {
    let py_msg = PyDict::new(py);
    // ... fill fields
    py_msg
}
```

---

## Module Boundary Assessment

**Well-designed boundaries (preserve):**
- `consumer/` - pure Rust, no PyO3
- `dispatcher/` - owns channel routing, backpressure
- `coordinator/` - offset tracking, retry coordination, shutdown
- `python/` - Python bridge, handler invocation
- `worker_pool/` - worker management (needs internal split)
- `routing/` - routing chain, decisions
- `dlq/` - DLQ metadata, routing, production

**Boundary issues:**
1. `dispatcher/mod.rs` mixes `Dispatcher` (pure dispatcher) with `ConsumerDispatcher` (orchestration). These should be split.
2. `coordinator/` contains `retry_coordinator.rs` which depends on `failure/` module. This is appropriate but the dependency goes the "wrong" direction (coordinator depends on failure). Consider if `RetryCoordinator` should live in `retry/` module.

---

## PyO3-Specific Refactoring Considerations

### GIL Boundary Discipline

Current pattern is correct: `spawn_blocking` + `Python::with_gil` minimizes GIL hold time. Do not change this pattern.

### `Py<PyAny>` Storage

`PythonHandler::callback: Arc<Py<PyAny>>` is the correct choice for GIL-independent, Send+Sync storage. This pattern should be preserved.

### No `unsafe` Code

No `unsafe` blocks found. This is correct for a PyO3 bridge. Do not introduce `unsafe` for performance.

---

## Recommended Refactoring Order

### Phase 1: Extract Duplicated Logic (Week 1)
- Extract `message_to_pydict()` helper in `python/handler.rs`
- Extract `handle_execution_failure()` in `worker_pool/mod.rs`
- Extract `flush_partition_batch()` helper in `batch_worker_loop`

### Phase 2: Split God Module - `worker_pool/` (Week 2)
- Move `PartitionAccumulator`, `BatchAccumulator` to `worker_pool/accumulator.rs`
- Move `worker_loop` to `worker_pool/loop.rs`
- Move `batch_worker_loop` and `handle_batch_result_inline` to `worker_pool/batch_loop.rs`
- `worker_pool/mod.rs` becomes thin re-export layer

### Phase 3: Split God Module - `dispatcher/` (Week 2)
- Extract pause/resume to `dispatcher/pause.rs`
- Move ConsumerDispatcher to `dispatcher/consumer_dispatcher.rs`
- `dispatcher/mod.rs` becomes `Dispatcher` + re-exports

### Phase 4: State Machine Extraction (Week 3)
- Replace `Option<OwnedMessage>` with explicit `WorkerState` enum in `worker_pool/loop.rs`
- Add `backpressure_active` state to proper state enum in `batch_worker_loop`

### Phase 5: Naming and Type Safety (Week 3)
- Add `HandlerId` newtype in `routing/context.rs`
- Rename `fake()` test helper to `test_helpers::owned_message()`
- Audit and clean up underscore-prefixed unused parameters

---

## Anti-Patterns to Avoid

| Anti-Pattern | Why Avoid | What to Do Instead |
|---|---|---|
| Over-abstracting PyO3 GIL handling | Obscures the critical GIL boundary | Keep `Python::with_gil` explicit |
| Sealing traits prematurely | Limits testability | Keep traits open until interface stabilizes |
| Extracting interfaces too early | YAGNI for abstraction | Wait until second consumer of a trait |
| Moving code just to move it | Rewrites without benefit | Only refactor code that has a smell |
| Making everything `pub(crate)` | Over-exposes | Default to private, open as needed |

---

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Duplication findings | HIGH | Verified by reading source, exact line ranges identified |
| God object assessment | HIGH | File sizes measured, responsibilities enumerated |
| Refactoring strategies | HIGH | Standard patterns applied to specific findings |
| Module boundary | MEDIUM | Architectural assessment, could benefit from integration test verification |
| PyO3 patterns | MEDIUM | Based on PyO3 best practices, not formal audit |

## Open Questions

1. **PartitionAccumulator naming:** Should it be renamed to something clearer? `PerPartitionBuffer`?
2. **HandlerId vs topic:** Is there a real distinction or are they always equal? If always equal, `HandlerId` newtype may be unnecessary complexity.
3. **coordinator/retry_coordinator.rs location:** Should `RetryCoordinator` live in `retry/` module since it uses `RetryPolicy` from there?
4. **NoopSink duplication:** Is `NoopSink` in `worker_pool/mod.rs` identical to any sink in observability module?

## Sources

- KafPy source code analysis (src/consumer/mod.rs, src/dispatcher/mod.rs, src/worker_pool/mod.rs, src/python/handler.rs, src/failure/logging.rs, src/routing/mod.rs, src/coordinator/mod.rs)
- Rust API guidelines (https://rust-lang.github.io/api-guidelines/)
- Refactoring Guru (https://refactoring.guru/refactoring)
- PyO3 user guide (https://pyo3.rs/)

---

*Research for: v2.0 Code Quality Refactor*
*Researched: 2026-04-20*
