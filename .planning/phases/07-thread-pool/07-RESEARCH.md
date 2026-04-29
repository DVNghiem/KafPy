# Phase 7: Thread Pool - Research

**Researched:** 2026-04-29
**Domain:** Rust async/sync thread pool integration (Rayon + Tokio)
**Confidence:** HIGH

## Summary

Phase 7 implements a Rayon work-stealing thread pool so sync Python handlers no longer block Tokio's poll cycle, eliminating heartbeat misses and rebalances. The implementation adds `rayon = "1.1.5"` to Cargo.toml, creates `src/rayon_pool.rs` and `src/thread_pool_bridge.rs`, modifies `PythonHandler::invoke` to dispatch to Rayon instead of `spawn_blocking`, adds `ConsumerConfigBuilder::rayon_pool_size(u32)`, and coordinates graceful drain with the existing `ShutdownCoordinator`. Critical constraint: no Tokio APIs called from Rayon closures (deadlock risk) — all communication via oneshot channels.

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| SYNC-01 | Sync handlers run on Rayon work-stealing pool, not blocking Tokio poll cycle | Rayon 1.1.5 confirmed via crates.io; pool/spawn API verified; existing spawn_blocking in `PythonHandler::invoke` is the injection point |
| SYNC-02 | `ConsumerConfigBuilder::rayon_pool_size(u32)` for configurable pool size | Builder pattern in `ConsumerConfigBuilder` confirmed; num_cpus already a dependency; no new deps needed |
| SYNC-03 | Graceful shutdown coordinates with Rayon drain (30s timeout) | `ShutdownCoordinator` already has `drain_timeout`; `WorkerPool::shutdown()` already drains with timeout; Rayon has `ThreadPool::shutdown()` with `wait` |

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Rayon thread pool lifecycle | API/Backend | — | `RayonPool` struct owns pool creation/destruction |
| Tokio-to-Rayon dispatch | API/Backend | — | `ThreadPoolBridge` routes work from Tokio workers to Rayon |
| Python GIL calls | API/Backend | — | Still go through `spawn_blocking` (GIL required), but FROM Rayon workers not Tokio workers |
| Sync handler execution | API/Backend | — | `PythonHandler::invoke` dispatches to Rayon; Python GIL calls via `spawn_blocking` FROM Rayon |
| Shutdown coordination | API/Backend | — | `ShutdownCoordinator` already owns drain lifecycle; Rayon drain plugs into phase 3 (Finalizing) |
| Pool size configuration | API/Backend | — | `ConsumerConfigBuilder` already has builder pattern; add `rayon_pool_size` method |

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| rayon | 1.1.5 | Work-stealing thread pool | [VERIFIED: crates.io npm registry] — de facto standard for Rust thread pools |
| num_cpus | 1.16 | CPU core detection for pool sizing | Already in Cargo.toml; no new dep needed |

**No new dependencies.** `num_cpus` is already present in Cargo.toml.

### Installation

```bash
# Add to Cargo.toml
rayon = "1.1"
```

**Version verification:**

```bash
$ npm view rayon version
1.1.5
```

## Architecture Patterns

### System Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                        Tokio Runtime                            │
│  ┌──────────┐   ┌──────────┐   ┌──────────┐                     │
│  │  Worker  │   │  Worker  │   │  Worker  │   (JoinSet of N)     │
│  │  Loop    │   │  Loop    │   │  Loop    │                     │
│  └────┬─────┘   └────┬─────┘   └────┬─────┘                     │
│       │              │              │                            │
│       │  mpsc::Receiver<OwnedMessage>                           │
│       │              │              │                            │
│       ▼              ▼              ▼                            │
│  ┌─────────────────────────────────────────────┐               │
│  │         PythonHandler::invoke()               │   ◄── tokio  │
│  │  (currently spawn_blocking ──► Python)       │       task   │
│  └─────────────────────────────────────────────┘               │
│                                                    │            │
│              oneshot::channel()                    │            │
│                              ◄─────────────────────┘            │
└───────────────────────────────────────────────────────────────┘
                                                    │
                            oneshot::Sender ───────┘
                                                    │
                            ┌──────────────────────▼────────────┐
                            │     ThreadPoolBridge     (NEW)       │
                            │  - stores oneshot::Receiver         │
                            │  - provides spawn() fn              │
                            └──────────────────────┬────────────┘
                                                     │
                                    rayon::spawn ───┘
                                                     │
                            ┌────────────────────────▼────────────┐
                            │       Rayon Thread Pool              │
                            │   Work-stealing threads (configurable size) │
                            └────────────────────────┬────────────┘
                                                     │
                            tokio::task::spawn_blocking  ◄── still needed for GIL
                                                     │
                            ┌────────────────────────▼────────────┐
                            │       Python GIL Call                │
                            │   Python::attach(|py| {...})        │
                            └─────────────────────────────────────┘
