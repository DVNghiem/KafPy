# Phase 10: Worker Pool - Research

**Researched:** 2026-04-16
**Domain:** Tokio async worker pool with JoinSet, CancellationToken graceful shutdown, Python handler integration
**Confidence:** HIGH

## Summary

Phase 10 builds the `WorkerPool` that connects the `ConsumerDispatcher`'s handler queues to the `PythonHandler` execution layer. Workers poll `mpsc::Receiver<OwnedMessage>` channels independently, invoke Python callbacks via `spawn_blocking`, and report outcomes through `QueueManager::ack()`. Graceful shutdown uses `CancellationToken` propagated to all workers so they finish in-flight work before exiting.

**Primary recommendation:** Create `src/worker_pool/mod.rs` with `WorkerPool` struct owning a `JoinSet<()>`; per-worker logic lives in an async fn that `select!`s on `recv()` and `cancelled()`; `CancellationToken` shared via cloning; `QueueManager::ack()` called directly by each worker on `ExecutionResult::Ok`.

## Context

This phase has NO CONTEXT.md - it starts fresh. All decisions are made in this research based on existing Phase 9 implementations.

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| EXEC-08 | WorkerPool — configurable N workers managed via Tokio JoinSet | JoinSet API, worker loop pattern |
| EXEC-09 | Each worker polls its assigned mpsc::Receiver<OwnedMessage> independently | mpsc Receiver pattern |
| EXEC-10 | spawn_blocking per-message — Tokio threads released during Python call | Already implemented in PythonHandler::invoke |
| EXEC-11 | Structured logging: worker start/stop, message pickup, handler success/failure (via tracing) | tracing crate already in use |
| EXEC-12 | Graceful shutdown: complete in-flight messages before worker exit; CancellationToken propagated to workers | tokio_util CancellationToken |
| EXEC-13 | QueueManager::ack() called on ExecutionResult::Ok — closes inflight counter | QueueManager::ack(topic, count) existing API |

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Worker task lifecycle | Tokio runtime (JoinSet) | — | JoinSet owns worker task handles |
| Per-worker message polling | Worker async task | — | Each worker has its own Receiver |
| Python invocation | PythonHandler (spawn_blocking) | Tokio blocking threadpool | GIL held only inside Python::attach closure |
| Shutdown coordination | CancellationToken | TaskTracker (optional) | Token cloned to each worker |
| Outcome ack | Worker (calls QueueManager) | — | Worker has access to QueueManager reference |
| Consumer loop | ConsumerDispatcher | ConsumerRunner | Dispatcher sends to handler channels |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| tokio | 1.40 | Async runtime, JoinSet | Project already uses tokio |
| tokio-util | 0.7.17 | CancellationToken | Already in Cargo.toml; provides graceful shutdown |
| tracing | 0.1.44 | Structured logging | Project already uses tracing |

### No New Dependencies Required
- `tokio::task::JoinSet` — built into tokio
- `tokio_util::sync::CancellationToken` — already in tokio-util 0.7.17
- `mpsc::Receiver<OwnedMessage>` — already provided by `Dispatcher::register_handler`

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| JoinSet | Manual `Vec<JoinHandle>` + `futures::future::join_all` | JoinSet provides `abort_all`/`shutdown` which is cleaner for graceful shutdown |
| CancellationToken | `broadcast::Receiver` shutdown signal | CancellationToken is purpose-built for this; broadcast requires more boilerplate |
| TaskTracker | Just JoinSet | TaskTracker adds tracking/waiting semantics; not needed since JoinSet already tracks |

## Architecture Patterns

### System Architecture Diagram

