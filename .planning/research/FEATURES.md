# Feature Research: Python Execution Lane

**Domain:** Kafka consumer worker pool with Python callback invocation
**Researched:** 2026-04-16
**Confidence:** MEDIUM (based on codebase analysis + PyO3 patterns; no web search available)

## Feature Landscape

### Table Stakes (Users Expect These)

Features users assume exist. Missing these = product feels broken.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| **WorkerPool with N workers** | Parallel message processing, not single-threaded serialization | MEDIUM | Workers pull from `mpsc::Receiver<OwnedMessage>`; each worker is a Tokio task |
| **Py<PyAny> callback storage** | Callbacks must survive GIL release and be Send+Sync across threads | LOW | Owned `Py<PyAny>`, not `&PyAny` — the key design decision from v1.2 |
| **spawn_blocking GIL acquisition** | GIL only held during the actual Python call, not during async wait | MEDIUM | `tokio::task::spawn_blocking` — minimal GIL window critical for throughput |
| **ExecutionResult normalization** | Python callbacks can return anything; Rust needs structured outcomes | LOW | `Ok`, `Error`, `Rejected` variants — normalize all Python return values |
| **Executor trait for policies** | Retry, commit, async, batch — all are policy choices, not core logic | MEDIUM | Pluggable at WorkerPool construction; default is fire-and-forget |
| **Graceful worker startup/shutdown** | Pool starts workers on construction; shutdown via `drop` or explicit `stop()` | LOW | Log worker start/stop; drain in-flight messages before exit |

### Differentiators (Competitive Advantage)

Features that set the product apart. Not required, but valued.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **Backpressure-aware dispatch** | WorkerPool respects queue depth from Dispatcher; doesn't overwhelm Python layer | MEDIUM | WorkerPool receives `mpsc::Receiver` already configured with semaphore per DISP-15 |
| **Per-topic queue isolation** | Each topic's handler has its own channel — one slow consumer doesn't block others | LOW | Already built in v1.1 Dispatcher; WorkerPool consumes from pre-registered receivers |
| **Executor retry policies** | Built-in retry with backoff without user implementing their own loop | HIGH | v1.x feature — interface exists now, implementation deferred |
| **Batch execution mode** | Process multiple messages in one Python call to reduce per-message overhead | HIGH | v2 feature — `Executor::execute_batch` trait method |

### Anti-Features (Commonly Requested, Often Problematic)

Features that seem good but create problems.

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| **Blocking GIL hold across entire message loop** | "Just call Python directly" — simple to understand | Blocks Tokio runtime from doing useful work while Python runs; kills async throughput | `spawn_blocking` — GIL only during the call |
| **Unbounded worker count** | "More workers = more throughput" | Oversubscribes Python GIL; diminishing returns; can destabilize the interpreter | Configurable pool size with sensible default (e.g., `num_cpus::get()`) |
| **GIL acquisition via Python::with_gil** | "Easier than spawn_blocking" | GIL held while awaiting, blocking other async tasks; same problem as blocking GIL | `spawn_blocking` with `Python::attach` inside the blocking closure |
| **Sync callback in async context** | "My handler is synchronous, why use spawn?" | Synchronous handler holds the async task slot; limits concurrency | Both sync and async callbacks via spawn_blocking (async via `into_super_chain`) |
| **Auto-commit per message** | "Want each message committed immediately" | Kafka commit is expensive; hammering commit per message is slow | Executor controls commit batching; default is fire-and-forget with manual commit |

## Feature Dependencies

```
OwnedMessage (from v1.0 consumer)
    └──requires──> Dispatcher (from v1.1)
                       └──requires──> Handler Queue (mpsc::Receiver<OwnedMessage>)
                                              └──requires──> WorkerPool (v1.2, this module)

Py<PyAny> (v1.2 decision)
    └──requires──> spawn_blocking for GIL (v1.2 decision)
                       └──requires──> ExecutionResult normalization

Executor trait (v1.2)
    └──extends──> ExecutionResult
    └──future──> RetryExecutor, CommitExecutor, AsyncExecutor, BatchExecutor

HandlerMetadata::ack()
    └──called_by──> WorkerPool after Python execution completes
                       └──updates──> QueueManager counters (inflight, queue_depth)
```