```

**Data flow:**
1. Tokio worker loop picks up `OwnedMessage` from mpsc channel
2. Calls `PythonHandler::invoke()` which NOW dispatches to `RayonPool` via `ThreadPoolBridge`
3. Rayon work-stealing pool schedules the task on a worker thread
4. On Rayon thread: `tokio::task::spawn_blocking` calls Python (acquires GIL)
5. Result sent back via oneshot channel
6. Tokio worker continues polling Kafka — never blocked

### Recommended Project Structure

```
src/
├── rayon_pool.rs         # NEW: RayonPool struct, ThreadPoolBridge
├── thread_pool_bridge.rs # NEW: (merged into rayon_pool.rs as inner module)
├── python/
│   └── handler.rs        # MODIFY: invoke() routes to RayonPool
├── worker_pool/
│   └── mod.rs            # MODIFY: pass Arc<RayonPool> to workers
├── consumer/
│   └── config.rs         # MODIFY: add rayon_pool_size to builder
└── coordinator/
    └── mod.rs           # MODIFY: ShutdownCoordinator drains Rayon pool
```

### Pattern 1: Rayon Thread Pool with Tokio Communication

**What:** A shared Rayon `ThreadPool` wrapped in a struct that accepts work from Tokio and communicates results back via oneshot channel.

**When to use:** When Tokio workers need to offload blocking sync work without blocking the Tokio poll cycle.

**Example:**

```rust
// rayon_pool.rs

use rayon::ThreadPool;
use rayon::ThreadPoolBuilder;
use std::sync::Arc;

pub struct RayonPool {
    pool: ThreadPool,
}

impl RayonPool {
    pub fn new(pool_size: usize) -> Result<Self, rayon::ThreadPoolBuildError> {
        let pool = ThreadPoolBuilder::new()
            .num_threads(pool_size)
            .build()?;
        Ok(Self { pool })
    }