```
Kafka
  │
  ▼
ConsumerRunner::stream()  ──OwnedMessage──▶ ConsumerDispatcher::send()
                                                │
                    ┌───────────────────────────┼───────────────────────────┐
                    │                           │                           │
                    ▼                           ▼                           ▼
              mpsc:topic-A                 mpsc:topic-B                 mpsc:topic-N
            Receiver<OwnedMessage>      Receiver<OwnedMessage>      Receiver<OwnedMessage>
                    │                           │                           │
                    ▼                           ▼                           ▼
              Worker 0                       Worker 1                   Worker N-1
         (JoinSet task)                (JoinSet task)             (JoinSet task)
                    │                           │                           │
                    ▼                           ▼                           ▼
            PythonHandler::invoke()    PythonHandler::invoke()   PythonHandler::invoke()
         (spawn_blocking + GIL)       (spawn_blocking + GIL)   (spawn_blocking + GIL)
                    │                           │                           │
                    ▼                           ▼                           ▼
              Executor::execute()          Executor::execute()         Executor::execute()
           (DefaultExecutor → Ack)     (DefaultExecutor → Ack)   (DefaultExecutor → Ack)
                    │                           │                           │
                    └───────────────────────────┼───────────────────────────┘
                                                ▼
                                    QueueManager::ack(topic, 1)
                                    (decrements inflight counter)
```

### Recommended Project Structure
```
src/
├── worker_pool/
│   ├── mod.rs           # WorkerPool struct, worker fn, shutdown logic
│   └── lib.rs
├── dispatcher/
│   ├── mod.rs           # ConsumerDispatcher (existing)
│   ├── queue_manager.rs # QueueManager (existing)
│   └── backpressure.rs
├── python/
│   ├── mod.rs           # PythonHandler, Executor (existing)
│   ├── handler.rs
│   ├── executor.rs
│   ├── context.rs
│   └── execution_result.rs
├── pyconsumer.rs        # PyO3 bridge (existing)
└── lib.rs
```

### Pattern 1: Worker Loop with select! + CancellationToken

**What:** Each worker runs an async loop that `select!`s on message receipt and cancellation.

**When to use:** When a task must process messages until cancelled but also respond to shutdown signals mid-loop.

**Example:**
```rust
// Source: tokio-util docs + verified against tokio-util 0.7.17
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tokio::select;

async fn worker_loop(
    mut rx: mpsc::Receiver<OwnedMessage>,
    handler: Arc<PythonHandler>,
    executor: Arc<dyn Executor>,
    queue_manager: Arc<QueueManager>,
    worker_id: usize,
    shutdown_token: CancellationToken,
) {
    let mut active_message: Option<OwnedMessage> = None;

    loop {
        select! {
            // Priority: finish in-flight first, then accept new
            Some(msg) = rx.recv(), if active_message.is_none() => {
                tracing::trace!(
                    worker_id = worker_id,
                    topic = %msg.topic,
                    offset = msg.offset,
                    "worker picked up message"
                );
                active_message = Some(msg);
            }
            // Check cancellation when not processing
            _ = shutdown_token.cancelled(), if active_message.is_none() => {
                tracing::info!(worker_id = worker_id, "worker收到了取消信号，无活动中消息，退出");
                break;
            }
        }

        // Process active_message if any
        if let Some(msg) = active_message.take() {
            let ctx = ExecutionContext::new(
                msg.topic.clone(),
                msg.partition,
                msg.offset,
                worker_id,
            );

            // PythonHandler::invoke uses spawn_blocking internally (EXEC-10)
            let result = handler.invoke(&ctx, msg.clone()).await;

            // Executor decides outcome (EXEC-05: DefaultExecutor always acks)
            let outcome = executor.execute(&ctx, &msg, &result);

            // EXEC-13: QueueManager::ack() on ExecutionResult::Ok
            if matches!(result, ExecutionResult::Ok) {
                queue_manager.ack(&msg.topic, 1);
            }

            // EXEC-12: Check cancellation after processing (finish in-flight first)
            if shutdown_token.is_cancelled() {
                tracing::info!(
                    worker_id = worker_id,
                    topic = %msg.topic,
                    offset = msg.offset,
                    "worker完成飞行中消息后收到取消信号，退出"
                );
                break;
            }
        }
    }

    tracing::info!(worker_id = worker_id, "worker stopped");
}
```

**Anti-pattern to avoid:** Do NOT use `tokio::spawn_blocking` inside the worker loop directly. `PythonHandler::invoke` already handles `spawn_blocking`. Adding another layer of blocking defeats the purpose.

### Pattern 2: WorkerPool with JoinSet

**What:** `WorkerPool` owns a `JoinSet` and spawns all workers at construction.

**When to use:** When you need to manage N identical worker tasks with uniform shutdown.

