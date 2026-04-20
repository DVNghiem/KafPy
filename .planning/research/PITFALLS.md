# Pitfalls Research: KafPy v2.0 Code Quality Refactor

**Domain:** PyO3/Rust Kafka Framework Refactoring
**Researched:** 2026-04-20
**Confidence:** MEDIUM-HIGH (based on Rust/PyO3 domain knowledge + KafPy codebase patterns; external search tools unavailable at time of research)

---

## Critical Pitfalls

Mistakes that silently change behavior during Rust/PyO3 refactoring — these cause bugs that appear as production incidents, not compile errors.

---

### Pitfall 1: Breaking `Send+Sync` Guarantees via Ownership Changes

**What goes wrong:**
After refactoring, code that previously compiled suddenly fails with `impl Trait` errors or "channel closed" panics because a type no longer implements `Send` or `Sync`. More dangerously, types that should be `Send+Sync` lose those bounds at runtime, causing panics in multi-threaded contexts.

**Why it happens:**
- Moving `Arc<Mutex<...>>` to `parking_lot::Mutex` (or vice versa) changes thread-safety semantics
- Adding interior mutability via `RefCell`, `Cell`, or `Mutex` inside a type used across threads
- Storing `Py<PyAny>` or `Python` GIL state in a context that crosses thread boundaries
- Using `Rc<T>` instead of `Arc<T>` in `ConsumerRunner`, `Dispatcher`, or `WorkerPool` paths
- Converting `impl Trait` to `dyn Trait` in return positions without verifying object safety

**How to avoid:**
- Always maintain `Send + Sync` bounds on all types used in `Arc<T>` across Tokio tasks
- Use the compiler-enforced trait: `fn assert_send_sync<T: Send + Sync>() {}` in tests (already present in `dispatcher/mod.rs` lines 542-543)
- Never use `Rc` in async/multi-threaded paths; use `Arc`
- When using `parking_lot::Mutex` (non-poisoning), ensure the wrapped type is truly thread-safe
- `Py<PyAny>` is `Send + Sync` when stored properly via `Arc<Py<PyAny>>` in `Consumer::handlers`

**Warning signs:**
- `impl Trait` return type changes cause cascading compilation errors across modules
- `channel closed` panics appearing where they did not exist before
- `Mutex` poison panics when previous code used `parking_lot::Mutex`
- Subtle deadlocks under load that did not occur previously
- `cargo clippy` warnings about `Arc` dereference patterns

**Phase to address:**
This is a **foundational concern** verified in Phase 1. Any module touching `ConsumerRunner`, `Dispatcher`, `WorkerPool`, or coordinator/offset tracking must have `Send+Sync` compile-time verification.

---

### Pitfall 2: Async Channel Semantic Changes (Capacity, Backpressure, Ordering)

**What goes wrong:**
Changing `mpsc::channel` capacity from 1000 to 100, or switching from `try_send` to `send`, or altering `Broadcast` channel behavior, fundamentally changes backpressure signaling and message loss characteristics. A refactor that changes channel capacity or error handling silently changes whether messages are dropped or blocking under load.

**Why it happens:**
- Capacity affects backpressure threshold ratios (`resume_threshold = 0.5`) in `ConsumerDispatcher`
- `try_send` returns `Backpressure` error immediately; `send` awaits — changes blocking semantics
- `broadcast` channel drops oldest messages when capacity exceeded; `mpsc` blocks or returns errors
- Order of `inflight.fetch_add` vs `queue_depth.fetch_add` relative to `try_send` affects metrics accuracy
- Refactoring that removes `biased` directive on `select!` causes shutdown signal to lose election to messages

**How to avoid:**
- Never change channel capacity without documenting the impact on `BackpressureAction::Drop` behavior
- When refactoring `send_with_policy` or `send_with_policy_and_signal`, preserve the **exact sequence**:
  1. Semaphore permit acquisition (`try_acquire_semaphore`)
  2. `inflight` counter increment
  3. `try_send` call
  4. On `Full`: decrement `inflight`, then apply policy
  5. On `Ok`: increment `queue_depth`
