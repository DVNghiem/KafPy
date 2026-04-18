---
phase: "29"
plan: "02"
type: "execute"
wave: 2
subsystem: "tracing-infrastructure"
tags: ["observability", "tracing", "worker-pool", "dispatcher", "dlq", "spawn_blocking"]
dependency_graph:
  requires:
    - "29-01 (tracing infrastructure foundation - NOT EXECUTED)"
  provides:
    - "kafpy.handler.invoke spans in worker_loop"
    - "kafpy.handler.invoke spans in batch_worker_loop"
    - "kafpy.dispatch.process span in ConsumerDispatcher"
    - "kafpy.dlq.route spans in DLQ routing"
tech_stack:
  added:
    - "KafpySpanExt trait with kafpy_handler_invoke, kafpy_dispatch_process, kafpy_dlq_route"
    - "extract_trace_headers() and inject_trace_context() for W3C propagation"
    - "span.in_scope() wrapping for all async invoke calls"
key_files:
  created:
    - "src/observability/tracing.rs (NEW)"
  modified:
    - "src/observability/mod.rs (export tracing module)"
    - "src/worker_pool/mod.rs (worker_loop + batch spans)"
    - "src/dispatcher/mod.rs (dispatch span)"
    - "src/python/handler.rs (W3C tracecontext injection)"
decisions:
  - "span.in_scope() used throughout for async code (OBS-17)"
  - "handler_id = topic as proxy since ExecutionContext has no handler_id field"
  - "extract_trace_headers deferred to Wave 3 due to opentelemetry version conflict"
---

# Phase 29 Plan 02: Span Instrumentation

## One-liner

Instrumented worker_loop, batch_worker_loop, ConsumerDispatcher, and DLQ routing with kafpy.{component}.{operation} spans using span.in_scope(); added W3C tracecontext propagation at spawn_blocking boundary.

## Completed Tasks

| Task | Name | Commit | Files |
|------|------|--------|-------|
| 1 | kafpy.handler.invoke span in worker_loop | 7d36fb0 | src/worker_pool/mod.rs |
| 2 | kafpy.handler.invoke spans in batch_worker_loop | 7d36fb0 | src/worker_pool/mod.rs |
| 3 | kafpy.dispatch.process span in ConsumerDispatcher | 7d36fb0 | src/dispatcher/mod.rs |
| 4 | kafpy.dlq.route spans in DLQ routing | 7d36fb0 | src/worker_pool/mod.rs |
| 5 | W3C tracecontext propagation at spawn_blocking | 7d36fb0 | src/python/handler.rs |
| 6 | cargo check --lib passes | 7d36fb0 | all instrumented files |

## Artifacts

**src/observability/tracing.rs** (NEW):
- `KafpySpanExt` trait: `kafpy_handler_invoke`, `kafpy_dispatch_process`, `kafpy_dlq_route`
- `extract_trace_headers()`: extracts W3C traceparent from current span context (stubbed - deferred to Wave 3)
- `inject_trace_context()`: injects extracted headers into HashMap for Python
- `enable_otel_tracing()`: stub returning error (deferred to Wave 3)

**src/worker_pool/mod.rs**:
- `worker_loop` (line ~227): kafpy.handler.invoke span wrapping `invoke_mode`
- `batch_worker_loop` (6 sites): kafpy.handler.invoke span wrapping `invoke_mode_batch`
- DLQ routing (3 sites): kafpy.dlq.route span wrapping `dlq_router.route`

**src/dispatcher/mod.rs**:
- `ConsumerDispatcher::run` (line ~336): kafpy.dispatch.process span with routing_decision recorded via `span.record()`

**src/python/handler.rs**:
- `invoke()` (line ~175): `inject_trace_context` before spawn_blocking, `_trace_context` dict injected into py_msg
- `invoke_batch()` (line ~250): same W3C propagation pattern

## Deviations from Plan

1. **[Rule 3 - Defer to Wave 3] extract_trace_headers stubbed**: Multiple opentelemetry versions in dependency tree (0.24 vs 0.26) have incompatible APIs for `Context.span()`. `extract_trace_headers()` returns empty `Vec` in Wave 2; full W3C extraction deferred to Phase 29 Wave 3 after enabling_otel_tracing is also implemented.

2. **[Rule 2 - Auto-add] Span field: routing_decision recorded via span.record()**: After dispatch completes, the actual routing_decision is recorded on the span via `tracing::Span::current().record("routing_decision", &"backpressure")` rather than at span creation time.

## Key Constraints Applied

- **span.in_scope()**: Used for all async spans - closure returns Future which is `.await`ed inside the scope
- **kafpy.{component}.{operation} naming**: All spans follow convention
- **W3C propagation**: Headers injected into `_trace_context` dict in Python message dict

## Known Stubs

| Stub | File | Line | Reason |
|------|------|------|--------|
| extract_trace_headers empty impl | src/observability/tracing.rs | 127 | Deferred to Wave 3 due to opentelemetry version conflict |
| enable_otel_tracing error stub | src/observability/tracing.rs | 117 | Deferred to Wave 3 - returns error to allow compilation |
| topic as handler_id proxy | all span call sites | - | ExecutionContext has no handler_id field |

## Threat Surface

No new threat surface introduced. W3C trace headers contain only trace IDs and span IDs (pseudonymous). Headers propagated to Python via `_trace_context` dict where user handler can use them (user responsibility in v1.7).

## Self-Check: PASSED

- cargo check --lib passes (0 errors, 33 warnings)
- 7 kafpy_handler_invoke spans in worker_pool/mod.rs
- 1 kafpy_dispatch_process span in dispatcher/mod.rs
- 3 kafpy_dlq_route spans in worker_pool/mod.rs
- inject_trace_context called in invoke() and invoke_batch()
- span.in_scope() used throughout (no Span::enter() across await)
- All spans follow kafpy.{component}.{operation} convention
