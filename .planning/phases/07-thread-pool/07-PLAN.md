---
phase: 7
slug: thread-pool
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - Cargo.toml
  - src/rayon_pool.rs
  - src/python/handler.rs
  - src/consumer/config.rs
  - src/runtime/mod.rs
autonomous: true
requirements:
  - SYNC-01
  - SYNC-02
must_haves:
  truths:
    - "Sync handlers execute on Rayon work-stealing pool, not blocking Tokio poll cycle"
    - "ConsumerConfigBuilder::rayon_pool_size(u32) accepts valid pool size (1-256)"
  artifacts:
    - path: "src/rayon_pool.rs"
      provides: "RayonPool struct with spawn(), drain(), abort() methods"
      min_lines: 60
    - path: "src/python/handler.rs"
      provides: "PythonHandler::invoke() dispatches to RayonPool via spawn_blocking"
      contains: "spawn_blocking"
    - path: "src/consumer/config.rs"
      provides: "rayon_pool_size(u32) builder method on ConsumerConfigBuilder"
      contains: "rayon_pool_size"
  key_links:
    - from: "src/rayon_pool.rs"
      to: "src/python/handler.rs"
      via: "Arc<RayonPool> clone passed to PythonHandler"
      pattern: "Arc\\<RayonPool\\>"
    - from: "src/python/handler.rs"
      to: "src/rayon_pool.rs"
      via: "rayon_pool.spawn() call"
      pattern: "rayon_pool\\.spawn"
    - from: "src/consumer/config.rs"
      to: "src/rayon_pool.rs"
      via: "rayon_pool_size passed to RuntimeBuilder"
      pattern: "rayon_pool_size"
---

<objective>
Implement Rayon work-stealing thread pool so sync Python handlers execute on Rayon instead of blocking Tokio's poll cycle. Key injection point: `PythonHandler::invoke()` at `src/python/handler.rs:316` currently uses `tokio::task::spawn_blocking` directly — replace with RayonPool::spawn wrapper. Add `ConsumerConfigBuilder::rayon_pool_size(u32)` for pool size configuration. Default pool size = `num_cpus::get().saturating_sub(2).max(2)` (leaves at least 2 threads for Tokio).
</objective>

<context>
@src/python/handler.rs
@src/consumer/config.rs
@src/worker_pool/pool.rs
@src/shutdown/shutdown.rs

# Existing code contracts

From src/python/handler.rs — PythonHandler struct:
```rust
pub struct PythonHandler {
    callback: Arc<Py<PyAny>>,
    retry_policy: Option<RetryPolicy>,
    mode: HandlerMode,
    batch_policy: Option<BatchPolicy>,
    handler_timeout: Option<Duration>,
    name: String,
}
```

From src/consumer/config.rs — ConsumerConfigBuilder:
```rust
#[derive(Debug, Default)]
pub struct ConsumerConfigBuilder {
    brokers: Option<String>,
    group_id: Option<String>,
    // ... existing fields
    drain_timeout_secs: u64,
    routing_rules: Vec<RoutingRule>,
}

impl ConsumerConfigBuilder {
    pub fn new() -> Self { ... }
    pub fn drain_timeout(mut self, secs: u64) -> Self { ... }
    // ... existing builder methods
}
```

From src/shutdown/shutdown.rs — ShutdownCoordinator:
```rust
pub struct ShutdownCoordinator {
    phase: parking_lot::Mutex<ShutdownPhase>,
    drain_timeout: Duration,
    // ...
}
impl ShutdownCoordinator {
    pub fn drain_timeout(&self) -> Duration { ... }
}
```

From src/worker_pool/pool.rs — WorkerPool::shutdown():
```rust
pub async fn shutdown(&mut self) {
    self.shutdown_token.cancel();
    let drain_timeout = self.coordinator.drain_timeout();
    match tokio::time::timeout(drain_timeout, self.join_set.shutdown()).await { ... }
}
```
</context>

<tasks>

<task type="auto">
  <name>Task 1: Add rayon dependency to Cargo.toml</name>
  <files>Cargo.toml</files>
  <read_first>Cargo.toml</read_first>
  <action>Add `rayon = "1.1"` to Cargo.toml dependencies section (after `num_cpus`). This is verified via crates.io at version 1.1.5. Do NOT add any other new dependencies.</action>
  <acceptance_criteria>
    - Cargo.toml contains `rayon = "1.1"` in [dependencies]
    - cargo check passes with no new warnings
  </acceptance_criteria>
  <verify>cargo check 2>&1 | tail -5</verify>
  <done>Cargo.toml updated with rayon dependency, cargo check passes</done>
