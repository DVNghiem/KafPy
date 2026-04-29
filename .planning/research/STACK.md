# Stack Research: Work-Stealing Thread Pool for Blocking Sync Handlers

**Project:** KafPy
**Context:** v1.1 milestone — add non-blocking execution for long-running sync Python handlers
**Researched:** 2026-04-29
**Confidence:** HIGH

## Executive Summary

Long-running sync Python handlers block the Tokio poll cycle, causing heartbeat misses and potential rebalances. The solution is a dedicated Rayon work-stealing thread pool that runs blocking sync work alongside the Tokio async runtime. Tokio continues processing Kafka heartbeats and async events while Rayon handles Python handler execution. No changes to existing Tokio configuration, no migration of message routing to async — just a bounded Rayon pool that receives work from Tokio via oneshot channels and notifies completion back.

---

## Recommended Stack Addition

### Rayon

| Property | Value | Rationale |
|----------|-------|-----------|
| Version | `1.12.0` | Current crates.io latest. Stable API, well-maintained, widely adopted. |
| Purpose | Work-stealing thread pool for blocking sync handlers | NUMA-aware work-stealing with zero-cost fork-join abstraction. Guarantees data-race freedom. |
| Integration | Pool-per-consumer-instance, not global | Clean isolation; each consumer can have independent pool size. |

**Cargo.toml addition:**

```toml
[dependencies]
rayon = "1.12"
```

### Version Verification

| Crate | Current in Cargo.toml | Recommended | Status |
|-------|----------------------|--------------|--------|
| `rayon` | Not present | `1.12.0` | New addition |
| `tokio` | `1.40` | `1.40` or `1.52.1` | Keep existing |
| `pyo3-async-runtimes` | `0.27.0` | `0.28.0` | Upgrade path noted, not blocking |

---

## Integration Architecture

### Data Flow

```
Tokio (async event loop)
  ├── poll loop → message → bounded channel (per handler, already exists)
  │                      └── dispatch work to Rayon via pool.spawn()
  │                      └── Python sync handler runs on Rayon thread
  │                      └── oneshot sender → completion flag
  │                      └── poll oneshot receiver (non-blocking try_recv)
  │                      └── if ready: advance offset, commit if batch-ready
  └── Kafka heartbeat / offset commit / channel ops (continues uninterrupted)
```

**Key property:** Tokio never blocks on sync Python work. The poll cycle stays responsive because blocking work lives exclusively on Rayon threads.

### Why Rayon Over Alternatives

| Option | Verdict | Reason |
|--------|---------|--------|
| `tokio::task::spawn_blocking` | Not recommended | Shared blocking pool; cannot cancel individual long-running tasks; designed for brief CPU work (< 10ms) |
| `std::thread::spawn` manual pool | Not recommended | No work-stealing, no graceful shutdown, significant implementation/maintenance burden |
| `async-std` runtime | Out of scope | Migration would rewrite entire async layer; Tokio already integrated and working |
| `threadpool` crate | Not recommended | Unmaintained, no work-stealing |

Rayon is the correct choice because:
1. Work-stealing handles uneven workloads without manual load balancing
2. `spawn()` returns immediately (fire-and-forget with work-tracking for shutdown)
3. `shutdown_timeout()` enables graceful drain of in-flight handlers
4. No shared state with Tokio — clean separation of concerns

---

## Integration Points with Existing Tokio Stack

### Components Used as-Is

| Component | Role | Integration |
|-----------|------|-------------|
| `tokio::sync::bounded` channel | Per-handler message queue | Already exists; upstream dispatches to this queue |
| `tokio::sync::oneshot` | Completion notification | Send from Rayon thread, receive on Tokio task |
| `Arc<Semaphore>` per handler | Concurrency limiting | Already exists; continues to govern both sync and async |
| `ConsumerConfigBuilder` | Config API | Add `rayon_pool_size(u32)` to builder |
| Prometheus metrics + W3C tracing | Observability | Unchanged |

### No Changes To

- Tokio runtime configuration (`tokio = { version = "1.40", features = ["full"] }`)
- Async handler signatures or dispatch paths
- Bounded channel backpressure behavior
- Message routing architecture
- Offset tracking or commit logic

### New Integration Points

1. **Pool initialization**: `rayon::ThreadPoolBuilder::new().num_threads(n).build()?` at consumer startup
2. **Work dispatch**: `pool.spawn(move || { /* Python handler via PyO3 */ })`
3. **Completion polling**: `oneshot_receiver.try_recv()` on each poll cycle iteration
4. **Graceful shutdown**: `pool.shutdown_timeout(Duration::from_secs(30))` joined with consumer drain sequence

---