- The current semaphore-before-dispatch pattern (DISP-15) is critical for concurrency limiting
- Maintain `biased;` directive on `select!` in `ConsumerRunner::run()` to prioritize shutdown

**Warning signs:**
- Throughput test degrades after refactor (messages being dropped instead of queued)
- Deadlock where dispatcher waits for queue space but workers are blocked
- Queue depth metrics showing unexpected spikes or negative values
- `BackpressureAction` enum variant changes causing missing match arms in `ConsumerDispatcher::run`
- `select!` without `biased` causes shutdown signal to not be received immediately

**Phase to address:**
Phase targeting `Dispatcher` and `QueueManager` must preserve exact dispatch semantics.

---

### Pitfall 3: PyO3 GIL Boundary Violations

**What goes wrong:**
Python callbacks stored as `Arc<Py<PyAny>>` must remain GIL-independent. Refactoring that moves `Python::with_gil` calls, changes `spawn_blocking` usage, or alters the `PythonAsyncFuture` bridge causes GIL deadlocks, use-after-free, or silent data corruption in Python callbacks.

**Why it happens:**
- `Py<PyAny>` stored outside `Python::with_gil` loses GIL lifetime association
- Calling Python code outside `spawn_blocking` holds the Tokio thread hostage
- `PythonAsyncFuture` relies on specific GIL acquire/release patterns that differ from `spawn_blocking`
- Storing `Bound<'_, PyAny>` instead of `Py<PyAny>` creates lifetime issues across `await` points
- Moving `Python::attach` outside the `move ||` closure captures the GIL state incorrectly

**How to avoid:**
- Always use `Arc<Py<PyAny>>` for callback storage (verified in `Consumer::handlers: Arc<Mutex<HashMap<String, Arc<Py<PyAny>>>>>`)
- All Python invocations must go through `spawn_blocking` for sync handlers or `PythonAsyncFuture` for async handlers
- Never call `Python::with_gil` directly in an async context without the custom CFFI bridge
- When refactoring `PythonHandler::invoke`, preserve the **exact pattern**: capture data before `spawn_blocking`, build `PyDict` inside `Python::attach`, extract `Arc<Py<PyAny>>` inside `move ||`
- `inject_trace_context` must be called **before** crossing the GIL boundary, not after

**Warning signs:**
- Python callback receives `None` or garbage values for message fields
- Deadlock when calling `stop()` during active Python handler execution
- `PyErr` types leaking across `await` points (they are not `Send`)
- `future_into_py` failing with "GIL not held" errors
- `callback.call1` panicking with "dropped GIL" errors

**Phase to address:**
Any refactoring of `python/handler.rs`, `python/async_bridge.rs`, or `pyconsumer.rs` must preserve GIL boundary patterns exactly.

---

### Pitfall 4: Shutdown Ordering Violations Causing Circular Waits

**What goes wrong:**
`ShutdownCoordinator` implements phased shutdown (Running -> Draining -> Finalizing -> Done). Refactoring that changes the order of `begin_draining()` signals or `CancellationToken::cancel` calls causes circular waits: dispatcher waits for workers to drain, workers wait for dispatcher to stop producing, coordinator waits indefinitely.

**Why it happens:**
- `ConsumerRunner::stop()` triggers `begin_draining()` which returns dispatcher/worker/committer cancel tokens
- Current code (LSC-02/03): **dispatcher cancel sent FIRST**, then `shutdown_tx` broadcast
- Refactoring that sends worker cancel before dispatcher cancel causes workers to wait for messages that stop coming
- Changing `select!` bias order or removing `biased` directive causes shutdown signal to lose election
- `spawn_blocking` tasks holding GIL during shutdown prevent coordinator from advancing phases

**How to avoid:**
- Maintain the current shutdown signal order: dispatcher cancellation -> wait briefly -> worker cancellation
- The `biased` directive on `select!` in `ConsumerRunner::run()` ensures shutdown takes priority
- `shutdown_token.cancel()` must be propagated to `WorkerPool` before `pool.run().await` completes
- When refactoring `ShutdownCoordinator`, preserve the three-phase token distribution
- `spawn_blocking` tasks must complete or be aborted before coordinator enters Finalizing phase