**Example:**
```rust
// Source: docs.rs/tokio/latest/tokio/task/struct.JoinSet.html
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

pub struct WorkerPool {
    join_set: JoinSet<()>,
    shutdown_token: CancellationToken,
}

impl WorkerPool {
    /// Spawns N workers, each polling its own receiver.
    pub fn new(
        n_workers: usize,
        receivers: Vec<mpsc::Receiver<OwnedMessage>>,
        handler: Arc<PythonHandler>,
        executor: Arc<dyn Executor>,
        queue_manager: Arc<QueueManager>,
    ) -> Self {
        let shutdown_token = CancellationToken::new();
        let mut join_set = JoinSet::new();

        for worker_id in 0..n_workers {
            let rx = receivers
                .get(worker_id)
                .expect("receivers count must match n_workers")
                .clone();
            let handler = Arc::clone(&handler);
            let executor = Arc::clone(&executor);
            let queue_manager = Arc::clone(&queue_manager);
            let token = shutdown_token.clone();

            join_set.spawn(async move {
                worker_loop(rx, handler, executor, queue_manager, worker_id, token).await;
            });
        }

        Self { join_set, shutdown_token }
    }

    /// Signals all workers to stop and waits for them to finish.
    pub async fn shutdown(mut self) {
        tracing::info!("initiating worker pool shutdown");
        self.shutdown_token.cancel();

        // JoinSet::shutdown aborts remaining tasks and waits for clean exit
        // This waits for all workers to complete their in-flight messages (EXEC-12)
        self.join_set.shutdown().await;
        tracing::info!("worker pool shutdown complete");
    }
}
```

**Key insight:** `JoinSet::shutdown()` (not `abort_all()`) cleanly waits for workers to finish rather than forcefully killing them. This satisfies EXEC-12: "complete in-flight messages before worker exit."

### Pattern 3: Graceful Shutdown with CancellationToken Propagation

**What:** A single `CancellationToken` is cloned for each worker; cancelling the original propagates to all.

**Example:**
```rust
// Source: docs.rs/tokio-util/latest/tokio_util/sync/struct.CancellationToken.html
let token = CancellationToken::new();
let token_clone = token.clone();

tokio::spawn(async move {
    select! {
        _ = token_clone.cancelled() => {
            // Clean up and exit
        }
        result = long_running_task() => {
            // Process result
        }
    }
});

// Later: cancel triggers all clones
token.cancel();
```

### Pattern 4: Connecting WorkerPool to ConsumerDispatcher

**What:** `ConsumerDispatcher` owns the `WorkerPool`; `register_handler` returns receivers passed to workers.

**Example:**
```rust
// In ConsumerDispatcher::run()
pub async fn run(&self, n_workers: usize) {
    // Register handlers and collect receivers
    let receivers: Vec<_> = self
        .dispatcher
        .handlers()
        .map(|(topic, capacity)| {
            self.dispatcher.register_handler(topic, capacity, None)
        })
        .collect();

    // Create worker pool with receivers
    let pool = WorkerPool::new(
        n_workers,
        receivers,
        self.handler.clone(),
        self.executor.clone(),
        self.queue_manager.clone(),
    );

    // Run consumer dispatch loop + worker pool concurrently
    tokio::join!(
        self.run_dispatch_loop(),
        pool.shutdown(),
    );
}
```

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Worker lifecycle management | Manual Vec<JoinHandle> + manual abort | JoinSet | JoinSet::shutdown() handles graceful cleanup atomically |
| Cancellation propagation | Channel-based shutdown signal | CancellationToken | Purpose-built for this; much less boilerplate than broadcast |
| Python GIL management | Custom blocking thread pool | PythonHandler::invoke (spawn_blocking) | Already implemented in Phase 9; PyO3-safe |

**Key insight:** Phase 9 already implemented `PythonHandler::invoke` with `spawn_blocking`. EXEC-10 ("spawn_blocking per-message") is already satisfied — workers just call `handler.invoke()`.

## Common Pitfalls

### Pitfall 1: CancellationToken cloned AFTER spawn
**What goes wrong:** If you clone the token inside the spawned task (rather than before), the original token may be dropped and cancellation won't propagate.
**How to avoid:** Clone `token` before `join_set.spawn()`, pass the clone into the async block.
**Warning signs:** Workers ignore `token.cancel()` calls.