</task>

<task type="auto">
  <name>Task 2: Create src/rayon_pool.rs with RayonPool struct</name>
  <files>src/rayon_pool.rs</files>
  <read_first>src/worker_pool/pool.rs</read_first>
  <action>
Create `src/rayon_pool.rs` with a `RayonPool` struct that wraps a `rayon::ThreadPool`. The file MUST contain:

```rust
//! Rayon work-stealing thread pool for sync handler dispatch.
//!
//! Tokio workers dispatch blocking work to this pool so the poll cycle
//! is never blocked. All communication from Rayon back to Tokio uses
//! oneshot channels — Rayon closures MUST NOT call any Tokio APIs.

use rayon::ThreadPool;
use rayon::ThreadPoolBuilder;
use std::sync::Arc;

/// Rayon work-stealing thread pool for offloading sync handler work.
///
/// Tokio workers dispatch to this pool via `spawn()`. The closure runs on
/// a Rayon worker thread. Completion is signaled via oneshot channel passed
/// to `spawn()` — Tokio awaits the receiver. Python GIL calls happen inside
/// the closure via `spawn_blocking` (which uses its own thread, not Tokio's).
///
/// # Critical Rule
/// Rayon closures MUST NOT call `tokio::spawn`, `Handle::current()`, or any
/// Tokio sync primitive. Use oneshot channels to communicate results back.
pub struct RayonPool {
    pool: ThreadPool,
}

impl RayonPool {
    /// Creates a new RayonPool with `pool_size` threads.
    ///
    /// Returns an error if `pool_size` is 0 or ThreadPoolBuilder fails.
    pub fn new(pool_size: usize) -> Result<Self, rayon::ThreadPoolBuildError> {
        let pool = ThreadPoolBuilder::new()
            .num_threads(pool_size)
            .build()?;
        Ok(Self { pool })
    }

    /// Spawns a blocking closure on the Rayon thread pool.
    ///
    /// The closure runs on a Rayon worker thread. It MUST NOT call any Tokio
    /// APIs (deadlock/panic risk). Use `spawn_blocking` inside the closure
    /// for Python GIL calls.
    ///
    /// Returns the Rayon JoinHandle — caller awaits to get the result.
    pub fn spawn<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        self.pool.spawn(f);
    }

    /// Initiates graceful drain of the pool.
    ///
    /// Uses `ThreadPool::shutdown()` with `Wait::All` — waits for all
    /// in-flight work to complete. This is called from the Tokio side
    /// during worker pool shutdown.
    pub fn drain(&self) {
        self.pool.shutdown();
    }

    /// Forces immediate abort of all pending work.
    ///
    /// Use only when drain timeout is exceeded. Panics are caught and
    /// logged by the pool.
    pub fn abort(&self) {
        self.pool.shutdown();
    }
}

impl std::fmt::Debug for RayonPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RayonPool").finish()
    }
}
```

Also add `pub mod rayon_pool;` to `src/lib.rs`.
</action>
  <acceptance_criteria>
    - src/rayon_pool.rs exists and contains `pub struct RayonPool`
    - src/rayon_pool.rs contains `impl RayonPool` with `new()`, `spawn()`, `drain()`, `abort()`
    - src/rayon_pool.rs contains the critical rule comment about not calling Tokio APIs from Rayon closures
    - src/lib.rs contains `pub mod rayon_pool;`
    - cargo check passes with no errors
  </acceptance_criteria>
  <verify>cargo check 2>&1 | tail -5</verify>
  <done>src/rayon_pool.rs created, src/lib.rs updated, cargo check passes</done>
</task>

<task type="auto">
  <name>Task 3: Add rayon_pool_size to ConsumerConfigBuilder</name>
  <files>src/consumer/config.rs</files>
  <read_first>src/consumer/config.rs</read_first>
  <action>
In `src/consumer/config.rs`, add `rayon_pool_size: Option<u32>` field to `ConsumerConfig` struct and `ConsumerConfigBuilder` struct.

Then add the builder method to `ConsumerConfigBuilder`:

```rust
/// Sets the Rayon thread pool size for sync handler dispatch.
///
/// Default: `num_cpus::get().saturating_sub(2).max(2)` threads.
/// Valid range: 1-256. Values outside this range cause build() to error.
pub fn rayon_pool_size(mut self, size: u32) -> Self {
    self.rayon_pool_size = Some(size);
    self
}
```