**Warning signs:**
- `Consumer::stop()` hangs for more than `drain_timeout_secs`
- WorkerPool shutdown drain not completing (pending messages not flushed to DLQ)
- `panic: closure called recursively` in `spawn_blocking` during shutdown
- Offset commits not flushing before process exit
- DLQ messages not flushed before process exit

**Phase to address:**
Phase targeting `coordinator/shutdown.rs`, `worker_pool/mod.rs`, or `ConsumerRunner::stop()` must preserve exact shutdown sequence.

---

### Pitfall 5: Offset Commit State Machine Breaking Highest-Contiguous-Offset Logic

**What goes wrong:**
`OffsetTracker` and `OffsetCommitter` implement highest-contiguous-offset commit: an offset is only committed when all prior offsets have been acknowledged. Refactoring that changes `should_commit` logic, `has_terminal` gating, or `highest_contiguous_offset` calculation silently breaks at-least-once delivery guarantees.

**Why it happens:**
- `OffsetTracker::should_commit(topic, partition)` checks `has_terminal` flag per-partition
- Once `has_terminal=true` on a partition, that partition stops committing until restart
- `record_ack` must be called before `record_success` in `RetryCoordinator` on final success
- Out-of-order ack buffering depends on `pending_acks` HashMap ordering
- `store_offset` must precede `commit` — this is the two-phase offset management contract

**How to avoid:**
- Never change `should_commit` to return `true` when `has_terminal=true` on a partition
- When modifying `OffsetCommitter::run`, preserve the watch channel pattern for commit signals
- `store_offset` must precede `commit` — this is the two-phase offset management contract
- `retry_coordinator.record_success()` must be called by **worker loop after all retries exhausted**, not by dispatcher
- `flush_failed_to_dlq` must be called during graceful shutdown before final commit

**Warning signs:**
- Duplicate message delivery after restart (offset committed too early)
- Message loss (offset committed skipping failed messages)
- `OffsetTracker::should_commit` returning different values under same partition state
- Missing `flush_failed_to_dlq` call during graceful shutdown
- `highest_contiguous_offset` calculation producing gaps

**Phase to address:**
Phase targeting `coordinator/offset_tracker.rs`, `coordinator/offset_coordinator.rs`, or `coordinator/retry_coordinator.rs` must preserve exact offset commit semantics.

---

### Pitfall 6: Enum State Machine Exhaustiveness Breaking

**What goes wrong:**
`RoutingDecision`, `BackpressureAction`, `FailureReason`, `ExecutionResult`, `HandlerMode`, and `BatchExecutionResult` are exhaustive enums used in `match` expressions throughout the codebase. Refactoring that adds a variant without updating all `match` arms silently introduces `panic!` at runtime in production.

**Why it happens:**
- Compiler catches non-exhaustive matches in the same crate
- PyO3 `#[pyfunction]` or `#[pymethods]` can expose variants that callers match on in Python
- Adding `RoutingDecision::Defer` variant (v1.5) required updating all dispatch paths
- Refactoring that removes or renames a variant without updating all match arms causes compile errors (which is good) but refactoring that **appears** to cover all arms via wildcard `_` is bad
- `#[non_exhaustive]` on enums being used — callers cannot match exhaustively

**How to avoid:**
- Never use wildcard `_` match arms for business-critical enums — use `#[deny(unreachable_patterns)]` at crate level
- When adding a new enum variant, search all match expressions that handle it: `RoutingDecision`, `BackpressureAction`, `ExecutionResult`, `BatchExecutionResult`, `FailureReason`
- Verify all four `RoutingDecision` arms are handled in `ConsumerDispatcher::route_with_chain`: `Route`, `Drop`, `Reject`, `Defer`
- Verify all `ExecutionResult` variants are handled when converting to `BatchExecutionResult`