### Dependency Notes

- **WorkerPool requires Dispatcher's handler queues:** WorkerPool consumes from `mpsc::Receiver<OwnedMessage>` returned by `Dispatcher::register_handler`. No new queue infrastructure needed.
- **Executor extends ExecutionResult:** `Executor::execute()` receives `OwnedMessage` and returns `Result<(), ExecutionError>`. The trait can be extended with batch/async variants without changing the core result type.
- **HandlerMetadata::ack() bridges Python→Rust:** After Python callback completes, WorkerPool calls `queue_manager.ack(topic, count)` to decrement inflight. This closes the loop started by `Dispatcher::send()` which incremented inflight.

## MVP Definition

### Launch With (v1.2)

Minimum viable product — what's needed to validate the concept.

- [x] **WorkerPool with configurable worker count** — Spawns N Tokio tasks pulling from `mpsc::Receiver<OwnedMessage>`
- [x] **Py<PyAny> callback storage** — GIL-independent, Send+Sync, stored in `PythonHandler`
- [x] **spawn_blocking GIL acquisition** — Minimal window; `Python::attach` inside blocking closure
- [x] **ExecutionResult normalization** — `Ok` (success), `Error` (exception), `Rejected` (handler returned falsey)
- [x] **DefaultExecutor (fire-and-forget)** — No retry, no commit; simply runs callback and logs outcome
- [x] **Worker lifecycle logging** — Worker start, stop, message pickup, handler success/failure

### Add After Validation (v1.x)

Features to add once core is working.

- [ ] **RetryExecutor** — `Executor::retry(OwnedMessage, Py<PyAny>, u32)` with configurable attempts
- [ ] **CommitExecutor** — Batches offsets and commits after N messages or T timeout
- [ ] **Shutdown drain** — Graceful stop with in-flight message completion before worker exit

### Future Consideration (v2+)

Features to defer until product-market fit is established.

- [ ] **AsyncExecutor** — Detects Python coroutines and runs them on Python's asyncio event loop via `pyo3-async-runtimes`
- [ ] **BatchExecutor** — Collects N messages and invokes a single Python callback with a list
- [ ] **DLQ (Dead Letter Queue)** — Rejected messages routed to error topic after max retries

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| WorkerPool with spawn_blocking | HIGH — core of v1.2 | MEDIUM | P1 |
| Py<PyAny> storage | HIGH — required for Send+Sync across threads | LOW | P1 |
| ExecutionResult normalization | HIGH — all outcomes must be structured | LOW | P1 |
| DefaultExecutor (fire-and-forget) | HIGH — MVP policy | LOW | P1 |
| Worker lifecycle logging | MEDIUM — operational visibility | LOW | P1 |
| RetryExecutor | MEDIUM — first policy extension | MEDIUM | P2 |
| CommitExecutor | MEDIUM — enables at-least-once | MEDIUM | P2 |
| Shutdown drain | MEDIUM — production readiness | MEDIUM | P2 |
| AsyncExecutor | LOW — most users use sync handlers | HIGH | P3 |
| BatchExecutor | LOW — reduces per-message overhead | HIGH | P3 |
| DLQ | LOW — advanced pattern | HIGH | P3 |

**Priority key:**
- P1: Must have for launch
- P2: Should have, add when possible
- P3: Nice to have, future consideration

## Competitor Feature Analysis

| Feature | kafka-python | confluent-kafka-python | faust | KafPy (our approach) |
|---------|--------------|------------------------|-------|----------------------|
| Sync callbacks | Yes (blocking) | Yes (blocking) | Yes (async) | Yes (spawn_blocking) |
| Async callbacks | No | No | Yes (native) | Planned (AsyncExecutor) |
| Worker pool | No (single-threaded loop) | No (single-threaded) | Yes (Celery-like) | Yes (Tokio-based) |
| Per-topic queues | No | No | Yes (topic routing) | Yes (Dispatcher, v1.1) |
| Retry with backoff | No (user-implemented) | No | Yes (builtin) | Via RetryExecutor (P2) |
| Batch processing | No | No | Yes (streaming) | Via BatchExecutor (P3) |
| Commit batching | No (auto-commit only) | Yes (manual) | Yes (changelog) | Via CommitExecutor (P2) |

