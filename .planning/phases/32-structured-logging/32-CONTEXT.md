# Phase 32: Structured Logging - Context

**Gathered:** 2026-04-18
**Status:** Ready for planning

<domain>
## Phase Boundary

Phase 32 establishes consistent structured logging across KafPy. Unified field names across logging/metrics/tracing, configurable log format (json/pretty/simple) via ObservabilityConfig, per-component log levels, and Python log forwarding to Rust tracing via tracing_log.

</domain>

<decisions>
## Implementation Decisions

### Consistent Field Names — OBS-33
- **OBS-33:** All structured log events use consistent field names: handler_id, topic, partition, offset

### Log Format — OBS-34
- **OBS-34:** ObservabilityConfig accepts log format (json/pretty/simple) for tracing-subscriber

### Standard Fields — OBS-35
- **OBS-35:** Log events include handler_id, topic, partition, offset as standard fields

### worker_loop Logging — OBS-36
- **OBS-36:** worker_loop emits log::info!/log::error! at invoke start, completion, error

### Per-Component Levels — OBS-37
- **OBS-37:** Log level configurable per component (worker_loop, dispatcher, accumulator)

### Python Log Forwarding — OBS-38
- **OBS-38:** Python logging forwarded to Rust tracing via tracing_log::LogTracer

</decisions>

<canonical_refs>
### Codebase References
- `src/logging.rs` — existing logging setup
- `src/observability/config.rs` — ObservabilityConfig (Phase 29)
- `src/worker_pool/mod.rs` — worker_loop logging sites

### Prior Phases
- `.planning/phases/28-metrics-infrastructure/28-CONTEXT.md` — field naming aligned with metrics
- `.planning/phases/29-tracing-infrastructure/29-CONTEXT.md` — kafpy.{component}.{operation} naming

### Requirements
- `.planning/REQUIREMENTS.md` — OBS-33 through OBS-38
</canonical_refs>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope.
</deferred>

---
*Phase: 32-structured-logging*
*Context gathered: 2026-04-18*
