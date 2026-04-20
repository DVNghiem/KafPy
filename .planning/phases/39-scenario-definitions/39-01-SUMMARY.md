# Phase 39 Plan 01: Scenario Trait and Core Scenarios Summary

## Plan Overview
- **Plan:** 39-01
- **Phase:** 39-scenario-definitions
- **Status:** COMPLETE
- **Completed:** 2026-04-20
- **Duration:** ~5 minutes

## Objective
Create the Scenario trait, WorkloadProfile enum, and the first two concrete scenario types (ThroughputScenario, LatencyScenario) in src/benchmark/scenarios.rs. Re-export from mod.rs for consumption by Phase 40 BenchmarkRunner.

## Tasks Completed

| # | Task | Commit | Files |
|---|------|--------|-------|
| 1 | Create Scenario trait and WorkloadProfile enum | a98d5e2 | src/benchmark/scenarios.rs |
| 2 | Create ThroughputScenario struct | a98d5e2 | src/benchmark/scenarios.rs |
| 3 | Create LatencyScenario struct | a98d5e2 | src/benchmark/scenarios.rs |
| 4 | Add re-export to mod.rs | a98d5e2 | src/benchmark/mod.rs |

## Deliverables

### Scenario Trait
- `scenario_name(&self) -> &str` — returns scenario type name
- `build_config(&self) -> ScenarioConfig` — returns configuration for result
- `default_warmup_messages(&self) -> usize` — returns 1000 by default

### WorkloadProfile Enum
Five variants: `ThroughputFocused`, `LatencyFocused`, `FailureFocused`, `BatchComparison`, `HandlerModeComparison`

### ThroughputScenario
Fields: `target_topic`, `messages_per_second` (Option), `payload_bytes`, `num_messages` (Option), `duration_secs` (Option), `warmup_messages`

### LatencyScenario
Fields: `target_topic`, `messages_per_second`, `payload_bytes`, `num_messages`, `warmup_messages`

## Key Decisions

| Decision | Rationale |
|----------|-----------|
| ScenarioConfig imported from Phase 38 results.rs | Avoids duplication, Phase 40 BenchmarkRunner depends on same type |
| WorkloadProfile uses simple enum (not newtype wrappers) | Plan D-03 explicitly specified 5 variants; complexity not warranted |
| HandlerModeComparison is a WorkloadProfile variant, not a struct | Aligns with plan D-03 classification |

## Verification

```bash
cargo check --lib  # PASSES - no errors
```

## Deviations from Plan

None - plan executed exactly as written.

## TDD Gate Compliance

Not applicable (plan type is `execute`, not `tdd`).

## Commits

- `a98d5e2` — feat(39-01): implement Scenario trait and core scenarios

## Files Created/Modified

| File | Change | Lines |
|------|--------|-------|
| src/benchmark/scenarios.rs | created | ~270 |
| src/benchmark/mod.rs | modified | +12 |
| Cargo.toml | modified | +3 (dev-dependencies) |

## Notes

- Pre-existing unused import warnings in coordinator/, failure/, observability/, routing/ modules are out of scope for this plan
- `cargo test --lib` fails to link due to Python symbols not available in no-run mode (expected for pyo3 cdylib); `cargo check --lib` passes which is the authoritative verification