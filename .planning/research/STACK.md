# Rust + Python Kafka Consumer Framework: Standard Stack Research

**Date**: 2026-04-28
**Purpose**: Foundation architecture for KafPy — a hybrid Rust/Python Kafka consumer framework where Rust owns the runtime (Kafka ingestion, Tokio concurrency, bounded channels, backpressure) and Python owns business logic (PyO3 bindings, `@handler` decorator API).

---

## 1. Key Crate Versions (Current Stable)

| Crate | Latest Stable | MSRV | Confidence |
|-------|--------------|------|------------|
| `rdkafka` | **0.39.0** (2026-01-25) | Rust 1.70+ | HIGH |
| `tokio` | **1.52.1** (2026-04-16) | Rust 1.71+ | HIGH |
| `pyo3` | **0.28.3** (2026-04-02) | Rust 1.83+ | HIGH |
| `pyo3-async-runtimes` | **0.28.0** (2026-02-03) | Rust 1.83+ | HIGH |
| `tracing` | **0.1.44** (2025-12-18) | Rust 1.65+ | HIGH |
| `tracing-opentelemetry` | **0.29.0** (2026-03) | Rust 1.75+ | HIGH |
| `opentelemetry-sdk` | **0.31.0** (2025-09-25) | Rust 1.75+ | MEDIUM |
| `rskafka` | **0.6.0** (2025-03-20) | Rust 1.85+ | MEDIUM |

### Why These Versions

**rdkafka 0.39.0**: The battle-tested choice. Built on librdkafka (C), supports all Kafka features (consumer groups, transactions, SASL, SSL, compression). The README still shows `0.25` in docs but crates.io shows `0.39.0`. Compatible with librdkafka v1.9.2+. **Confirmed via docs.rs changelog and GitHub releases**.

**tokio 1.52.1**: The de facto standard async runtime. LTS releases available (1.47.x until Sep 2026, 1.51.x until Mar 2027). Multi-threaded runtime with work-stealing is the default and correct choice for Kafka ingestion. **Confirmed via GitHub releases tokio-1.52.1 (2026-04-16)**.

**pyo3 0.28.3**: The standard Rust-Python binding crate. MSRV 1.83 as of 0.28.x. Supports `async fn` in `#[pyfunction]` with `experimental-async` feature. GIL API renamed: `Python::with_gil` → `Python::attach`, `Python::allow_threads` → `Python::detach`. **Confirmed via GitHub v0.28.3 (2026-04-02)**.

**pyo3-async-runtimes 0.28.0**: Bridges Python asyncio and Rust async runtimes (Tokio, async-std). Required for proper `#[pyfunction] async fn` → Python `asyncio` interoperability. **Confirmed via GitHub**.

**tracing 0.1.44**: The standard instrumentation crate from the Tokio project. MSRV 1.65. Works with `tracing-subscriber` for output formatting and `tracing-opentelemetry` for distributed tracing. **Confirmed via crates.io**.

**opentelemetry-rust 0.29.0**: Logs-SDK and Logs-Appender-Tracing are now stable in 0.29.0. Prometheus exporter is **deprecated** in favor of OTLP exporter. **Confirmed via GitHub opentelemetry-0.29.0 release**.

---

## 2. Async Runtime Patterns for High-Throughput Kafka Consumption

