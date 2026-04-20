# Phase 34: Configuration Model — Summary

**Completed:** 2026-04-20
**Plans:** 1/1 (34-01-PLAN.md)
**Status:** Complete

## Files Created/Modified

| File | Change |
|------|--------|
| `kafpy/config.py` | **Created** — 5 frozen dataclasses: ConsumerConfig, RoutingConfig, RetryConfig, BatchConfig, ConcurrencyConfig |
| `kafpy/__init__.py` | **Modified** — ConsumerConfig from .config, all 5 config classes in __all__ |

## Decisions Applied

| ID | Decision | Source |
|----|----------|--------|
| D-01 | Hybrid config model: Python ConsumerConfig wrapping Rust ConsumerConfig | 34-CONTEXT.md |
| D-02 | Python field name `bootstrap_servers` maps to Rust `brokers` | 34-CONTEXT.md |
| D-03 | RoutingConfig with routing_mode (pattern/header/key/python/default) | 34-CONTEXT.md |
| D-04 | String handler name for fallback_handler | 34-CONTEXT.md |
| D-05 | String literal values for retry backoff | 34-CONTEXT.md |
| D-06 | @dataclass(frozen=True) for all config classes | 34-CONTEXT.md |
| D-07 | ConsumerConfig.to_rust() converts Python config to Rust | 34-CONTEXT.md |
| D-08 | Extra fields (fetch_min_bytes, etc.) on ConsumerConfig | 34-CONTEXT.md |
| D-09 | RoutingConfig validates routing_mode against known modes | 34-CONTEXT.md |
| D-10 | RetryConfig validates jitter_factor 0.0-1.0 | 34-CONTEXT.md |
| D-11 | BatchConfig with max_batch_size and max_batch_timeout_ms | 34-CONTEXT.md |
| D-12 | ConcurrencyConfig with num_workers | 34-CONTEXT.md |
| D-13 | ConsumerConfig.__post_init__ validates auto_offset_reset | 34-CONTEXT.md |
| D-14 | Module-level validation constants (ROUTING_MODES, AUTO_OFFSET_RESET_VALUES) | 34-CONTEXT.md |

## Success Criteria Status

| Criterion | Status |
|-----------|--------|
| ConsumerConfig accepts bootstrap_servers, group_id, topics, auto_offset_reset, enable_auto_commit and is frozen after construction | ✓ |
| RoutingConfig accepts routing_mode and fallback_handler; BatchConfig accepts max_batch_size and max_batch_timeout_ms | ✓ |
| RetryConfig and ConcurrencyConfig are immutable with max_attempts/base_delay/max_delay/jitter_factor and num_workers respectively | ✓ |
| ConsumerConfig is passed to KafPy constructor; per-handler config passed at registration time | ✓ |

## Notes

- ConsumerConfig wraps _kafpy.ConsumerConfig via `to_rust()` method
- bootstrap_servers maps to brokers at the Python→Rust boundary (D-02)
- All classes use `@dataclass(frozen=True)` for immutability
- Module-level frozenset constants used for validation (ROUTING_MODES, AUTO_OFFSET_RESET_VALUES)
