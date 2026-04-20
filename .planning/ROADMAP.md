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
| 37. Documentation & Packaging | v1.8 | 1/1 | Complete    | 2026-04-20 |

---

*Last updated: 2026-04-20 after Phase 36 planning*