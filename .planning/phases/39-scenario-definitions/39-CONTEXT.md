# Phase 39: Scenario Definitions - Context

**Gathered:** 2026-04-20
**Status:** Ready for planning

<domain>
## Phase Boundary

Data models for benchmark scenario definitions — defining WHAT to benchmark (target topic, message rate, payload size, duration, warmup messages) without coupling to HOW measurement happens. Pure trait + enum + struct definitions in `src/benchmark/` as `pub(crate)`, invisible to Python.

</domain>

<decisions>
## Implementation Decisions

### Scenario Trait Architecture (SCEN-01)
- **D-01:** `Scenario` trait defines WHAT to benchmark: target topic, message rate, payload size, duration, warmup messages
- **D-02:** Scenario trait consumed by BenchmarkRunner (Phase 40) — separation of WHAT vs HOW

### WorkloadProfile Enum (SCEN-02)
- **D-03:** `WorkloadProfile` enum with variants: `ThroughputFocused`, `LatencyFocused`, `FailureFocused`, `BatchComparison`, `HandlerModeComparison`

### Concrete Scenario Types (SCEN-03 through SCEN-08)
- **D-04:** `ThroughputScenario`: configurable messages_per_second, payload_bytes, num_messages or duration
- **D-05:** `LatencyScenario`: single-message latency measurement under steady-state load
- **D-06:** `FailureScenario`: configurable failure rate (%), retry behavior, DLQ routing exercised
- **D-07:** `BatchVsSyncScenario`: compares `BatchSync` vs `SingleSync` handler modes under identical workload
- **D-08:** `AsyncVsSyncScenario`: compares `SingleAsync` vs `SingleSync` under identical workload

### Configurability (SCEN-08)
- **D-09:** All scenarios configurable via Python dict or TOML file; new scenarios addable without touching runner core
- **D-10:** Scenario enum holds concrete variants — no dynamic scenario creation in this phase

### Type Structure
- **D-11:** ScenarioConfig (from Phase 38) echoed in results for reproducibility
- **D-12:** Scenario trait has `default_warmup_messages()` returning 1000 (MEAS-07)

### Claude's Discretion
- Scenario field ordering and helper method names — standard Rust trait conventions
- Whether to use `Default` impl or explicit constructor for each scenario struct

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Phase 38 (Dependency)
- `src/benchmark/results.rs` — ScenarioConfig struct already defined
- `src/benchmark/measurement.rs` — Measurement infrastructure types available

### Observability (reuse patterns)
- `src/observability/metrics.rs` — MetricLabels pattern for consistent label construction
- `src/failure/retry.rs` — RetryPolicy for failure scenario configuration
- `src/dlq/mod.rs` — DlqMetadata for DLQ routing

### Prior Phase Patterns
- `src/failure/retry.rs` — RetryCoordinator as a good example of a data-driven struct with clear fields
- `src/dlq/mod.rs` — DlqMetadata as a good example of a metadata envelope struct

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `ScenarioConfig` from Phase 38 — already has scenario_name, num_messages, payload_bytes, rate, warmup_messages, failure_rate
- `MetricLabels` from observability — for consistent benchmark metric labels
- `RetryPolicy` from failure subsystem — for configuring retry behavior in FailureScenario

### Established Patterns
- Structs with clear field names and derive(Serialize, Deserialize) — follow existing pattern
- `pub(crate)` module visibility for internals — benchmark infrastructure not exposed to Python
- Enum variants with data — use struct variants for scenarios with configuration

### Integration Points
- Phase 40 (Benchmark Runner) consumes Scenario trait
- Phase 38 (Result Models) provides ScenarioConfig for reproducibility

</code_context>

<specifics>
## Specific Ideas

- `Scenario::scenario_name()` method returns the scenario type name
- `Scenario::build_config()` method returns a ScenarioConfig for the result
- Handler mode comparison scenarios use identical message generator and payload size
- Warmup exclusion configurable via ScenarioConfig.warmup_messages

</specifics>

<deferred>
## Deferred Ideas

None — all SCEN requirements are in-scope for Phase 39.

</deferred>

---

*Phase: 39-scenario-definitions*
*Context gathered: 2026-04-20*