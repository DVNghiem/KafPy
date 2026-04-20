# Roadmap: KafPy

## Milestones

- [x] **v1.0 Core Consumer Refactor** — Phases 1-5 (shipped 2026-04-15)
- [x] **v1.1 Dispatcher Layer** — Phases 6-8 (shipped 2026-04-16)
- [x] **v1.2 Python Execution Lane** — Phases 9-10 (shipped 2026-04-16)
- [x] **v1.3 Offset Commit Coordinator** — Phases 11-16 (shipped 2026-04-17)
- [x] **v1.4 Failure Handling & DLQ** — Phases 17-20 (shipped 2026-04-17)
- [x] **v1.5 Extensible Routing** — Phases 21-23 (shipped 2026-04-18)
- [x] **v1.6 Execution Modes** — Phases 24-27 (shipped 2026-04-18)
- [x] **v1.7 Observability Layer** — Phases 28-32 (shipped 2026-04-18)
- [ ] **v1.8 Public API Foundation** — Phases 33-37 (in progress)
- [ ] **v2.0 Code Quality Refactor** — Phases 01-06 (planned)

---

## Phase Details

### Phase 33: Public API Conventions
**Goal**: Establish PascalCase naming, snake_case methods, type hints, private Rust internals, and `__all__` definitions across all public Python modules.
**Depends on**: Nothing (first phase of milestone)
**Requirements**: API-01, API-02, API-03, API-04, API-05
**Success Criteria** (what must be TRUE):
  1. All public Python classes use PascalCase; all methods use snake_case with type hints
  2. No global mutable state exists in the public API — all runtime state in instance objects
  3. Rust internals are private; only explicitly marked `pub` items are accessible
  4. Every public Python module defines `__all__` listing only intended public names
  5. No underscore-prefixed names in `__all__`, no raw Rust struct names exposed
**Plans**: 2 plans
- [ ] 33-01-PLAN.md — Python API surface audit and __all__ definitions
- [ ] 33-02-PLAN.md — Rust pub boundary audit

### Phase 34: Configuration Model
**Goal**: Python-side configuration objects with immutable builders for ConsumerConfig, RoutingConfig, RetryConfig, BatchConfig, and ConcurrencyConfig.
**Depends on**: Phase 33
**Requirements**: CFG-01, CFG-02, CFG-03, CFG-04, CFG-05, CFG-06, CFG-07
**Success Criteria** (what must be TRUE):
  1. ConsumerConfig accepts bootstrap_servers, group_id, topics, auto_offset_reset, enable_auto_commit and is frozen after construction
  2. RoutingConfig accepts routing_mode and fallback_handler; BatchConfig accepts max_batch_size and max_batch_timeout_ms
  3. RetryConfig and ConcurrencyConfig are immutable with max_attempts/base_delay/max_delay/jitter_factor and num_workers respectively
  4. ConsumerConfig is passed to KafPy constructor; per-handler config passed at registration time
**Plans**: 1 plan
- [ ] 34-01-PLAN.md — Configuration model with 5 frozen dataclasses

### Phase 35: Handler Registration & Runtime
**Goal**: Decorator-based and explicit handler registration, KafkaMessage/HandlerContext/HandlerResult types, KafPy runtime with start/stop/run lifecycle.
**Depends on**: Phase 34
**Requirements**: REG-01, REG-02, REG-03, REG-04, REG-05, REG-06, REG-07
**Success Criteria** (what must be TRUE):
  1. @consumer.handler(topic="...", routing=...) decorates a callable as a handler; consumer.register_handler(topic, handler_fn, routing=...) provides explicit registration
  2. Handler callable signature is def handler(msg: KafkaMessage, ctx: HandlerContext) -> HandlerResult
  3. Sync, async, and batch handlers all use the same registration API
  4. app.start() begins consuming; app.stop() initiates graceful drain and shutdown; app.run() blocks until stop()
**Plans**: 1 plan
- [ ] 35-01-PLAN.md — Handler registration and KafPy runtime

### Phase 36: Error Handling
**Goal**: Python exception hierarchy (KafPyError base), Rust-to-Python translation at PyO3 boundary, meaningful Python exceptions.
**Depends on**: Phase 35
**Requirements**: ERR-01, ERR-02, ERR-03, ERR-04, ERR-05
**Success Criteria** (what must be TRUE):
  1. kafpy.exceptions exposes KafPyError, ConsumerError, HandlerError, ConfigurationError — KafPyError is the base
  2. Framework errors inherit from KafPyError, not Exception directly
  3. Rust errors are translated to Python exceptions at the PyO3 boundary with context-preserving messages
  4. KafkaMessage access errors (missing key, wrong type) raise HandlerError, not raw Rust panics
  5. kafpy.exceptions is the only public import path for exceptions; nothing from _kafpy or kafpy._rust leaks
**Plans**: 1 plan
- [ ] 36-01-PLAN.md — Python exception hierarchy and KafkaMessage access errors

### Phase 37: Documentation & Packaging
**Goal**: README/quickstart, docs/ directory, kafpy/guides/ module with markdown guides, full docstrings, maturin/pyproject.toml packaging.
**Depends on**: Phase 36
**Requirements**: DOC-01, DOC-02, DOC-03, DOC-04, DOC-05, DOC-06, BUILD-01, BUILD-02, BUILD-03, BUILD-04
**Success Criteria** (what must be TRUE):
  1. README.md at repo root with install instructions, quickstart example under 30 lines, and feature list
  2. docs/ directory with quickstart guide and API reference (auto-generated from docstrings)
  3. kafpy/guides/ as a Python package containing getting-started.md, handler-patterns.md, configuration.md, error-handling.md
  4. Every public class and function has a docstring: purpose, args, returns, raises
  5. pyproject.toml uses maturin backend; maturin develop works on a machine with librdkafka installed
