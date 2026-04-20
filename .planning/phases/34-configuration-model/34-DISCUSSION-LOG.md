# Phase 34: Configuration Model - Discussion Log

> **Audit trail only.** Decisions are captured in CONTEXT.md.

**Date:** 2026-04-20
**Phase:** 34-configuration-model
**Areas discussed:** Python config layer, type-safe config fields, builder/immutability patterns, fallback_handler type

---

## Python Config Layer

| Option | Description | Selected |
|--------|-------------|----------|
| Python wrapper | Pure Python ConsumerConfig that translates bootstrap_servers → brokers when calling Rust | |
| Rust re-export | Keep re-exporting Rust ConsumerConfig; use brokers= in Python | |
| **Hybrid (Recommended)** | Rust ConsumerConfig stays for power users; Python ConsumerConfig wraps it with better defaults and type-safety | ✓ |

**User's choice:** Hybrid — Python ConsumerConfig as frozen dataclass wrapping Rust one

---

## Type-Safe Config Fields

### routing_mode in RoutingConfig

| Option | Description | Selected |
|--------|-------------|----------|
| String literals | "pattern" / "header" / "key" / "python" / "default" | ✓ |
| Enum class | class RoutingMode(enum.Enum) | |

**User's choice:** String literals

### jitter_factor in RetryConfig

| Option | Description | Selected |
|--------|-------------|----------|
| Float 0.0-1.0 | jitter_factor: float = 0.1 — fraction of delay as randomness | ✓ |
| Integer milliseconds | jitter_ms: int — absolute jitter in ms | |

**User's choice:** Float 0.0-1.0

---

## Builder / Immutability Style

| Option | Description | Selected |
|--------|-------------|----------|
| @dataclass(frozen=True) | Idiomatic Python, simple, no extra files | ✓ |
| Explicit builder class | ConsumerConfig.builder().group_id("x").build() — fluent, verbose | |
| from_dict / from_env class methods | Factory methods for specific sources | |

**User's choice:** @dataclass(frozen=True)

---

## fallback_handler Type in RoutingConfig

| Option | Description | Selected |
|--------|-------------|----------|
| String handler name | Name of a registered handler function | ✓ |
| Callable | The actual handler function object | |
| Either | Accept both str and Callable | |

**User's choice:** String handler name

---

## Deferred Ideas

None
