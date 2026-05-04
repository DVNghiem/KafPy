# KafPy Tech Stack

## Core Language & Runtime
- **Rust** (2021 edition) — Core consumer engine, PyO3 bindings
- **Python** ≥3.11 — Public API surface, handler registration, configuration

## Build System
- **maturin** ≥1.0 — PyO3 build system (builds Rust `_kafpy` extension module)
- **Cargo** — Rust package manager and build tool
- Crate name: `KafPy`, lib name: `_kafpy`

## Key Rust Dependencies
| Crate | Version | Purpose |
|-------|---------|---------|
| `pyo3` | 0.27.2 | Python ↔ Rust bridge (extension-module, generate-import-lib) |
| `pyo3-async-runtimes` | 0.27.0 | Async PyO3 + Tokio integration |
| `tokio` | 1.40 (full) | Async runtime for consumer loop |
| `tokio-util` | 0.7.17 | Async stream utilities |
| `rdkafka` | 0.38 | Kafka client (librdkafka bindings) |
| `rayon` | 1.1 | Parallelism for batch processing |
| `serde` / `serde_json` | 1.0 | Serialization |
| `tracing` | 0.1 | Structured logging (span context only) |
| `thiserror` | 2.0.17 | Error derive macros |
| `anyhow` | 1.0 | Error handling |
| `parking_lot` | 0.12 | Fast mutex/Rwlock |
| `metrics` | 0.24 | Metrics facade |
| `prometheus-client` | 0.24 | Prometheus exposition |
| `regex` | 1.0 | Topic pattern routing |
| `glob` | 0.3 | Glob pattern matching |
| `chrono` | 0.4 | Timestamp handling |
| `tdigest` | 0.2 | Percentile estimation |
| `rand` | 0.8 | Jitter for retry backoff |
| `dotenvy` | 0.15.7 | .env file loading |
| `async-stream` | 0.3 | Async stream macros |
| `futures-util` | 0.3 | Stream combinators |
| `tokio-stream` | 0.1 (sync) | Tokio stream adapters |

## Python Dependencies
- None declared in `pyproject.toml` (zero runtime Python dependencies)
- `pytest` used for testing (dev dependency)

## Infrastructure
- **Docker Compose** — 3-node KRaft Kafka cluster + Kafka UI
- Ports: localhost:19092, 29092, 39092 (Kafka); 8080 (Kafka UI)

## Testing
- Rust: `#[cfg(test)]` unit tests within source files + `tests/` integration tests
- Python: `pytest` in `tests/test_exceptions.py`
- No CI configuration found in `.github/`

## Documentation
- **mkdocs** — Project documentation (mkdocs.yml present)
- Docs in `docs/` directory covering API, architecture, guides