### Recommended Pattern: Dedicated Kafka Thread + Bounded MPSC Channel

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Tokio Multi-Thread Runtime                       │
│                                                                      │
│  ┌──────────────────┐         ┌──────────────────────────────────┐ │
│  │  Kafka Fetch Loop │──mpsc──▶│  Worker Pool (N tasks)           │ │
│  │  (spawn_blocking) │ channel │  ┌──────┐ ┌──────┐ ┌──────┐     │ │
│  │                   │ bounded │  │ Task │ │ Task │ │ Task │     │ │
│  │  [rdkafka consumer] │ (CHAN- │  │  0   │ │  1   │ │  2   │     │ │
│  │                   │ CAPACITY)│  └──────┘ └──────┘ └──────┘     │ │
│  └──────────────────┘         │        ↓ GIL release              │ │
│                                │  [Python handler via PyO3]        │ │
│                                └──────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────┘
```

**Why this pattern**:
- **Bounded channel = backpressure**: When `CHANNEL_CAPACITY` is full, the fetch loop blocks naturally, slowing Kafka consumption without unbounded memory growth. This is the core insight from production Kafka consumer patterns.
- **Dedicated thread for Kafka**: rdkafka's consumer loop is blocking by nature. Running it in `spawn_blocking` (Tokio's blocking thread pool) avoids blocking the async worker threads.
- **Worker pool with pre-spawned tasks**: Instead of spawning tasks per message (spawn storms), pre-spawn a fixed number of workers that pull from the shared channel. Overhead stays constant regardless of throughput.

**Key configurations from research**:
- `CHANNEL_CAPACITY`: 100–1000 based on memory budget and processing latency
- `WORKER_COUNT`: `num_cpus::get()` or benchmark-tuned (typically 4–16)
- Use `tokio::sync::mpsc::channel::<Result<KafkaMessage>>` (bounded)

**Reference**: The pattern is validated across multiple blog posts (OneUptime, 2026-01-25) and the official `rust-rdkafka` `asynchronous_processing.rs` example.

### Tokio Runtime Configuration

```rust
#[tokio::main(flavor = "multi_thread", worker_threads = N)]
async fn main() {
    // worker_threads: match to partition count or benchmark
}
```

**For extreme throughput**: Consider `Builder::enable_eager_driver_handoff` (added in 1.52.0 unstable) for faster I/O driver handoff before polling tasks.

---

## 3. PyO3 Python Bindings with Async Rust

### Recommended Approach: `pyo3-async-runtimes` + `spawn_blocking`

**The critical insight from GitHub issue #58 (pyo3-async-runtimes)**:

> Calling `Python::attach` inside a `tokio::spawn` task can freeze non-Python-related tasks because Tokio does not allow work-stealing while a task is blocked on GIL.

**Correct pattern**:
1. Python handlers should be **synchronous** (`#[pyfunction]`) or, if async, **leaf async** (not calling back into Tokio from Python)
2. Heavy Python work should run in **`spawn_blocking`** (dedicated blocking thread pool, not the async worker threads)
3. For `async fn` Python functions, use `pyo3-async-runtimes` which handles the GIL correctly:
   - In 0.27.0+, `future_into_py` finalizes futures **without holding GIL** inside the async runtime
   - The `Runtime` trait now requires `spawn_blocking` function

**Anti-pattern to avoid**: Calling `Python::attach` inside `tokio::spawn` for heavy Python code. This blocks the Tokio event loop thread, starving other tasks.

**Correct async PyO3 usage**:
```rust
#[pyfunction]
async fn process_message(msg: String) -> PyResult<String> {
    // Python::attach is called by pyo3-async-runtimes automatically
    // but the GIL is released during await points
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    Ok(format!("processed: {}", msg))
}
```

**For the `@handler` decorator pattern**: Python registers handlers via a PyO3-exposed function. Each handler call should be:
- Fast-path: Release GIL via `Python::detach` for CPU-bound Rust work
- Slow-path: Use `spawn_blocking` when calling Python business logic

---

## 4. Logging/Tracing Crates

### Recommended Stack

| Layer | Crate | Version | Purpose |
|-------|-------|---------|---------|
| Core instrumentation | `tracing` | 0.1.44 | Span/event creation |
| Output formatting | `tracing-subscriber` + `FmtSubscriber` | latest | Console output |
| Distributed tracing | `tracing-opentelemetry` | 0.29.0 | OTLP export |
| OpenTelemetry SDK | `opentelemetry-sdk` | 0.31.0 | Collector pipeline |
| OTLP exporter | `opentelemetry-otlp` | 0.31.1 | Protocol encoding |

### Standard Setup Pattern

```rust
use tracing_subscriber::{layer::SubscriberExt, Registry, FmtSubscriber};
use tracing_opentelemetry::OpenTelemetryLayer;

let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);
let subscriber = Registry::default()
    .with(FmtSubscriber::builder().with_max_level(Level::INFO).finish())
    .with(telemetry);

tracing::subscriber::set_global_default(subscriber).expect("failed");
```

**Why `tracing` over `log`/`env_logger`**: `tracing` has first-class span semantics — you can correlate log events to specific execution phases. For Kafka consumers processing millions of messages, span-based tracing is essential for distributed observability.

**Why `tracing-opentelemetry` over raw OpenTelemetry**: Integrates directly with `tracing`'s span model rather than requiring separate instrumentation.

**Prometheus exporter deprecated**: Use OTLP exporter and an OpenTelemetry Collector + Prometheus backend instead.

---

## 5. NOT Recommended and Why

### Kafka Client Alternatives

| Crate | Why NOT Recommended |
|-------|---------------------|
| `rskafka` | **Not a general-purpose Kafka client**. Explicitly designed for "simple workloads" and "low number of high-throughput partitions". No consumer groups, no offset tracking, no transactions, no built-in buffering/aggregation/linger timeouts. OK for InfluxDB-style use cases, but unsuitable for a general framework. MSRV 1.85 is also a burden. |

### Anti-Patterns

