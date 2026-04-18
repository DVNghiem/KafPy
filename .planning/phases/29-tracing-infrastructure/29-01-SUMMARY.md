# Phase 29 Plan 01 Summary: Tracing Infrastructure Foundation

**Plan:** 29-01
**Status:** Complete
**Commit:** a2dde92
**Date:** 2026-04-18

## Objective

Create tracing infrastructure foundation: ObservabilityConfig struct, enable_otel_tracing() setup function, tracing module with OpenTelemetry Layer.

## Completed Tasks

| Task | Files | Commit |
|------|-------|--------|
| Create config.rs | src/observability/config.rs | a2dde92 |
| Create tracing.rs | src/observability/tracing.rs | a2dde92 |
| Update mod.rs | src/observability/mod.rs | a2dde92 |
| Add Cargo deps | Cargo.toml | a2dde92 |
| Fix worker_pool calls | src/worker_pool/mod.rs | a2dde92 |

## Artifacts Created

### src/observability/config.rs (39 lines)
- `LogFormat` enum: Json, Pretty, Simple
- `ObservabilityConfig` struct: otlp_endpoint, service_name, sampling_ratio, log_format
- Default: otlp_endpoint=None (zero-cost when disabled)

### src/observability/tracing.rs (169 lines)
- `KafpySpanExt` trait with `kafpy_handler_invoke`, `kafpy_dispatch_process`, `kafpy_dlq_route`
- `enable_otel_tracing()` returning LayerHandle (full impl deferred to Wave 3)
- `extract_trace_headers()` / `inject_trace_context()` for W3C propagation
- `SetupError` enum

### src/observability/mod.rs (14 lines)
- Exports: config, tracing modules
- Re-exports: LogFormat, ObservabilityConfig, enable_otel_tracing, extract_trace_headers, inject_trace_context, SetupError, KafpySpanExt

### Cargo.toml additions
- opentelemetry = "0.26"
- opentelemetry-otlp = "0.17"
- tracing-opentelemetry = "0.31"
- opentelemetry_sdk = "0.26"

## Key Decisions

1. **Zero-cost when disabled**: otlp_endpoint=None returns fmt layer only, no OTLP exporter created
2. **User owns LayerHandle**: KafPy never calls set_global_default()
3. **Full implementation deferred**: enable_otel_tracing returns NotYetImplemented error (Wave 3)
4. **span.in_scope() pattern**: For async code, not Span::enter()
5. **kafpy.{component}.{operation} naming**: Consistent with metrics naming

## Deviations

- enable_otel_tracing full implementation deferred to Phase 29 Wave 3 due to OpenTelemetry API version conflicts between 0.24 and 0.26 crates

## Verification

- `cargo check --lib` passes with warnings only
- ObservabilityConfig has required fields
- KafpySpanExt trait provides all span creation methods
- W3C helpers available for spawn_blocking boundary

## Dependencies

- Phase 28-02 (metrics infrastructure patterns)