## Specific Configuration Recommendations

### Thread Pool Size

```rust
let rayon_threads = std::cmp::max(2, num_cpus::get() - 2);
```

**Rationale:** Tokio needs at least 1-2 threads for its own async operations (Kafka client, channel sends). Over-subscribing the CPU causes context-switching overhead. Rayon's work-stealing handles load imbalance, so exact thread count is less critical than having a sensible default that does not starve Tokio.

**User-configurable:** Add `ConsumerConfigBuilder::rayon_pool_size(u32)` so users can tune for their workload. Default can be `num_cpus::get()` if they want full CPU allocation.

### Handler Result Routing

Sync handler results must route back through the same offset-commit path as async handlers:

```rust
// On Rayon thread: compute result, send via oneshot
oneshot_sender.send(handler_result)?;

// On Tokio task: non-blocking check
if let Ok(result) = oneshot_receiver.try_recv() {
    // advance offset, DLQ routing, retry logic — identical to async path
}
```

This ensures retry/DLQ/backoff logic works identically for both handler types.

### Graceful Shutdown Sequence

```rust
// 1. Consumer stops polling new messages
// 2. Wait for in-flight messages to complete or timeout
pool.shutdown_timeout(std::time::Duration::from_secs(30));
// 3. Drain remaining Tokio tasks (offset commits, metrics flush)
```

Do not simply drop the pool — in-flight sync handlers may be mid-processing (database transaction, HTTP call). The timeout-based drain sets user expectations.

### Memory Considerations

Each Rayon thread holds stack memory. Default stack size is usually 2MB on Linux. A 4-thread pool adds ~8MB resident memory. This is acceptable for a consumer framework. If memory is tight, `ThreadPoolBuilder::stack_size()` can reduce per-thread allocation at the cost of less headroom for deep Python call stacks.

---

## Alternatives Considered

### `tokio::task::spawn_blocking`

**Verdict:** Not recommended for long-running Python handlers.

`spawn_blocking` is designed for short-lived CPU-bound work. Python sync handlers may run for seconds (database calls, HTTP requests, file I/O). Long-running blocking tasks on Tokio's blocking thread pool:
- Consume slots from the shared blocking thread pool (exhaustion causes deadlocks)
- Cannot be individually cancelled (no per-task cancellation token)
- No work-stealing — tasks queue serially on the blocking pool

**Trade-off:** Simpler to integrate (no new dependency), but breaks under long-running handlers and provides no isolation, tunability, or graceful shutdown.

---

### Manual `std::thread::spawn` Pool

**Verdict:** Not recommended.

Rolling your own thread pool requires implementing:
- Lock-free work queue to avoid contention
- Thread lifecycle management (start, stop, join)
- Graceful shutdown with in-flight task completion
- Task cancellation

Rayon provides all of this with battle-tested correctness. The implementation burden and ongoing maintenance risk are not worth zero dependencies.

**Trade-off:** Zero new dependencies at the cost of significant implementation risk.

---

### `crossbeam-channel` for Message Passing

**Verdict:** Not needed.

Existing `tokio::sync::bounded` channels already handle inter-thread communication via their sender side. The message routing from Kafka to handler channels is already working. We only need Rayon for parallel execution, not for replacing the message-passing infrastructure.

**Trade-off:** Adding `crossbeam-channel` would increase dependency surface without solving the actual problem (blocking sync handlers).

---

## What NOT to Add

1. **Do not replace Tokio runtime with Rayon** — Tokio owns async I/O, Kafka polling, and channel operations. Rayon only handles sync Python handler execution.

2. **Do not use `spawn_blocking` for all sync work** — Designed for brief CPU work, not long-running Python handlers. Use the dedicated Rayon pool instead.

3. **Do not make the Rayon pool global** — Pool-per-consumer-instance is cleaner. Global pool prevents independent consumer instances from having different pool sizes.

4. **Do not add `crossbeam-channel` or other queue implementations** — Existing `tokio::sync::bounded` channels handle all message passing. Rayon only provides the execution venue.

5. **Do not add `threadpool` crate (old/unmaintained)** — `threadpool` lacks work-stealing and is unmaintained. Rayon is the standard for work-stealing parallelism in Rust.

6. **Do not change PyO3 binding signatures** — The existing `#[pyfunction]` pattern works for sync handlers on Rayon. No changes needed to the Python-callable interface.

---

## Sources

- [Rayon crate — crates.io](https://crates.io/crates/rayon) — version 1.12.0
- [Rayon documentation — Context7](https://context7.com/rayon-rs/rayon)
- [Tokio spawn_blocking limitations — docs.rs](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html)
- Current `Cargo.toml` — confirmed existing dependency versions