**Warning signs:**
- Wildcard `_` arm in critical dispatch paths (should be `unreachable!()` with context)
- Python `match` statements on `RoutingDecision` string variants not updated
- New variant added to enum without updating all match sites
- `#[non_exhaustive]` on enums being matched exhaustively by internal code

**Phase to address:**
Phase adding any new enum variants or modifying existing state machine enums must audit all match expressions.

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Using `unwrap()` in async code to avoid error handling | Faster to write | Panics corrupt Tokio task state, difficult to debug | Never in production async paths |
| `Arc<Mutex<T>>` instead of proper typed channels | Avoids thinking about ownership | Deadlocks, poisoning issues | Only when true shared mutation is needed |
| `.clone()` to avoid lifetime issues | Compiles faster | Memory leaks, performance degradation | Only for true data duplication, not as band-aid |
| `select!` without `biased` for shutdown | Simpler code | Race conditions in shutdown vs. message paths | Only when message ordering truly doesn't matter |
| `#[allow(unused)]` on public API fields | Avoids compiler warnings | API contract becomes unclear | Never on public API types |
| `unsafe` blocks to bypass borrow checker | Quick fix for complex ownership | Memory safety violations, undefined behavior | Never without extensive SAFETY comments and review |
| Ignoring `clippy` warnings | Code compiles without changes | Missed optimization opportunities, subtle bugs | Never — treat warnings as errors |

---

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|-----------------|
| rdkafka / StreamConsumer | Modifying `ConsumerConfig` after `subscribe()` call | Config is consumed at creation; clone before if needed |
| Tokio / rdkafka | Blocking in `consumer.recv()` loop without timeout | Use `select!` with timeout arm to detect stalls |
| PyO3 / Tokio | Holding GIL across `.await` points | All Python calls via `spawn_blocking` or `PythonAsyncFuture` |
| Broadcast / WorkerPool | Forgetting to `drop(tx)` after creating receiver | Drop sender to close channel and unblock workers |
| Tokio / `select!` | Shutdown signal losing election to message receipt | Add `biased;` directive and shutdown arm first |
| OffsetTracker / RetryCoordinator | Calling `record_success` before all retries exhausted | Only call after final success, not intermediate |

---

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| `Mutex<HashMap>` on hot path | Lock contention causing throughput collapse | Use `parking_lot::Mutex` or `DashMap` for concurrent HashMaps | At >1000 messages/sec with many topics |
| `spawn_blocking` per message | GIL thrashing, latency spikes | Batch messages before crossing GIL | Batch handlers already mitigate; sync single handlers at risk |
| Unbounded channel to worker | Memory growth if workers slower than producers | Use bounded channels with backpressure | 10x normal load or slow Python handlers |
| Frequent `Arc::clone` in message path | Reference count churn | Pass `Arc<OwnedMessage>` through pipeline | Very high message rates (>50k/sec) |
| `select!` without bias on shutdown | Shutdown delayed up to one poll interval | Add `biased;` directive and shutdown arm first | Under heavy message throughput |
| Cloning `OwnedMessage` on every handler dispatch | Allocation pressure, copy overhead | Only clone fields needed (key, payload, headers) | High throughput scenarios |

---

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| Logging `message.payload` at DEBUG level | Sensitive data in logs | Use `#[instrument(skip(payload))]` or guard with log level |
| Python callback exception details to Kafka | Information leakage via dead letter | Sanitize `traceback` in `ExecutionResult::Error` before DLQ |
| Storing `Py<PyAny>` callback without validation | Malformed Python objects causing segfaults | Python callbacks are user-provided; trust but verify types at call site |
| DLQ topic name from config without sanitization | Kafka topic injection | Validate `dlq_topic_prefix` contains only allowed characters |

---

## "Looks Done But Isn't" Checklist

These items appear complete but are frequently missed during refactoring:

- [ ] **Offset commit:** `store_offset` called before `commit` — not just at shutdown
- [ ] **RetryCoordinator:** `record_success` called only after **all retries exhausted**, not on intermediate success
- [ ] **has_terminal flag:** Set once per-partition, never cleared, blocks commits until restart
- [ ] **Shutdown drain:** `flush_failed_to_dlq` called before `graceful_shutdown` in `WorkerPool`
- [ ] **GIL release:** `spawn_blocking` used for ALL sync Python calls, not `block_on`
- [ ] **RoutingDecision:** All 4 variants (`Route`, `Drop`, `Reject`, `Defer`) have explicit handling paths
- [ ] **BackpressureAction:** All 3 variants (`Drop`, `Wait`, `FuturePausePartition`) handled in dispatch
- [ ] **Semaphore acquire:** Happens BEFORE `try_send`, not after
- [ ] **inflight decrement:** Happens when `try_send` fails (Full/Closed), not only on success
- [ ] **ShutdownCoordinator:** Three-phase token distribution preserved (dispatcher, worker, committer)
- [ ] **select! biased:** Shutdown arm appears first with `biased;` directive in `ConsumerRunner::run()`
- [ ] **Py<PyAny> storage:** Callbacks stored as `Arc<Py<PyAny>>` not `Bound<PyAny>`
- [ ] **Dispatch order:** `queue_depth` incremented only on `try_send` success, not on enqueue

---

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Breaking Send+Sync | HIGH | Revert ownership changes; add `assert_send_sync` compile-time checks |
| Channel semantic changes | HIGH | Run throughput/latency benchmarks; revert capacity changes if degraded |
| GIL boundary violations | CRITICAL | Revert to `spawn_blocking` pattern; verify `PythonAsyncFuture` CFFI bridge |
| Circular shutdown waits | CRITICAL | Add logging to each phase; verify `biased` on `select!`; check token cancel order |
| Offset commit state machine | CRITICAL | Run consumer group restart test; verify no duplicate/lost messages |
| Enum exhaustiveness | LOW | Enable `#[deny(unreachable_patterns)]` in lib.rs; add missing match arms |

---

## Pitfall-to-Phase Mapping

| Pitfall | Primary Concern Phase | Verification Strategy |
|---------|----------------------|----------------------|
| Send+Sync guarantees | Any module touching `Arc`, `Mutex`, `ConsumerRunner` | `cargo clippy` passes; `assert_send_sync` tests green; miri clean |
| Channel semantics | `dispatcher/mod.rs`, `queue_manager.rs` | Throughput benchmark unchanged; backpressure test passes |
| PyO3 GIL boundary | `python/handler.rs`, `pyconsumer.rs`, `async_bridge.rs` | Integration tests with sync/async Python handlers pass |
| Shutdown ordering | `coordinator/shutdown.rs`, `worker_pool/mod.rs` | Graceful shutdown completes within `drain_timeout_secs` |
| Offset commit state | `coordinator/offset_tracker.rs`, `retry_coordinator.rs` | Consumer restart test: no duplicates, no gaps |
| Enum exhaustiveness | Any phase adding enum variants | `#[deny(unreachable_patterns)]` in lib.rs; all match arms explicit |

---

## Sources

- **Rustonomicon** — unsafe code, aliasing, and variance concepts (https://doc.rust-lang.org/nomicon/) [MEDIUM]
- **The Rust Programming Language Book** — iterators, closures, async (https://doc.rust-lang.org/book/) [HIGH]
- **Tokio documentation** — channel semantics, select bias, cancellation (https://tokio.rs/tokio) [HIGH]
- **PyO3 User Guide** — GIL handling, pyclass, async (https://pyo3.rs) [HIGH]
- **KafPy source patterns** — observed in `src/dispatcher/mod.rs`, `src/pyconsumer.rs`, `src/coordinator/shutdown.rs`, `src/python/handler.rs`, `src/coordinator/retry_coordinator.rs` [HIGH]
- **Rust-lang/rust-clippy** — lints for common mistakes (https://github.com/rust-lang/rust-clippy) [MEDIUM]

---

*Pitfalls research for: KafPy v2.0 Code Quality Refactor*
*Researched: 2026-04-20*
