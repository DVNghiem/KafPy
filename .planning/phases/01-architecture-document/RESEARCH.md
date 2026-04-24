# Phase 1: Architecture Document - Research

**Phase:** 01
**Research Date:** 2026-04-24
**Status:** COMPLETE
**Forced refresh:** Yes (--research flag)

---

## RESEARCH COMPLETE

---

## Research Summary

This phase will create comprehensive architecture documentation for KafPy using MkDocs + Mermaid diagrams.

---

## What I Learned

### MkDocs + Mermaid Best Practices for Rust/PyO3

1. **MkDocs Setup for Rust Projects**
   - Use `mkdocs-material` theme for professional appearance
   - Configure `mkdocs-mermaid2-plugin` for Mermaid diagram rendering
   - Existing `docs/` directory at project root — use existing structure
   - Use `markdown-include` or `toc` macro for multi-file docs

2. **Rust Documentation Patterns**
   - `rustdoc` generates API docs from doc comments (`///`, `//!`)
   - Document public API surface in `src/lib.rs` module docs
   - Use `[#]: <> (hidden)` comments for internal documentation links
   - Cross-reference with `{{#title}}` templates in MkDocs

3. **Mermaid Diagram Types for Architecture**
   - `flowchart LR/RL/TB/BT` — message flows, data pipelines
   - `graph TD` — state machines, state transitions
   - `classDiagram` — type hierarchies, trait implementations
   - `sequenceDiagram` — call sequences, API interactions
   - `erDiagram` — entity relationships for data models
   - `stateDiagram-v2` — explicit state machine diagrams

4. **PyO3-Specific Documentation**
   - Document the GIL boundary — `spawn_blocking` is critical for performance
   - Show `#[pyclass]` → Python class mapping
   - Document async/sync boundaries with `future_into_py`
   - Explain `Arc<RwLock<T>>` patterns for thread-safe sharing

### Module Inventory (from src/)

**74 Rust source files across 14 modules:**

| Module | Files | Purpose |
|--------|-------|---------|
| `consumer/` | 5 | ConsumerConfig, OwnedMessage, ConsumerRunner, ConsumerStream, ConsumerTask |
| `dispatcher/` | 5 | Dispatcher, QueueManager, BackpressurePolicy, ConsumerDispatcher |
| `worker_pool/` | 6 | WorkerPool, worker_loop, batch_worker_loop, PerPartitionBuffer, WorkerState, BatchState |
| `routing/` | 9 | RoutingChain, Router, HandlerId (newtype), RoutingDecision, pattern/header/key/python routers |
| `python/` | 6 | PythonHandler, Executor trait, ExecutionContext, ExecutionResult, BatchAccumulator |
| `runtime/` | 2 | RuntimeBuilder for composing ConsumerRuntime |
| `offset/` | 4 | OffsetTracker, OffsetCoordinator, OffsetCommitter, CommitTask |
| `shutdown/` | 2 | ShutdownCoordinator, ShutdownPhase |
| `retry/` | 3 | RetryPolicy, RetryCoordinator |
| `dlq/` | 4 | DlqRouter, DlqMetadata, SharedDlqProducer |
| `failure/` | 5 | FailureReason, FailureCategory, FailureClassifier, Logging |
| `observability/` | 5 | MetricsSink, PrometheusMetricsSink, LogTracer, RuntimeSnapshot, Config |
| `benchmark/` | 5 | BenchmarkRunner, scenarios, measurement, results, hardening |
| `coordinator/` | 2 | Thin re-export layer (backward-compatible) |

**Files at src/ root:**
- `lib.rs` — PyO3 module entry point, Send+Sync assertions
- `config.rs` — ConsumerConfig, ProducerConfig (#[pymodule-exposed])
- `error.rs` — unified error re-exports
- `errors.rs` — internal error types
- `kafka_message.rs` — KafkaMessage Python class
- `logging.rs` — Logger initialization
- `produce.rs` — PyProducer
- `pyconsumer.rs` — PyO3 Consumer bridge

### Existing Documentation Assets

**docs/ directory already exists with:**
- `index.md`, `getting-started.md`, `configuration.md`
- `consumer.md`, `handlers.md`, `benchmark.md`, `error-handling.md`
- `use-cases.md`, `best-practices.md`, `routing.md`
- `api/kafpy.md`

**Gap:** No comprehensive architecture section with Mermaid diagrams

**.planning/codebase/ provides:**
- `ARCHITECTURE.md` — outdated (v1.0 state), needs refresh to v2.0
- `STRUCTURE.md` — accurate file inventory
- `STACK.md` — technology stack
- `CONVENTIONS.md` — PyO3 patterns, async patterns

### Recommended Documentation Structure

```
docs/
├── index.md                         # (exists)
├── architecture/                    # NEW — comprehensive section
│   ├── index.md                    # Section landing
│   ├── overview.md                 # High-level + module org Mermaid diagrams
│   ├── modules.md                  # All 14 modules detailed
│   ├── message-flow.md             # sequenceDiagram: Kafka→Python→Offset
│   ├── state-machines.md           # stateDiagram-v2: WorkerState, BatchState, ShutdownPhase
│   ├── routing.md                  # Routing chain flowchart
│   └── pyboundary.md               # GIL boundary, spawn_blocking patterns
├── api/
│   └── kafpy.md                    # (exists)
├── contributing/                   # NEW
│   ├── setup.md                    # Dev environment
│   └── conventions.md              # From .planning/codebase/CONVENTIONS.md
└── mkdocs.yml                      # (to be created/updated)
```

### Key Diagrams to Create

1. **Module Hierarchy** (graph TD) — pub/use relationships, color-coded by type
2. **Message Flow** (sequenceDiagram) — Kafka→Consumer→Dispatcher→WorkerPool→Python→OffsetCommit
3. **State Machine** (stateDiagram-v2) — WorkerState, BatchState, ShutdownPhase transitions
4. **Routing Chain** (flowchart TB) — pattern→header→key→python→default precedence
5. **Shutdown Lifecycle** (stateDiagram-v2) — 4-phase graceful shutdown
6. **Retry/DLQ Flow** (flowchart LR) — failure classification → retry → DLQ routing
7. **PyO3 Boundary** (flowchart TB) — GIL acquire/release, spawn_blocking pattern

---

## Implementation Approach

### Phase Boundary
Create comprehensive architecture document serving:
- **New contributors** — onboarding path, setup, conventions
- **API users** — public API contracts, configuration, usage examples
- **Maintainers** — design rationale, extension patterns, internals

### Deliverables
1. `mkdocs.yml` — MkDocs configuration with material theme and Mermaid2
2. `docs/architecture/` directory with Markdown + Mermaid architecture docs
3. `docs/contributing/` directory with setup and conventions
4. Module-by-module specs with responsibilities, APIs, relationships

### Verification
- MkDocs builds successfully (`mkdocs build`)
- All Mermaid diagrams render without errors
- All src/ modules have documentation coverage
- Cross-references between docs work correctly

---

## Notes

- Existing `docs/` MkDocs structure — add architecture section, don't replace
- Existing `.planning/codebase/ARCHITECTURE.md` is outdated (v1.0) — refresh with v2.0 state
- `src/` module structure from v2.0 is the authoritative source for module inventory
- MkDocs `mermaid2` plugin handles all diagram rendering in HTML output
- Consider `rustdoc` integration for API docs generation from source doc comments
