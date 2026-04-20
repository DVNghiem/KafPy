# Phase 40-02: Graceful Termination & CLI — Summary

**Plan:** 40-02
**Tasks:** 6/6
**Completed:** 2026-04-20

## Deliverables

| File | Description |
|------|-------------|
| `src/benchmark/runner.rs` | graceful_teardown(), MetricsSink integration |
| `kafpy/benchmark.py` | Python CLI entry point (161 lines) |

## Key Implementations

- `graceful_teardown()`: drains inflight messages, commits offsets, emits final metrics (RUN-05)
- MetricsSink integration: run_scenario emits throughput/latency/error_rate/memory_delta (RUN-03)
- PyO3 #[pymethods] with `new()` and `run_scenario_py(scenario_name, config_json)` (RUN-06)
- Python CLI: `python -m kafpy.benchmark run --scenario throughput --output ./results` (RUN-07)

## Commits

- `32e95b8` — feat(40-02): implement graceful termination, CLI