| Anti-Pattern | Why NOT Recommended |
|--------------|---------------------|
| **Unbounded channels** | Will cause OOM under load. Always use bounded `mpsc::channel` for backpressure. |
| **`tokio::spawn` per message** | Spawn storms create unbounded coordination overhead. Use a fixed worker pool. |
| **Calling `Python::attach` (GIL acquire) inside `tokio::spawn`** | GIL blocking stalls the Tokio event loop, affecting all tasks on that thread. Use `spawn_blocking` for Python calls. |
| **`async-std` instead of Tokio** | Tokio is the dominant runtime with better ecosystem support for Kafka (rdkafka tokio integration), observability (tracing is Tokio project), and community size. |
| **`log` + `env_logger`** | Old logging API with no span semantics. `tracing` provides structured, span-aware logging with ecosystem support. |
| **Raw `pthread` threads for Kafka** | Using `spawn_blocking` integrates with Tokio's blocking thread pool and enables graceful shutdown. Raw threads bypass this. |
| **`tracing` 0.2.x (pre-release)** | Pre-release documentation shows MSRV 1.65+. The stable ecosystem is 0.1.x. Stay on 0.1.x until 0.2 is stable. |

### GIL + Tokio Gotcha (Critical for PyO3)

The **most important** PyO3+Tokio insight from research:

> `Python::attach` (GIL acquire) called inside `tokio::spawn` can block all tasks on that Tokio worker thread. Tokio does not allow work-stealing while a task is blocked on GIL.

**Fix**: Use `spawn_blocking` (or `#[pyfunction] async fn` via `pyo3-async-runtimes`) to ensure Python code runs on the blocking thread pool, not the async worker threads. This is a fundamental architectural constraint.

---

## 6. Confidence Summary

| Category | Recommendation | Confidence |
|----------|---------------|------------|
| Kafka client | `rdkafka` 0.39.0 | **HIGH** |
| Async runtime | `tokio` 1.52.1 | **HIGH** |
| Python bindings | `pyo3` 0.28.3 | **HIGH** |
| Async bridge | `pyo3-async-runtimes` 0.28.0 | **HIGH** |
| Observability | `tracing` 0.1.44 + `tracing-opentelemetry` 0.29.0 | **HIGH** |
| Worker pool pattern | Bounded mpsc + fixed workers | **HIGH** |
| PyO3+Tokio GIL | Use `spawn_blocking` for Python calls | **HIGH** |
| rskafka vs rdkafka | Use rdkafka (rskafka too limited) | **HIGH** |
| OpenTelemetry SDK | `opentelemetry-sdk` 0.31.0 | **MEDIUM** (version overlap with tracing-otel) |
| Logging over log | `tracing` over `log`/`env_logger` | **HIGH** |

---

## 7. Example Cargo.toml Dependencies

```toml
[dependencies]
# Kafka
rdkafka = { version = "0.39", features = ["cmake-build", "ssl"] }

# Async runtime
tokio = { version = "1.52", features = ["full"] }

# Python bindings
pyo3 = { version = "0.28", features = ["auto-initialize"] }
pyo3-async-runtimes = "0.28"

# Observability
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-opentelemetry = "0.29"
opentelemetry-sdk = { version = "0.31", features = ["rt-tokio"] }
opentelemetry-otlp = "0.31"

# Utilities
thiserror = "2"
anyhow = "1"
```

---

## 8. Sources

- [rdkafka GitHub](https://github.com/fede1024/rust-rdkafka) — changelog, README
- [tokio GitHub releases](https://github.com/tokio-rs/tokio/releases) — tokio-1.52.1, tokio-1.52.0
- [pyo3 GitHub](https://github.com/pyo3/pyo3) — v0.28.3, v0.28.0 releases
- [pyo3-async-runtimes GitHub](https://github.com/PyO3/pyo3-async-runtimes) — v0.28.0, issue #58
- [tracing crates.io](https://crates.io/crates/tracing/versions) — v0.1.44
- [tracing-opentelemetry docs.rs](https://docs.rs/tracing-opentelemetry/latest) — v0.29.0
- [opentelemetry-rust releases](https://github.com/open-telemetry/opentelemetry-rust/releases) — opentelemetry-0.29.0
- [rskafka docs.rs](https://docs.rs/rskafka) — v0.6.0, limitations
- [OneUptime: Kafka Consumers with Backpressure in Rust](https://oneuptime.com/blog/post/2026-01-25-kafka-consumers-backpressure-rust/view) — production pattern validation
- [Rust Forum: High performance kafka consumer](https://users.rust-lang.org/t/high-performance-kafka-consumer-service/53161) — spawn storms warning
