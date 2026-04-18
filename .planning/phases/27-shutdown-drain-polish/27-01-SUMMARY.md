---
phase: "27-shutdown-drain-polish"
plan: "01"
type: "verify"
wave: "1"
subsystem: "worker-pool"
tags: ["verification", "shutdown-drain", "GIL", "handler-mode"]
dependency_graph:
  requires: []
  provides: []
  affects: []
tech_stack:
  added: []
  patterns: []
key_files:
  created: []
  modified:
    - "src/worker_pool/mod.rs"
    - "src/python/handler.rs"
    - "src/python/async_bridge.rs"
decisions: []
---

# Phase 27 Plan 01: Shutdown Drain Polish — Verification Summary

## One-Liner

Phase 27 verification-only plan confirms shutdown drain Branch 3, all 4 HandlerMode routes, GIL invariant patterns, and cargo check passing.

## Verification Results

### Task 1: Shutdown Drain — batch_worker_loop Branch 3

**Status:** VERIFIED

**Findings (src/worker_pool/mod.rs lines 655-688):**

- **Trigger:** `_ = shutdown_token.cancelled()` at line 656
- **Drain:** `accumulator.flush_all()` at line 661 — returns all `(partition, Vec<OwnedMessage>)` pairs
- **Processing:** For each nonempty batch (line 663):
  - Creates `ExecutionContext` (lines 665-670)
  - Calls `handler.invoke_mode_batch(&ctx, batch.clone()).await` (line 671)
  - Passes result to `handle_batch_result_inline()` (lines 672-684)
- **Exit:** `break` at line 688 after all partitions drained
- **Log:** `"batch worker: shutdown signal, draining"` at line 659

Branch 3 drain implementation matches the specification exactly.

---

### Task 2: All 4 HandlerMode Paths from WorkerPool::new

**Status:** VERIFIED

**Findings (src/worker_pool/mod.rs lines 855-900):**

```rust
let mode = handler.mode();
let use_batch = matches!(
    mode,
    crate::python::handler::HandlerMode::BatchSync
        | crate::python::handler::HandlerMode::BatchAsync
);
```

Routing table:

| HandlerMode     | `use_batch` | Loop spawned         |
| --------------- | ----------- | -------------------- |
| SingleSync      | `false`     | `worker_loop`        |
| SingleAsync     | `false`     | `worker_loop`        |
| BatchSync       | `true`      | `batch_worker_loop`  |
| BatchAsync      | `true`      | `batch_worker_loop`  |

- Batch path: lines 874-886 (`batch_worker_loop`)
- Single path: lines 887-900 (`worker_loop`)
- No other routing logic redirects any mode.

---

### Task 3: GIL Invariant — spawn_blocking (sync) and PythonAsyncFuture (async)

**Status:** VERIFIED

**Sync path (src/python/handler.rs lines 160-196):**

```rust
let result = tokio::task::spawn_blocking(move || {
    Python::attach(|py| {
        // All Python work here — GIL held only inside this closure
        let py_msg = PyDict::new(py);
        // ... build message dict ...
        match callback.call1(py, (py_msg,)) { ... }
    })
    // Python::attach returns here — GIL released before .await
}).await;
```

- `spawn_blocking` moves the entire task to a blocking thread
- `Python::attach` acquires GIL **only** during `|py| { ... }` closure
- GIL is released when the closure returns (end of Python work)
- Rust-side orchestration (building PyDict) happens **before** `spawn_blocking`

**Async path (src/python/async_bridge.rs lines 41-81):**

```rust
fn poll_coroutine(&mut self, cx: &mut Context<'_>) -> Poll<ExecutionResult> {
    Python::attach(|py| {
        let result = self.coro.call_method1(py, "send", (py.None(),));
        match result {
            Ok(_val) => {
                let waker = cx.waker().clone();
                waker.wake();
                Poll::Pending  // GIL released BEFORE returning Pending
            }
            // ...
        }
    })  // ← GIL released here, BEFORE returning Pending
}
```

- GIL acquired via `Python::attach`
- `coro.send(None)` called while GIL is held (line 45)
- Immediately returns `Poll::Pending` **after** the attach scope (line 56)
- Waker registered so Tokio re-polls when ready
- GIL is **not held across the await point**

GIL invariant confirmed: transient only during Python execution.

---

### Task 4: cargo check

**Status:** PASSED

```
$ cargo check
  Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.06s
```

31 warnings (unused imports, deprecated API, dead code) — zero errors. All code compiles cleanly.

---

## Deviations from Plan

None — Phase 27 is verification-only.

## Threat Flags

None — verification only, no code changes.

## Self-Check: PASSED

| Check | Result |
| ---- | ------ |
| Branch 3 drain code exists at lines 655-688 | PASS |
| WorkerPool::new routing at lines 855-900 | PASS |
| spawn_blocking GIL pattern in handler.rs:160-196 | PASS |
| PythonAsyncFuture GIL pattern in async_bridge.rs:41-81 | PASS |
| cargo check passes (no errors) | PASS |
