# KafPy Technical Concerns

## High-Priority Concerns

### 1. Excessive `unwrap()` Usage (Severity: HIGH)
- **~120 non-test `unwrap()` calls** in production Rust code
- Critical locations: `src/dispatcher/consumer_dispatcher.rs:302`, `src/worker_pool/pool.rs:267,276`, `src/routing/python_router.rs` (multiple)
- `unwrap()` on locks (`Mutex::lock().unwrap()`) can panic under contention or if a thread panics while holding the lock
- **Recommendation**: Replace critical `unwrap()` calls with proper error handling using `?` operator or `match`

### 2. `unsafe impl Send/Sync` on BenchmarkRunner (Severity: MEDIUM)
- File: `src/benchmark/runner.rs:294-295`
- Unconditionally implements `Send` and `Sync` — only safe if all fields are actually thread-safe
- **Recommendation**: Audit field types; add SAFETY comments if sound

### 3. `Python::assume_attached()` Unsafe Calls (Severity: MEDIUM)
- File: `src/pyconsumer.rs:288,333`
- `unsafe { Python::assume_attached() }` bypasses GIL safety checks
- Only safe when called from threads known to hold the GIL
- **Recommendation**: Verify all call sites hold the GIL; add SAFETY comments

### 4. No CI Pipeline (Severity: HIGH)
- `.github/` directory exists but no CI configuration found
- No automated testing, linting, or release pipeline
- **Recommendation**: Add GitHub Actions for `cargo test`, `pytest`, `maturin develop`, clippy

### 5. TODO in Worker Pool (Severity: MEDIUM)
- File: `src/worker_pool/pool.rs:130`
- `// TODO: Streaming workers need StreamConsumer injection via WorkerPool::new`
- Streaming worker injection is incomplete
- **Recommendation**: Complete or document as known limitation

## Medium-Priority Concerns

### 6. Large Files (500+ lines)
| File | Lines | Concern |
|------|-------|---------|
| `src/config.rs` | 822 | PyO3 config with many fields; consider splitting builder from struct |
| `src/python/handler.rs` | 726 | Handler execution logic; could benefit from sub-module extraction |
| `src/benchmark/measurement.rs` | 696 | Measurement with many `Mutex` fields |
| `src/worker_pool/worker.rs` | 661 | Worker loop logic |
| `src/worker_pool/fan_out.rs` | 539 | Fan-out worker with test code inline |
| `src/pyconsumer.rs` | 500 | PyO3 bridge; complex GIL management |

### 7. Dual Logging Configuration in `__init__.py`
- Lines 49-80 duplicate the logging setup (two `StreamHandler` additions possible)
- `if not _logger.hasHandlers()` on line 51 conflicts with `if not _logger.handlers` on line 75
- **Recommendation**: Consolidate to single logging initialization block

### 8. Test Coverage Gaps
- No tests for: `coordinator/`, `offset/`, `dlq/`, `retry/`, `observability/`, `routing/` (Python routing), `middleware/` chain
- Python tests only cover exceptions; no handler, consumer, or config E2E tests
- **Recommendation**: Prioritize `offset/` and `coordinator/` tests as they manage critical offset commit logic

### 9. `benchmark/` Module Embedded in Library (Severity: LOW)
- Benchmark infrastructure ships in the production library via `_kafpy` module
- `run_scenario_py` and `run_hardening_checks_py` are exposed in the PyO3 module
- **Recommendation**: Consider feature-gating behind `["benchmark"]` feature flag to exclude from production builds

### 10. `test.py` at Repository Root
- Ad-hoc integration test script at project root, not in `tests/`
- Likely a local development convenience, not part of any test suite
- **Recommendation**: Move to `tests/` or remove if superseded

## Low-Priority Concerns

### 11. `parking_lot` vs `std::sync` Inconsistency
- Code uses both `parking_lot::Mutex` and `std::sync::Mutex`
- `src/benchmark/measurement.rs` uses `std::sync::Mutex` extensively
- `src/dispatcher/queue_manager.rs` may use `parking_lot`
- **Recommendation**: Standardize on `parking_lot` for performance, or document the choice

### 12. No Documentation Tests
- `src/consumer/mod.rs` doc examples use `.unwrap()` which would fail doc tests
- Doc comments exist but aren't validated
- **Recommendation**: Add `#![deny(rustdoc::broken_intra_doc_links)]` and enable doc tests

### 13. Docker Compose for Development Only
- `docker-compose.yaml` has `KAFKA_OFFSETS_TOPIC_REPLICATION_FACTOR: 1` — not production config
- No auth configured (PLAINTEXT listeners)
- Only useful for local development; not suitable for CI or integration testing without Kafka