# Architecture

This section documents the internal architecture of KafPy.

## Contents

- [Overview](overview.md) — High-level architecture and design principles
- [Modules](modules.md) — Module-by-module breakdown with responsibilities
- [Message Flow](message-flow.md) — Kafka→Python→Offset commit flow diagrams
- [State Machines](state-machines.md) — WorkerState, BatchState, ShutdownPhase
- [Routing](routing.md) — Routing chain decision tree
- [PyO3 Boundary](pyboundary.md) — GIL boundary patterns and async/sync bridges

## Audience

This documentation serves:

- **New contributors** — Onboarding, setup, coding conventions
- **API users** — Public API contracts, configuration, usage patterns
- **Maintainers** — Design rationale, extension patterns, internals

## Key Design Principles

1. **Rust core / Python business logic** — Performance + idiomatic bindings
2. **PyO3-free consumer core** — Testable without Python interpreter
3. **Per-topic bounded dispatch** — Isolated backpressure per handler
4. **Highest contiguous offset commit** — At-least-once delivery guarantee
5. **Explicit state machines** — WorkerState, BatchState replace boolean flags