In `ConsumerConfig::build()`:
- Validate that `rayon_pool_size` is in range 1-256 if set
- If not set, compute default: `num_cpus::get().saturating_sub(2).max(2)`

In `ConsumerConfig::into_rdkafka_config()` — no changes needed (this field is not passed to rdkafka).

Also update `ConsumerConfig` struct to store the field:
```rust
pub drain_timeout_secs: u64,
pub rayon_pool_size: Option<usize>,  // computed from u32 in builder
pub routing_rules: Vec<RoutingRule>,
```
</action>
  <acceptance_criteria>
    - ConsumerConfigBuilder has `rayon_pool_size(self, size: u32) -> Self` method
    - ConsumerConfig has `rayon_pool_size: Option<usize>` field
    - build() validates 1-256 range and computes default as `num_cpus::get().saturating_sub(2).max(2)`
    - cargo check passes
  </acceptance_criteria>
  <verify>cargo check 2>&1 | tail -10</verify>
  <done>ConsumerConfigBuilder has rayon_pool_size method, validation in build(), cargo check passes</done>
</task>

<task type="auto">
  <name>Task 4: Modify PythonHandler to use RayonPool</name>
  <files>src/python/handler.rs</files>
  <read_first>src/python/handler.rs</read_first>
  <action>
Modify `PythonHandler` struct in `src/python/handler.rs` to store an `Arc<RayonPool>`:

```rust
use crate::rayon_pool::RayonPool;

pub struct PythonHandler {
    callback: Arc<Py<PyAny>>,
    retry_policy: Option<RetryPolicy>,
    mode: HandlerMode,
    batch_policy: Option<BatchPolicy>,
    handler_timeout: Option<Duration>,
    name: String,
    /// Rayon pool for offloading sync handler work. None means use spawn_blocking directly.
    rayon_pool: Option<Arc<RayonPool>>,
}
```

Add `rayon_pool: Option<Arc<RayonPool>>` parameter to `PythonHandler::new()` and `PythonHandler::with_timeout()`. Update both constructors to store the value.

Modify `PythonHandler::invoke()` (around line 316) to dispatch to RayonPool instead of calling `spawn_blocking` directly:

```rust
pub async fn invoke(&self, ctx: &ExecutionContext, message: OwnedMessage) -> ExecutionResult {
    let callback = Arc::clone(&self.callback);
    let ctx_clone = ctx.clone();
    let rayon_pool = self.rayon_pool.clone();

    // ... extract trace context (unchanged) ...

    let result = if let Some(pool) = rayon_pool {
        // Dispatch to Rayon pool — closure MUST NOT call any Tokio APIs
        let (tx, rx) = tokio::sync::oneshot::channel();
        pool.spawn(move || {
            // On Rayon thread — safe to do CPU preprocessing here
            // MUST NOT call tokio::spawn, Handle::current(), or any Tokio sync primitive
            let r = tokio::task::spawn_blocking(move || {
                Python::attach(|py| {
                    let py_msg = message_to_pydict(py, &message, Some(&trace_context));
                    let py_ctx = ctx_to_pydict(py, &ctx_clone, &message);
                    match callback.call(py, (py_msg, py_ctx), None) {
                        Ok(_) => ExecutionResult::Ok,
                        Err(py_err) => {
                            let classifier = DefaultFailureClassifier;
                            let reason = classifier.classify(&py_err, &ctx_clone);
                            let exception = py_err
                                .get_type(py)
                                .name()
                                .map(|s| s.to_string())
                                .unwrap_or_else(|_| "Unknown".to_string());
                            let traceback = py_err.to_string();
                            ExecutionResult::Error { reason, exception, traceback }
                        }
                    }
                })
            }).join();
            let _ = tx.send(r);
        });
        rx.await.unwrap_or_else(|_| ExecutionResult::Error {
            reason: FailureReason::Terminal(crate::failure::TerminalKind::HandlerPanic),
            exception: "Panic".to_string(),
            traceback: "rayon pool task panicked".to_string(),
        })
    } else {
        // Fallback: use spawn_blocking directly (no Rayon pool configured)
        tokio::task::spawn_blocking(move || { /* ... existing code ... */ }).await
    };

    match result {
        Ok(r) => r,
        Err(_) => ExecutionResult::Error {
            reason: FailureReason::Terminal(crate::failure::TerminalKind::HandlerPanic),
            exception: "Panic".to_string(),
            traceback: "spawn_blocking task panicked".to_string(),
        },
    }
}
```

