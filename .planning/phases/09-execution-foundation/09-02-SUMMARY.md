# Phase 09-02 Summary: PythonHandler + PyO3 Bridge

**Completed:** 2026-04-16
**Plan:** `09-02-PLAN.md`

## Deliverables

### `src/python/handler.rs`
- `PythonHandler` struct: stores `Arc<Py<PyAny>>` (Gil-independent, Send+Sync)
- `invoke(ctx, message) -> ExecutionResult`: async method using `tokio::spawn_blocking`
- GIL acquired only inside `Python::attach` closure (minimal window)
- Converts `OwnedMessage` to `PyDict` (py_msg) for Python callable
- Handles `MessageTimestamp` variants (NotAvailable, CreateTime, LogAppendTime)
- Normalizes exceptions to `ExecutionResult::Error { exception, traceback }`
- Handles `spawn_blocking` panic → `ExecutionResult::Error { exception: "Panic", ... }`

### `src/python/mod.rs`
- Added `pub mod handler;`
- Added `pub use handler::PythonHandler;`

### `src/pyconsumer.rs` (pre-existing, verified)
- `add_handler(topic, callback: Bound<'_, PyAny>)` uses `callback.unbind()` at PyO3 boundary
- Stores `Py<PyAny>` in `Arc<Mutex<HashMap<String, Py<PyAny>>>>`
- Handler registration with dispatcher deferred to Phase 10 (Worker Pool)

## Verification

```
cargo build --lib ✓ (warnings only, no errors)
cargo test --lib ✓ (9 tests passing)
```

## Requirements Addressed

- **EXEC-06**: `Py<PyAny>` storage (not `&PyAny` or `Bound`)
- **EXEC-07**: `spawn_blocking` for GIL release during Python calls
- **EXEC-15**: `callback.unbind()` at PyO3 boundary → owned `Py<PyAny>`
- **EXEC-16**: Python callable receives KafkaMessage dict (topic/partition/offset/payload/timestamp/headers)
- **EXEC-17**: GIL held only inside `Python::attach` closure

## Key Design Decisions

1. **`Python::attach`** (not `Python::with_gil`): Modern PyO3 API, no deprecation warning
2. **`Arc<Py<PyAny>>`**: Cheap cloning for `spawn_blocking` closure capture
3. **Defensive panic handling**: `spawn_blocking` task panic returns `ExecutionResult::Error` (not propagated)
4. **Dict-based message**: `Py<PyAny>` callable receives `&PyDict` (KafkaMessage dict, not custom class)

## Deferred to Phase 10

- `add_handler` dispatcher registration (dispatcher not yet connected to consumer runner)
- Worker pool (`JoinSet`, graceful shutdown, per-worker `PythonHandler` instances)
- `ConsumerDispatcher` integration with `ConsumerRunner::stream()`

## Notes

- Fixed `name().to_string()` → `name().map(|s| s.to_string()).unwrap_or_else(|_| "Unknown".to_string())` for `PyErr::get_type` API change
- All pre-existing warnings in `dispatcher/` and `pyconsumer.rs` are unrelated to Phase 9 work
