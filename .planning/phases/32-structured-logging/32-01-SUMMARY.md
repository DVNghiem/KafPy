# Phase 32-01 Summary: Structured Logging

## Objective

Implement structured logging with consistent field names (OBS-33), configurable log format (OBS-34), per-component log levels (OBS-37), and Python log forwarding (OBS-38).

## Changes Made

### 1. Added tracing-log dependency

**File:** `Cargo.toml`
- Added `tracing-log = "0.2"` for Python log forwarding (OBS-38)

### 2. Added ComponentLogLevels to ObservabilityConfig

**File:** `src/observability/config.rs`
- Added `ComponentLogLevels` struct with per-component levels (worker_loop, dispatcher, accumulator)
- Added `component_log_levels: ComponentLogLevels` field to `ObservabilityConfig`

### 3. Updated Logger::init() to use ObservabilityConfig

**File:** `src/logging.rs`
- Accepts `&ObservabilityConfig` parameter
- Applies log format (json/pretty/simple) via tracing-subscriber
- Builds EnvFilter from per-component log levels
- Wires `tracing_log::LogTracer::init()` for Python log forwarding

### 4. Added structured log events to worker_loop

**File:** `src/worker_pool/mod.rs`
- **Invoke start**: `tracing::info!` with handler_id, topic, partition, offset, mode
- **Invoke complete**: `tracing::info!` with handler_id, topic, partition, offset, elapsed_ms
- **Invoke error**: `tracing::error!` with handler_id, topic, partition, offset, error_type

## Fields (OBS-33, OBS-35)

All structured log events use consistent field names:
- `handler_id`
- `topic`
- `partition`
- `offset`

## Per-Component Log Levels (OBS-37)

Default levels:
- `worker_loop`: INFO
- `dispatcher`: DEBUG
- `accumulator`: DEBUG

## Python Log Forwarding (OBS-38)

Python handler log messages (via Python's logging module) are forwarded to Rust `tracing` via `tracing_log::LogTracer` integration.

## Verification

- [x] `cargo check` passes
- [x] Log format configurable via ObservabilityConfig::log_format
- [x] worker_loop emits structured logs with consistent field names