**Important:** The trace_context HashMap must be captured by the Rayon closure before the function exits. Move it into the spawn closure:

```rust
let trace_context_clone = trace_context; // clone for rayon closure
pool.spawn(move || {
    // use trace_context_clone inside closure
});
```
</action>
  <acceptance_criteria>
    - PythonHandler struct has `rayon_pool: Option<Arc<RayonPool>>` field
    - PythonHandler::new() and PythonHandler::with_timeout() accept rayon_pool parameter
    - PythonHandler::invoke() dispatches to RayonPool when configured, fallback to spawn_blocking otherwise
    - The oneshot channel pattern is used for Tokio-Rayon communication
    - cargo check passes with no errors
  </acceptance_criteria>
  <verify>cargo check 2>&1 | tail -10</verify>
  <done>PythonHandler uses RayonPool for sync dispatch when configured, cargo check passes</done>
</task>

<task type="auto">
  <name>Task 5: Wire RayonPool through RuntimeBuilder</name>
  <files>src/runtime/builder.rs</files>
  <read_first>src/runtime/builder.rs</read_first>
  <action>
In `src/runtime/builder.rs`, modify the build() method to:

1. Compute `RayonPool` size from `rust_config.rayon_pool_size` (default: `num_cpus::get().saturating_sub(2).max(2)`)
2. Create `Arc<RayonPool>` with the computed size
3. Pass `Arc<RayonPool>` to each `PythonHandler` during construction

In build() after creating the coordinator:
```rust
// Compute rayon pool size: configured value or default (num_cpus - 2, min 2)
let rayon_pool_size = self.config.rayon_pool_size
    .unwrap_or_else(|| {
        std::cmp::max(num_cpus::get().saturating_sub(2), 2)
    });
let rayon_pool = Arc::new(
    RayonPool::new(rayon_pool_size)
        .expect("failed to create rayon thread pool")
);
```

When creating PythonHandler in the handler_map loop:
```rust
let handler = Arc::new(PythonHandler::with_timeout(
    meta.callback.clone(),
    Some(default_retry_policy.clone()),
    meta.mode.clone(),
    batch_policy,
    timeout,
    topic.clone(),
    Some(Arc::clone(&rayon_pool)),  // NEW: pass rayon pool to handler
));
```

In `src/runtime/mod.rs`, add `pub mod rayon_pool;` so the module is accessible from runtime builder.
</action>
  <acceptance_criteria>
    - RuntimeBuilder creates RayonPool with computed size
    - RayonPool is passed to PythonHandler constructors via Arc clone
    - Default pool size uses num_cpus::get().saturating_sub(2).max(2)
    - cargo check passes with no errors
  </acceptance_criteria>
  <verify>cargo check 2>&1 | tail -10</verify>
  <done>RayonPool wired through RuntimeBuilder to PythonHandler, cargo check passes</done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| Tokio worker -> RayonPool::spawn | Untrusted input (message) crosses here via oneshot channel |
| Rayon thread -> Python GIL | Blocking call executes on Rayon, uses spawn_blocking for GIL |

## STRIDE Threat Register

| Threat ID | Category | Component | Disposition | Mitigation Plan |
|-----------|----------|-----------|-------------|-----------------|
| T-7-01 | S/T/R/I/D/E | PythonHandler::invoke | mitigate | All Python calls go through spawn_blocking from Rayon; no tokio::spawn from Rayon closures |
| T-7-02 | S/T/R/I/D/E | RayonPool::new | mitigate | pool_size validated 1-256 in build(), Pool new() returns Result<, ThreadPoolBuildError> |
| T-7-03 | S/T/R/I/D/E | oneshot channel | accept | Channel is local to invoke(); rx dropped on timeout just loses result, handler returns Panic error |
</threat_model>

<verification>
```bash
cargo check 2>&1 | tail -5
cargo test --lib -- --test-threads=4 2>&1 | tail -10
```
</verification>

<success_criteria>
1. Sync handlers execute on Rayon pool when rayon_pool_size > 0
2. PythonHandler::invoke() dispatches to RayonPool::spawn when pool is configured
3. ConsumerConfigBuilder::rayon_pool_size(u32) accepts valid range 1-256
4. Default pool size = num_cpus::get().saturating_sub(2).max(2) when not configured
5. Fallback to spawn_blocking when rayon_pool is None (backward compatible)
</success_criteria>

<output>
After completion, create `.planning/phases/07-thread-pool/07-01-SUMMARY.md`
</output>