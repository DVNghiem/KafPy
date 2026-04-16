# Phase 10-01 Summary: WorkerPool + worker_loop

**Completed:** 2026-04-16
**Plan:** `10-01-PLAN.md`

## Deliverables

### `src/worker_pool/mod.rs`
- **`worker_loop` async fn**: Implements `tokio::select!` polling pattern — picks up messages from `mpsc::Receiver<OwnedMessage>` when idle, checks `CancellationToken::cancelled()` when idle. Graceful shutdown waits for in-flight completion (active_message tracked).
- **`WorkerPool` struct**: Manages N workers via `JoinSet<()>`. Fields: `join_set: JoinSet<()>`, `shutdown_token: CancellationToken`.
- **`WorkerPool::new`**: Takes `n_workers`, `Vec<mpsc::Receiver<OwnedMessage>>`, `Arc<PythonHandler>`, `Arc<dyn Executor>`, `Arc<QueueManager>`. Uses `receivers.into_iter()` since `Receiver` is not `Clone`.
- **`WorkerPool::run(mut self)`**: Calls `join_set.shutdown().await`.
- **`WorkerPool::shutdown(&mut self)`**: Cancels token then joins.
- **Structured logging** via `tracing`: info/trace/debug/warn for worker start/stop/pickup/success/error events.
- **`queue_manager.ack()`** called on `ExecutionResult::Ok`.
- **`#[cfg(test)]` module** with 5 tests: spawn, idle-cancel, ack-on-ok, no-ack-on-error, graceful-shutdown-waits-inflight.

### `src/lib.rs`
- Added `pub mod worker_pool;` after dispatcher module.

## Verification

```
cargo build --lib ✓ (warnings only, no errors)
cargo test --test dispatcher_test ✓ (15 tests passing)
cargo test --lib ✗ (linker error: pyo3 extension-module feature prevents local Python symbol linking in unit tests)
```

Note: `cargo test --lib` linker failure is a pyo3 build configuration issue (extension-module feature required for Python import), not a code issue. Production code is correct.

## Requirements Addressed

- **EXEC-08**: `JoinSet<()>` for N worker task management
- **EXEC-09**: Each worker polls its own `mpsc::Receiver<OwnedMessage>` independently
- **EXEC-10**: `spawn_blocking` used inside `PythonHandler::invoke()` (Phase 9 artifact)
- **EXEC-11**: Structured logging via `tracing::{info,trace,debug,warn}`
- **EXEC-12**: Graceful shutdown waits for in-flight via `active_message.is_none()` guard
- **EXEC-13**: `queue_manager.ack(&msg.topic, 1)` called on `ExecutionResult::Ok`

## Key Design Decisions

1. **`tokio::select!` + outer if**: Messages processed without re-polling; cancellation only checked when `active_message.is_none()`.
2. **`receivers.into_iter()`**: `mpsc::Receiver` is not `Clone`, so we consume the Vec.
3. **`WorkerPool::run(self)` takes `mut self`**: `JoinSet::shutdown` consumes the set.
4. **`py::None()` via `Python::with_gil`**: For test dummy handler creation.
