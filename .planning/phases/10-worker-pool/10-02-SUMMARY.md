# Phase 10-02 Summary: WorkerPool Wiring into pyconsumer.rs

**Completed:** 2026-04-16
**Plan:** `10-02-PLAN.md`

## Deliverables

### `src/pyconsumer.rs`
- **`Consumer` struct**: Added `shutdown_token: tokio_util::sync::CancellationToken` field; `handlers` changed to `Arc<Mutex<HashMap<String, Arc<Py<PyAny>>>>>` (Arc wrapper for Py handle).
- **`add_handler`**: Only stores callbacks in `self.handlers` — no dispatcher wiring (fixes Issue 1 from plan).
- **`start()`**: Creates `ConsumerDispatcher`, registers handlers, builds `WorkerPool` with shared `shutdown_token`, runs dispatcher and pool concurrently.
- **`stop()`**: Calls `self.shutdown_token.cancel()` — all workers detect via `select!` and exit gracefully after in-flight completion.

### `src/dispatcher/mod.rs`
- Added `pub fn queue_manager(&self) -> Arc<QueueManager>` accessor for WorkerPool ack integration.
- Added `use std::sync::Arc` import.

### `src/dispatcher/queue_manager.rs`
- Added `#[derive(Clone)]` to `QueueManager`.
- Changed `handlers` field to `Arc<parking_lot::Mutex<...>>` (enables Clone).
- Updated `QueueManager::new()` to wrap mutex in `Arc::new(...)`.

### `src/python/handler.rs`
- Changed `PythonHandler::new` to accept `Arc<Py<PyAny>>` instead of `Py<PyAny>` (avoids Clone requirement on Py handle).

## Verification

```
cargo build --lib ✓ (warnings only, no errors)
cargo test --test dispatcher_test ✓ (15 tests passing)
```

## Requirements Addressed

- **EXEC-12**: `CancellationToken` shared between `Consumer::stop()` and `WorkerPool` workers via `WorkerPool::new(shutdown_token)`. Workers check `shutdown_token.is_cancelled()` after processing in-flight message.
- **EXEC-13**: `ConsumerDispatcher::queue_manager()` returns `Arc<QueueManager>`; WorkerPool calls `queue_manager.ack()` on `ExecutionResult::Ok`.

## Key Design Decisions

1. **`shutdown_token` passed to `WorkerPool::new`**: Token is created once in `Consumer::new`, cloned for workers in `WorkerPool::new`, cancelled in `Consumer::stop()`. Clean separation.
2. **`handlers: Arc<Mutex<HashMap<String, Arc<Py<PyAny>>>>>`**: Py handles wrapped in Arc so they can be cloned into `PythonHandler::new`.
3. **`pool.run().await` blocks `start()`**: When `stop()` cancels the token, workers exit, `pool.run()` completes, and `start()` returns to Python.
