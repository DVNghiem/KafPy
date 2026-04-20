# Phase 34: Configuration Model - Context

**Gathered:** 2026-04-20
**Status:** Ready for planning

<domain>
## Phase Boundary

Python-side configuration objects with immutable builders for ConsumerConfig, RoutingConfig, RetryConfig, BatchConfig, and ConcurrencyConfig. Phase 33's __all__ and pub boundary decisions are the foundation — this phase implements the config classes listed in PKG-03.

</domain>

<decisions>
## Implementation Decisions

### Config Layer Architecture
- **D-01:** Hybrid config model — Rust `ConsumerConfig` stays as re-export for power users (rdkafka-level control); Python `ConsumerConfig` wraps it with better defaults, type-safety, and `bootstrap_servers` naming. Same pattern applies to `ProducerConfig`.

### Naming Conventions
- **D-02:** Python `ConsumerConfig` uses `bootstrap_servers` as the field name (aligns with CFG-01); internally translates to `brokers` when passing to Rust `ConsumerConfig`
- **D-03:** All config field names use snake_case (Python convention); enum values use string literals

### RoutingConfig
- **D-04:** `routing_mode` accepts string literals: "pattern", "header", "key", "python", "default"
- **D-05:** `fallback_handler` is a string (handler name, not a callable) — references a registered handler by name

### RetryConfig
- **D-06:** `jitter_factor` is a float in range 0.0-1.0 representing the fraction of base_delay added as random jitter

### Immutability Pattern
- **D-07:** All config classes use `@dataclass(frozen=True)` — idiomatic Python, no extra builder classes needed

### Config Classes to Implement
- **D-08:** `ConsumerConfig` — bootstrap_servers, group_id, topics, auto_offset_reset, enable_auto_commit, session_timeout_ms, heartbeat_interval_ms, max_poll_interval_ms, security_protocol, sasl_mechanism, sasl_username, sasl_password, fetch_min_bytes, max_partition_fetch_bytes, partition_assignment_strategy, retry_backoff_ms, message_batch_size
- **D-09:** `RoutingConfig` — routing_mode, fallback_handler
- **D-10:** `RetryConfig` — max_attempts, base_delay, max_delay, jitter_factor
- **D-11:** `BatchConfig` — max_batch_size, max_batch_timeout_ms
- **D-12:** `ConcurrencyConfig` — num_workers

### Per-Handler Config
- **D-13:** ConsumerConfig is passed to KafPy constructor; per-handler config (RoutingConfig, RetryConfig, BatchConfig, ConcurrencyConfig) passed at handler registration time

### Module Location
- **D-14:** All config classes live in `kafpy/config.py`; re-exported at package level per PKG-03

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

- `.planning/REQUIREMENTS.md` — CFG-01 through CFG-07 (config model requirements)
- `.planning/ROADMAP.md` Phase 34 — full phase goal and success criteria
- `.planning/phases/33-public-api-conventions/33-CONTEXT.md` — Phase 33 decisions (naming, __all__, pub boundary)

</canonical_refs>

<integration>
## Integration Points

### Phase 33 (Public API Conventions)
- PKG-03 requires `kafpy/config.py` with config classes — this phase implements it
- `__all__` in `kafpy/__init__.py` already lists these types with placeholder comments

### Phase 35 (Handler Registration & Runtime)
- RoutingConfig and per-handler config passed at registration time
- Handler callable signature: `def handler(msg: KafkaMessage, ctx: HandlerContext) -> HandlerResult`

### Phase 36 (Error Handling)
- ConfigurationError in kafpy.exceptions (Phase 36) — config classes should raise this for validation failures

</integration>

<deferred>
## Deferred Ideas

None — all configuration decisions captured above.

</deferred>

---
*Phase: 34-configuration-model*
*Context gathered: 2026-04-20*