## "Normal" vs Anti-Patterns

### Normal Behavior

```
WorkerPool::new(handlers, 4 workers)
    └── spawns 4 Tokio tasks
            └── each task:
                loop {
                    message = receiver.recv().await  // Parked while waiting — no CPU
                    spawn_blocking {
                        Python::attach(|py| {
                            callback.call1(py, (KafkaMessage::from(message),))
                        })
                    }.await
                    queue_manager.ack(topic, 1)
                }
```

**What good looks like:**
1. Worker tasks are parked on `.recv()` — zero CPU while waiting for messages
2. GIL acquired only inside `spawn_blocking` closure — other Tokio tasks run freely
3. After Python call completes, `ack()` updates counters and loop continues
4. Slow Python handler blocks only one worker, not the entire pool
5. Worker startup logs "Worker-0 started", message pickup logs "Worker-0 processing offset=123"

### Anti-Pattern 1: GIL Held in Async Context

```rust
// ANTI-PATTERN — GIL held while async task is parked
while let Some(msg) = stream.next().await {
    Python::with_gil(|py| {  // GIL held HERE — async task blocked
        let handler = handlers.get(&msg.topic).unwrap();
        handler.call1(py, (msg,));
    });
}
```

**What goes wrong:** GIL is held for the entire duration of the async stream iteration. Tokio cannot process other tasks while Python runs. Throughput is terrible — you get single-threaded Python behavior with async overhead.

**Detection:** If `spawn_blocking` is not used, this is the anti-pattern.

### Anti-Pattern 2: &PyAny Instead of Py<PyAny>

```rust
// ANTI-PATTERN — reference to Python object, not owned
struct Handler {
    callback: &PyAny,  // Not Send+Sync — can't use across threads
}
```

**What goes wrong:** `&PyAny` borrows from the GIL context — not `Send`, not `Sync`. Cannot be stored in Tokio task. Must use `Py<PyAny>` (owned reference) for GIL-independent storage.

**Detection:** If callbacks are stored as `&PyAny` or `Bound<'_, PyAny>`, Tokio tasks cannot hold them.

### Anti-Pattern 3: Per-Message Commits

```rust
// ANTI-PATTERN — expensive commit per message
for msg in consumer {
    process(msg);
    consumer.commit_consumer_state();  // Kafka round-trip per message
}
```

**What goes wrong:** Each commit is a network round-trip to Kafka brokers. At high throughput, commit overhead dominates processing time. Commit batching amortizes cost across N messages.

**Detection:** If `commit()` is called inside the message loop, it is the anti-pattern.

### Anti-Pattern 4: Unbounded Worker Pool

```rust
// ANTI-PATTERN — no limit on workers
for _ in 0..1000 {
    tokio::spawn(async move { /* process */ });
}
```

**What goes wrong:** 1000 workers all competing for the Python GIL — massive context switching, GIL thrashing, interpreter destabilization. Workers should be bounded to CPU core count or a small multiple.

**Detection:** If worker count is not bounded or defaults to unbounded, it is the anti-pattern.

### Anti-Pattern 5: No Result Normalization

```rust
// ANTI-PATTERN — ignore callback return value
handler.call1(py, (msg,));
// No check of return value — all successes look the same
```

**What goes wrong:** Python callback can return `False`, `None`, raise an exception — all have different semantics. Without normalization, rejected messages are indistinguishable from successful ones.

**Detection:** If callback return value is discarded, it is the anti-pattern.

## Sources

- PyO3 documentation: `Py<PyAny>` is owned reference, `Bound<'_, PyAny>` is temporary borrow
- Tokio `spawn_blocking`: transfers thread to blocking async context, releases tokio thread pool slot
- `pyo3-async-runtimes` `future_into_py`: bridges Rust async to Python async
- KafPy v1.0 `ConsumerRunner` and v1.1 `Dispatcher` source code
- `HandlerMetadata::ack()` in `queue_manager.rs` — closes the dispatch/ack loop

---

*Feature research for: Python Execution Lane (v1.2)*
*Researched: 2026-04-16*