### Pitfall 2: workers calling ack() AFTER shutdown_token cancelled
**What goes wrong:** After `shutdown_token.cancel()`, workers should finish in-flight but NOT call `ack()` for new messages (there shouldn't be new messages if receiver is closed properly).
**How to avoid:** The `select!` pattern with `if active_message.is_none()` guard on the recv branch ensures we only accept new messages when not shutting down.

### Pitfall 3: JoinSet::abort_all() vs JoinSet::shutdown()
**What goes wrong:** `abort_all()` forcefully cancels tasks — workers won't complete in-flight messages (violates EXEC-12).
**How to avoid:** Use `shutdown()` which aborts and waits for graceful exit.
**Warning signs:** Messages being processed when consumer stops.

### Pitfall 4: Dropped receivers before workers finish
**What goes wrong:** If `ConsumerDispatcher::run()` returns before workers finish, receivers get dropped and workers exit prematurely.
**How to avoid:** `shutdown()` must be awaited (via `tokio::join!` or similar) before `run()` returns.

## Code Examples

### Tracing structured log events (EXEC-11)

```rust
// Worker start
tracing::info!(
    worker_id = worker_id,
    topic = %topic,
    "worker started"
);

// Message pickup
tracing::trace!(
    worker_id = worker_id,
    topic = %msg.topic,
    partition = msg.partition,
    offset = msg.offset,
    "worker picked up message"
);

// Success
tracing::debug!(
    worker_id = worker_id,
    topic = %ctx.topic,
    partition = ctx.partition,
    offset = ctx.offset,
    "handler executed successfully"
);

// Failure
tracing::warn!(
    worker_id = worker_id,
    topic = %ctx.topic,
    partition = ctx.partition,
    offset = ctx.offset,
    exception = %exception,
    "handler raised exception"
);
```

### QueueManager ack integration (EXEC-13)

```rust
// From queue_manager.rs - existing API:
pub fn ack(&self, topic: &str, count: usize) {
    if let Some(entry) = self.handlers.lock().get(topic) {
        entry.metadata.ack(count); // decrements queue_depth and inflight
    }
}

// In worker loop:
let result = handler.invoke(&ctx, msg.clone()).await;
match result {
    ExecutionResult::Ok => {
        queue_manager.ack(&msg.topic, 1);
    }
    ExecutionResult::Error { .. } | ExecutionResult::Rejected { .. } => {
        // Error/rejected — inflight already counted; no ack
        // Logged by DefaultExecutor
    }
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Synchronous Python calls in consumer loop | spawn_blocking per message | Phase 9 | Tokio threads no longer blocked during Python calls |
| No worker pool | JoinSet-managed N workers | Phase 10 | Parallel message processing, bounded concurrency |
| No graceful shutdown | CancellationToken + JoinSet::shutdown | Phase 10 | In-flight messages complete before exit |
| No ack tracking | QueueManager::ack on ExecutionResult::Ok | Phase 10 | Backpressure correctly tracks in-flight |

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `PythonHandler::invoke` uses `spawn_blocking` internally (satisfies EXEC-10) | Standard Stack | Verified in handler.rs line 38-63 |
| A2 | `tokio-util 0.7.17` includes `CancellationToken` | Environment | Verified in Cargo.toml |
| A3 | Each handler registered via `ConsumerDispatcher::register_handler` gets its own `mpsc::Receiver` | Architecture | Verified in dispatcher/mod.rs |
| A4 | `JoinSet::shutdown()` waits for tasks to exit (not just abort) | Architecture | Verified in tokio docs: "aborts all tasks and waits for them to finish" |

## Open Questions

1. **How does WorkerPool get the handler (PythonHandler)?**
   - The `ConsumerDispatcher` currently owns no handler. In `pyconsumer.rs`, the Python callable is stored as `Py<PyAny>` in a `HashMap`.
   - Phase 10 needs to create `PythonHandler` instances and pass them to workers.
   - Recommendation: `WorkerPool::new()` takes `Arc<PythonHandler>`. The `ConsumerDispatcher` (or a new `ExecutionLayer` struct) owns the `PythonHandler` instances and passes them to the pool.

2. **Should WorkerPool be part of `src/python/` or `src/dispatcher/`?**
   - `python/` already has `PythonHandler`, `Executor`, `ExecutionContext` — the execution layer.
   - `WorkerPool` orchestrates the execution layer but is also the bridge to the dispatcher.
   - Recommendation: Create `src/worker_pool/mod.rs` (new top-level module) since it is the orchestrator combining dispatcher queues with python execution.

3. **How many receivers vs workers?**
   - If there are more topics than workers, some workers handle multiple topics (their receiver handles messages from one topic).
   - If there are more workers than topics, some workers get no messages (idle).
   - Recommendation: Use `receivers.len()` as the actual worker count, capped at the configured `n_workers`.

4. **Where is the shutdown signal triggered from?**
   - `pyconsumer.rs` has `stop()` which is currently a TODO.
   - Recommendation: `stop()` sets a flag that `run()` checks; when flag is set, `ConsumerDispatcher` cancels the token and waits for pool shutdown.

## Environment Availability

> Step 2.6: SKIPPED (no external dependencies identified beyond those already in Cargo.toml)

**Verification:**
- `tokio = "1.40"` with `full` features — includes JoinSet ✓
- `tokio-util = "0.7.17"` — includes CancellationToken ✓
- `tracing = "0.1.44"` — structured logging ✓
- All required crates already in Cargo.toml

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | tokio::test + proptest |
| Config file | none — standard cargo test |
| Quick run command | `cargo test --lib worker_pool -- --nocapture` |
| Full suite command | `cargo test --lib` |

### Phase Requirements to Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|--------------|
| EXEC-08 | WorkerPool spawns N workers in JoinSet | unit | `cargo test --lib worker_pool` | needs creation |
| EXEC-09 | Each worker polls its own receiver independently | unit | `cargo test --lib worker_poll_receiver` | needs creation |
| EXEC-10 | spawn_blocking used (via PythonHandler invoke) | verified | PythonHandler already tested in Phase 9 | existing |
| EXEC-11 | Structured logging on worker events | unit | `cargo test --lib worker_log` | needs creation |
| EXEC-12 | Graceful shutdown completes in-flight before exit | integration | `cargo test --lib worker_graceful_shutdown` | needs creation |
| EXEC-13 | QueueManager::ack called on ExecutionResult::Ok | unit | `cargo test --lib ack_on_ok` | needs creation |

### Wave 0 Gaps
- `src/worker_pool/mod.rs` — WorkerPool, worker_loop fn (NEW)
- `src/worker.rs` or inline in mod.rs — worker task logic
- Tests: `#[cfg(test)] mod worker_pool_tests` or `tests/worker_pool_test.rs`
- No new dependencies needed — all tokio/tokio-util already in Cargo.toml

## Security Domain

N/A — WorkerPool is internal async orchestration with no external I/O or user input. Security considerations from Phase 9 (PyO3 GIL safety, no hardcoded secrets) remain unchanged.

## Sources

### Primary (HIGH confidence)
- `src/python/handler.rs` — PythonHandler::invoke with spawn_blocking (verified)
- `src/dispatcher/mod.rs` — ConsumerDispatcher, register_handler (verified)
- `src/dispatcher/queue_manager.rs` — QueueManager::ack (verified)
- `src/python/executor.rs` — Executor trait, DefaultExecutor (verified)
- `src/consumer/runner.rs` — ConsumerRunner::stream (verified)
- docs.rs/tokio/latest/tokio/task/struct.JoinSet.html — JoinSet API
- docs.rs/tokio-util/latest/tokio_util/sync/struct.CancellationToken.html — CancellationToken API

### Secondary (MEDIUM confidence)
- tokio-util TaskTracker graceful shutdown example (contextual example, not exact API)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all crates already in Cargo.toml, APIs verified via docs.rs
- Architecture: HIGH — JoinSet and CancellationToken patterns well-established in tokio ecosystem
- Pitfalls: MEDIUM — cancellation edge cases discovered through tokio docs analysis

**Research date:** 2026-04-16
**Valid until:** 60 days (tokio APIs are stable)