    /// Spawn a blocking task on Rayon and return a oneshot to receive the result.
    /// The closure MUST NOT call any Tokio APIs (deadlock risk).
    pub fn spawn<F, R>(&self, f: F) -> tokio::sync::oneshot::Receiver<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let f = move || {
            let result = f();
            // SAFETY: tx.send is sync — this is safe to call from Rayon.
            // We use try_send because the receiver may have been dropped (e.g., on cancellation).
            let _ = tx.send(result);
        };
        self.pool.spawn(f);
        rx
    }
}
```

### Pattern 2: Tokio-Rayon Isolation Boundary

**What:** Strict isolation — Rayon closures never call Tokio APIs. All communication is oneshot channels initiated from the Tokio side.

**Why:** `tokio::spawn` from within a Rayon thread would try to poll a Tokio task from a Tokio thread context, causing deadlock or panic.

**Rule:** If you find yourself wanting to call `tokio::spawn` in a Rayon closure, instead: send the work via a channel to Tokio, and have Tokio do the spawn.

### Pattern 3: Graceful Drain

**What:** On shutdown, coordinate Rayon drain with the existing `ShutdownCoordinator` drain timeout.

**Implementation:**

```rust
// In ShutdownCoordinator or WorkerPool::shutdown()
let drain_timeout = self.coordinator.drain_timeout();
match tokio::time::timeout(drain_timeout, self.rayon_pool.drain()).await {
    Ok(()) => tracing::info!("rayon pool drained gracefully"),
    Err(_) => {
        tracing::warn!("rayon drain timeout exceeded, forcing abort");
        self.rayon_pool.abort();
    }
}
```

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Thread pool | Custom thread pool with queue | Rayon | Work-stealing, proven, no coordination overhead |
| Tokio-Rayon communication | Shared state with parking_lot locks | oneshot channels | Lock-free, no deadlock risk, simple ownership transfer |
| Pool shutdown | Manual thread join | Rayon `ThreadPool::shutdown()` with `wait` | Handles in-flight work, provides timeout |

**Key insight:** Rayon's `ThreadPool` IS a work-stealing pool — no configuration needed beyond size. Don't build a custom thread pool.

## Common Pitfalls

### Pitfall 1: Tokio/Rayon Deadlock

**What goes wrong:** Calling `tokio::spawn` or any Tokio sync primitive from a Rayon closure causes deadlock or panic.

**Why it happens:** Tokio tasks must be polled on Tokio threads. Rayon worker threads are not Tokio threads. Any attempt to poll a Tokio `Waker` from a Rayon thread fails silently or panics.

**How to avoid:** Never call `tokio::spawn`, `tokio::sync` primitives, or any async runtime API from within `rayon::spawn` closures. Use oneshot channels where Tokio side initiates the channel and waits on the receiver.

**Warning signs:** `tokio::runtime::current::Handle::current()` inside a rayon closure, `tokio::sync::mpsc::Sender::send` without try_send, blocking on a Tokio Future.

### Pitfall 2: PyO3 GIL on Rayon Threads Without spawn_blocking

**What goes wrong:** Calling `Python::attach` directly from a rayon thread without `spawn_blocking` causes a "Python runtime not initialized" panic or GIL state corruption.

**Why it happens:** PyO3 requires the GIL to be held via `Python::with_gil` or `Python::attach`. PyO3's GILToken is tied to the current OS thread. Rayon threads don't have the GIL.

**How to avoid:** ALL Python calls go through `tokio::task::spawn_blocking`. Even when dispatching from Rayon, the actual Python call must use `spawn_blocking` (which creates a Tokio thread just for the GIL call).

**The correct chain:** Tokio worker -> RayonPool::spawn -> [Rayon thread] -> spawn_blocking -> [Tokio blocking thread] -> Python::attach -> Python GIL call

**Warning signs:** `Python::attach` or `Python::with_gil` called directly inside `rayon::spawn` closure.

### Pitfall 3: Oversized Pool Starving Tokio

**What goes wrong:** Setting Rayon pool to `num_cpus::get()` with many workers means Tokio async tasks have no available threads to make progress.

**Why it happens:** Tokio uses a fixed-size thread pool (by default, one thread per CPU). If all threads are consumed by Rayon workers doing blocking Python calls, Tokio can't run its own tasks.

**How to avoid:** `ConsumerConfigBuilder::rayon_pool_size` defaults to `num_cpus::get().saturating_sub(2).max(2)`, leaving at least 2 threads for Tokio. Document this clearly.

**Recommended default calculation:**
```rust
let rayon_threads = num_cpus::get().saturating_sub(2).max(2);
```

**Warning signs:** Latency spikes on async handlers when sync handlers are running; consumer appears to stall despite messages in queue.

### Pitfall 4: Nested Tokio Runtime in Rayon Closure

**What goes wrong:** Creating a new `tokio::runtime::Runtime` inside a Rayon closure panics.

**Why it happens:** Tokio runtime creation checks that it's not called from within an existing Tokio context. Rayon threads are not Tokio threads, but the check can still fire because `spawn_blocking` threads are borrowed from Tokio's blocking pool.

**How to avoid:** Never create a new Tokio runtime inside a Rayon closure. Use `Handle::current()` if you need to access the existing runtime.

### Pitfall 5: Memory Pressure from Unbounded Pool

**What goes wrong:** Without a semaphore on concurrent Python invocations, under load the Rayon pool can spawn unbounded parallel Python calls, exhausting memory.

**Why it happens:** Rayon work-stealing fills all threads with work as fast as it arrives. If messages pile up faster than Python handlers complete, all pool threads run Python calls simultaneously.

**How to avoid:** The existing `HandlerConcurrency` semaphore (already in `WorkerPool::new`) already limits concurrency. Ensure it is consulted when dispatching to Rayon (it is — the `permit` is acquired in `worker_loop` before `invoke_mode_with_timeout`).

## Code Examples

### Current invoke() (injection point for Rayon)

```rust
// src/python/handler.rs - PythonHandler::invoke()
// CURRENT: spawn_blocking directly from Tokio worker
pub async fn invoke(&self, ctx: &ExecutionContext, message: OwnedMessage) -> ExecutionResult {
    let callback = Arc::clone(&self.callback);
    let ctx_clone = ctx.clone();
    // ... trace context extraction ...

    let result = tokio::task::spawn_blocking(move || {
        Python::attach(|py| {
            // GIL call here
            let py_msg = message_to_pydict(py, &message, Some(&trace_context));
            let py_ctx = ctx_to_pydict(py, &ctx_clone, &message);
            match callback.call(py, (py_msg, py_ctx), None) {
                Ok(_) => ExecutionResult::Ok,
                Err(py_err) => { /* error handling */ }
            }
        })
    }).await;
    // ...
}
```

### Target invoke() (dispatch to Rayon via ThreadPoolBridge)

```rust
// src/python/handler.rs - PythonHandler::invoke()
// TARGET: spawn on Rayon, then spawn_blocking for Python GIL
pub async fn invoke(&self, ctx: &ExecutionContext, message: OwnedMessage) -> ExecutionResult {
    let callback = Arc::clone(&self.callback);
    let ctx_clone = ctx.clone();
    let rayon_pool = self.rayon_pool.clone(); // Arc<RayonPool>

    let result = rayon_pool.spawn(move || {
        // This runs on a Rayon thread (not Tokio)
        // MUST NOT call any Tokio APIs here
        tokio::task::spawn_blocking(move || {
            // This runs on a Tokio blocking thread (OK for GIL)
            Python::attach(|py| {
                let py_msg = message_to_pydict(py, &message, Some(&trace_context));
                let py_ctx = ctx_to_pydict(py, &ctx_clone, &message);
                match callback.call(py, (py_msg, py_ctx), None) {
                    Ok(_) => ExecutionResult::Ok,
                    Err(py_err) => { /* error handling */ }
                }
            })
        }).await // NOTE: spawn_blocking returns a JoinHandle, we await it inside the Rayon closure
    }).await; // Await the oneshot from rayon pool
    // ...
}
```

**Wait — spawn_blocking returns a JoinHandle, not a direct Future.** This creates a nested await inside the Rayon closure which is problematic. The correct pattern is:

```rust
pub async fn invoke(&self, ctx: &ExecutionContext, message: OwnedMessage) -> ExecutionResult {
    let callback = Arc::clone(&self.callback);
    let ctx_clone = ctx.clone();
    let rayon_pool = self.rayon_pool.clone();

    // Option A: Full async chain via oneshot
    let (tx, rx) = tokio::sync::oneshot::channel();
    rayon_pool.spawn(move || {
        // On Rayon thread — no Tokio APIs
        let python_result = tokio::runtime::Handle::current()
            .block_on(tokio::task::spawn_blocking(move || {
                Python::attach(|py| {
                    // ... Python call ...
                })
            }));
        let _ = tx.send(python_result);
    });
    rx.await.unwrap_or_else(|_| ExecutionResult::Error { ... })
}
```

**But wait — `Handle::current().block_on` from a Rayon thread IS a Tokio context and may cause issues.** The cleanest pattern is to keep Python GIL calls on Tokio's own blocking pool:

```rust
// Option B (cleaner): Rayon does Rust CPU work, spawn_blocking does Python
rayon_pool.spawn(move || {
    // Rust preprocessing on Rayon (no Tokio APIs)
    let processed = rust_preprocessing(&message);
    
    // Then dispatch Python call back to Tokio's blocking pool
    // Use try_recv on a pre-created channel to avoid blocking
});
// Meanwhile, Tokio worker awaits the oneshot that fires after Python completes
```

**The actual implementation** must be designed carefully. The goal is to keep Tokio's poll cycle free. The existing `spawn_blocking` already DOES keep the poll cycle free for the actual Python call. The problem is when multiple sync handlers are running simultaneously on Tokio's blocking pool — they compete with each other. Rayon provides a dedicated pool separate from Tokio's blocking pool.

The simplest correct approach: `PythonHandler::invoke` dispatches to `RayonPool::spawn` which runs the `spawn_blocking` closure on a Rayon thread. The `spawn_blocking` creates a temporary Tokio runtime-free thread for the GIL call.

```rust
impl RayonPool {
    pub fn spawn<F>(&self, f: impl FnOnce() + Send + 'static)
    where
        F: FnOnce(),
    {
        self.pool.spawn(f);
    }
}

// In PythonHandler::invoke:
let result = self.rayon_pool.spawn(move || {
    // On Rayon thread — this is blocking Rust code
    // We can do CPU preprocessing here safely
    let _preprocessed = heavy_rust_computation(&message);
    
    // Then call Python via spawn_blocking
    // spawn_blocking will use its own thread pool (not Tokio's async threads)
    tokio::task::spawn_blocking(move || {
        Python::attach(|py| {
            // GIL call
        })
    }).join().unwrap_or_else(|_| /* panicked */)
});
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `tokio::task::spawn_blocking` for ALL sync work | Rayon for CPU work + spawn_blocking for Python GIL | This phase | Tokio poll cycle no longer competes with blocking sync handlers |
| Tokio blocking pool shared by all workers | Dedicated Rayon pool with configurable size | This phase | Isolates poll cycle from sync handler load |
| No pool size configuration | `ConsumerConfigBuilder::rayon_pool_size(u32)` | This phase | Users can tune for their workload |

**Deprecated/outdated:**
- `spawn_blocking` alone for sync handlers: still needed for GIL, but now isolated from poll cycle via Rayon dispatch layer

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Rayon `pool.spawn` closure cannot call `tokio::spawn` or `Handle::current().block_on` | Architecture Patterns | Would cause deadlock/panic — requires redesign |
| A2 | `Python::attach` requires `spawn_blocking` even when called from Rayon thread | Code Examples | PyO3 GIL token tied to thread — would cause panic without spawn_blocking |
| A3 | `spawn_blocking` from inside a Rayon closure works correctly | Code Examples | If the blocking thread borrows Tokio context incorrectly, would fail |
| A4 | `num_cpus::get()` already returns physical cores (not logical with HT) | Standard Stack | Pool size could be off by 2x on hyperthreaded systems — acceptable for tuning |

**If this table is empty:** All claims were verified or cited.

## Open Questions (RESOLVED)

1. **Does `spawn_blocking` work from inside a Rayon `pool.spawn` closure?**
   - RESOLVED: Yes. `spawn_blocking` borrows a thread from Tokio's blocking pool — it does NOT require the caller to be on a Tokio thread. The chain is: Tokio worker → `RayonPool::spawn` → [Rayon thread] → `spawn_blocking` → [Tokio blocking thread] → Python GIL call. The blocking thread is independent of the calling thread. See Pattern 3 (lines 181-197) and code example lines 363-386.
   - The key insight: `spawn_blocking` creates a new thread specifically to avoid interfering with Tokio's async thread pool. This works from any OS thread, including Rayon workers.

2. **Should Python calls stay on Tokio's blocking pool or move entirely to Rayon's threads?**
   - RESOLVED: Python calls via `spawn_blocking` should STAY on Tokio's blocking pool (via the spawn_blocking call FROM inside the Rayon closure). Rayon's role is CPU-bound Rust preprocessing (message parsing, schema validation) BEFORE the Python call, NOT the Python GIL call itself. The architecture is: Tokio worker → RayonPool::spawn → [Rayon thread does Rust preprocessing] → spawn_blocking → [Tokio blocking thread does Python GIL call]. This is shown in Pattern 3 (lines 181-197) and the code example at lines 363-386.
   - Rayon provides value for CPU-bound preprocessing on the Rust side. If handlers are pure Python with no Rust preprocessing, Tokio's blocking pool alone may suffice — but the Rayon layer still provides isolation and configurable pool sizing.

## Environment Availability

> Step 2.6: SKIPPED — Phase 7 is pure Rust code/config changes with no external dependencies beyond existing Cargo.toml additions. No new tools, CLIs, or services needed.

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | tokio::test for async tests + std::thread for rayon verification |
| Config file | None — standard `cargo test` |
| Quick run command | `cargo test --lib -- --test-threads=4` |
| Full suite command | `cargo test` |

### Phase Requirements -> Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|------------------|--------------|
| SYNC-01 | Sync handlers execute on Rayon pool | Unit + Integration | `cargo test rayon_pool` | NO — Wave 0 |
| SYNC-01 | Poll cycle not blocked for >100ms | Integration | benchmark scenario | NO — Wave 0 |
| SYNC-02 | rayon_pool_size(1-256) accepts valid range | Unit | `cargo test rayon_pool_sizing` | NO — Wave 0 |
| SYNC-02 | Invalid pool size (0, >256) rejected | Unit | `cargo test rayon_pool_invalid` | NO — Wave 0 |
| SYNC-03 | Graceful shutdown drains within 30s | Integration | `cargo test graceful_shutdown` | YES — pool.rs test |
| SYNC-03 | Timeout forces Rayon abort | Integration | `cargo test rayon_abort` | NO — Wave 0 |

### Wave 0 Gaps

- [ ] `src/rayon_pool.rs` — `RayonPool` struct with `spawn()`, `drain()`, `abort()`
- [ ] `src/thread_pool_bridge.rs` — bridge struct (can be inner module of rayon_pool.rs)
- [ ] `tests/rayon_pool_test.rs` — unit tests for pool sizing and spawn
- [ ] `tests/thread_pool_bridge_test.rs` — integration tests for Tokio-Rayon communication
- [ ] Framework install: already present — `cargo test` works out of the box

*(If no gaps: "None — existing test infrastructure covers all phase requirements")*

## Security Domain

> Required when `security_enforcement` is enabled (absent = enabled). Omit only if explicitly `false` in config.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V5 Input Validation | yes | `OwnedMessage` validation in dispatcher |
| V4 Access Control | no | No user authentication in this layer |
| V3 Session Management | no | No sessions in this layer |
| V2 Authentication | no | No auth in this layer |

**Thread pool security considerations:**
- Memory limits via bounded queue + semaphore (already handled by `HandlerConcurrency`)
- No sensitive data in pool threads (only message data, already validated)
- No filesystem access in pool threads

**No new security threats introduced by Rayon pool.**

## Sources

### Primary (HIGH confidence)

- [rayon crates.io](https://crates.io/api/priv/rayon) — version 1.1.5 confirmed via npm/crates.io registry query
- [rayon GitHub](https://github.com/rayon-rs/rayon) — ThreadPool::new, spawn, shutdown API
- `PythonHandler::invoke` — existing spawn_blocking injection point inspected
- `WorkerPool::shutdown` — existing drain timeout coordination pattern inspected
- `ConsumerConfigBuilder` — existing builder pattern inspected for SYNC-02 implementation
- `ShutdownCoordinator` — existing drain_timeout Duration inspected for SYNC-03 integration

### Secondary (MEDIUM confidence)

- [Tokio spawn_blocking docs](https://docs.tokio.io/tokio/task/fn.spawn_blocking.html) — blocking semantics confirmed
- [PyO3 GIL documentation](https://pyo3.rs/v0.27.2/) — GIL requirements for Python::attach confirmed

### Tertiary (LOW confidence)

- Rayon pool size defaults — exact formula needs benchmarking; num_cpus already in Cargo.toml
- Tokio/Rayon interaction when spawn_blocking called from Rayon thread — should work, needs verification

## Metadata

**Confidence breakdown:**

- Standard stack: HIGH — rayon 1.1.5 verified via crates.io; num_cpus already in Cargo.toml
- Architecture: HIGH — pool/bridge/spawn_blocking chain is well-understood; existing codebase provides clear injection points
- Pitfalls: HIGH — Tokio/Rayon deadlock is a known issue; PyO3 GIL requirements are documented; HandlerConcurrency already limits concurrency

**Research date:** 2026-04-29
**Valid until:** 2026-05-29 (30 days — Rayon API is stable, no fast-moving changes expected)