**Plans**: TBD

### v2.0 Code Quality Refactor

### Phase 01: Extract Duplicated Logic
**Goal**: Extract duplicated helpers to reduce error/DLQ branch duplication in worker_pool and eliminate copied Python conversion logic.
**Depends on**: Phase 37
**Requirements**: DUP-01, DUP-02, DUP-03, DUP-04
**Plans**: 1 plan
- [ ] 01-01-PLAN.md — Extract handle_execution_failure, message_to_pydict, flush_partition_batch helpers

### Phase 02: Split worker_pool/ God Module
**Goal**: Extract PartitionAccumulator to batch/ module, extract batch_worker_loop and worker_loop to their own files.
**Depends on**: Phase 01
**Requirements**: SPLIT-A-01, SPLIT-A-02, SPLIT-A-03, SPLIT-A-04, SPLIT-A-05, SPLIT-A-06
**Plans**: 2 plans
- [ ] 02-01-PLAN.md — Extract PartitionAccumulator to worker_pool/accumulator.rs
- [ ] 02-02-PLAN.md — Extract loop functions and WorkerPool to pool.rs

### Phase 03: Split dispatcher/ and Extract runtime/
**Goal**: Extract ConsumerDispatcher to own file; extract RuntimeBuilder from pyconsumer.rs to new runtime/ module.
**Depends on**: Phase 02
**Requirements**: SPLIT-B-01, SPLIT-B-02, SPLIT-B-03, SPLIT-B-04
**Plans**: 1 plan
- [x] 03-01-PLAN.md — Extract ConsumerDispatcher + create RuntimeBuilder

### Phase 04: State Machine Extraction
**Goal**: Replace implicit Option/Bool state in worker_loop and batch_worker_loop with explicit state enums.
**Depends on**: Phase 03
**Requirements**: STATE-01, STATE-02, STATE-03, STATE-04, STATE-05
**Plans**: 2 plans
- [ ] 04-01-PLAN.md — WorkerState enum (worker_loop), BatchState enum (batch_worker_loop)
- [ ] 04-02-PLAN.md — RetryState enum (RetryCoordinator), exhaustive match verification

### Phase 05: Split coordinator/ Module
**Goal**: Extract OffsetTracker/OffsetCommitter to offset/, ShutdownCoordinator to shutdown/, RetryCoordinator to retry/.
**Depends on**: Phase 04
**Requirements**: SPLIT-C-01, SPLIT-C-02, SPLIT-C-03, SPLIT-C-04, SPLIT-C-05
**Plans**: TBD

### Phase 06: Type Safety
**Goal**: Create HandlerId newtype wrapper, consolidate error types, verify Send+Sync guarantees.
**Depends on**: Phase 05
**Requirements**: TYPES-01, TYPES-02, TYPES-03, TYPES-04, TYPES-05
**Plans**: TBD

---

## Progress

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. Foundation | v1.0 | - | Complete | 2026-04-15 |
| 6. Dispatcher Layer | v1.1 | - | Complete | 2026-04-16 |
| 9. Python Execution Lane | v1.2 | - | Complete | 2026-04-16 |
| 11. Offset Commit Coordinator | v1.3 | - | Complete | 2026-04-17 |
| 17. Failure Handling & DLQ | v1.4 | - | Complete | 2026-04-17 |
| 21. Routing Core | v1.5 | 3/3 | Complete | 2026-04-17 |
| 22. Python Integration | v1.5 | 1/1 | Complete | 2026-04-18 |
| 23. Dispatcher Integration | v1.5 | 1/1 | Complete | 2026-04-18 |
| 24. HandlerMode & Execution Foundation | v1.6 | 1/1 | Complete | 2026-04-18 |
| 25. Batch Accumulation & Flush | v1.6 | 3/3 | Complete | 2026-04-18 |
| 26. Async Python Handlers | v1.6 | 2/2 | Complete | 2026-04-18 |
| 27. Shutdown Drain & Polish | v1.6 | 1/1 | Complete | 2026-04-18 |
| 28. Metrics Infrastructure | v1.7 | 1/1 | Complete | 2026-04-18 |
| 29. Tracing Infrastructure | v1.7 | 1/1 | Complete | 2026-04-18 |
| 30. Kafka-Level Metrics | v1.7 | 1/1 | Complete | 2026-04-18 |
| 31. Runtime Introspection | v1.7 | 1/1 | Complete | 2026-04-18 |
| 32. Structured Logging | v1.7 | 1/1 | Complete | 2026-04-18 |
| 33. Public API Conventions | v1.8 | 2/2 | Complete | 2026-04-20 |
| 34. Configuration Model | v1.8 | 1/1 | Complete | 2026-04-20 |
| 35. Handler Registration & Runtime | v1.8 | 1/1 | Complete | 2026-04-20 |
| 36. Error Handling | v1.8 | 1/1 | Not started | - |
| 37. Documentation & Packaging | v1.8 | 1/1 | Complete | 2026-04-20 |
| 01. Extract Duplicated Logic | v2.0 | 1/1 | Complete | 2026-04-20 |
| 02. Split worker_pool/ | v2.0 | 2/2 | Complete | 2026-04-20 |
| 03. Split dispatcher/ + Extract runtime/ | v2.0 | 1/1 | Planned | - |
| 04. State Machine Extraction | v2.0 | 2/2 | Planned | - |
| 05. Split coordinator/ | v2.0 | TBD | - | - |
| 06. Type Safety | v2.0 | TBD | - | - |

---

*Last updated: 2026-04-20 after Phase 04 planning*
