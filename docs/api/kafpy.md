# KafPy API Reference

## Top-level Imports

```python
import kafpy
```

The `kafpy` package exposes all public types directly:

| Type | Description |
|------|-------------|
| `kafpy.ConsumerConfig` | Consumer configuration (frozen dataclass) |
| `kafpy.Consumer` | Python wrapper around Rust Consumer |
| `kafpy.KafPy` | Main runtime with handler lifecycle |
| `kafpy.KafkaMessage` | Kafka message with typed field access |
| `kafpy.HandlerContext` | Context for handler invocation |
| `kafpy.HandlerResult` | Result of a handler invocation |
| `kafpy.RetryConfig` | Retry policy configuration |
| `kafpy.ObservabilityConfig` | OTLP/metrics observability configuration |
| `kafpy.FailureCategory` | Failure classification enum (Retryable, Terminal, NonRetryable) |
| `kafpy.FailureReason` | Structured failure details with category and description |
| `kafpy.KafPyError` | Base exception (all errors inherit from this) |
| `kafpy.ConsumerError` | Consumer-level errors |
| `kafpy.HandlerError` | Handler processing errors |
| `kafpy.ConfigurationError` | Configuration errors |

## Exception Import

```python
from kafpy.exceptions import KafPyError, ConsumerError, HandlerError, ConfigurationError
```

## Configuration Import

```python
from kafpy.config import ConsumerConfig, RetryConfig, ObservabilityConfig, FailureCategory, FailureReason
```

### ConsumerConfig

Frozen dataclass with the following fields:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `bootstrap_servers` | `str` | — | Comma-separated Kafka broker addresses |
| `group_id` | `str` | — | Consumer group ID |
| `topics` | `list[str]` | — | Topics to subscribe to |
| `auto_offset_reset` | `str` | `"earliest"` | Starting offset (`"earliest"`, `"latest"`) |
| `enable_auto_commit` | `bool` | `False` | Enable automatic offset commits |
| `session_timeout_ms` | `int` | `30000` | Session timeout in milliseconds |
| `heartbeat_interval_ms` | `int` | `3000` | Heartbeat interval in milliseconds |
| `max_poll_interval_ms` | `int` | `300000` | Maximum poll interval in milliseconds |
| `security_protocol` | `str \| None` | `None` | Security protocol |
| `sasl_mechanism` | `str \| None` | `None` | SASL mechanism |
| `sasl_username` | `str \| None` | `None` | SASL username |
| `sasl_password` | `str \| None` | `None` | SASL password |
| `fetch_min_bytes` | `int` | `1` | Minimum bytes per fetch request |
| `max_partition_fetch_bytes` | `int` | `1048576` | Maximum bytes per partition |
| `partition_assignment_strategy` | `str` | `"roundrobin"` | Partition assignment strategy |
| `retry_backoff_ms` | `int` | `100` | Retry backoff in milliseconds |
| `message_batch_size` | `int` | `100` | Messages per batch |
| `default_retry_policy` | `RetryConfig \| None` | `None` | Retry policy for failed messages |
| `dlq_topic_prefix` | `str \| None` | `"dlq."` | DLQ topic name prefix |
| `drain_timeout_secs` | `int \| None` | `30` | Graceful shutdown drain timeout |
| `num_workers` | `int \| None` | `4` | Worker thread count |
| `enable_auto_offset_store` | `bool \| None` | `False` | Auto offset store after processing |
| `observability_config` | `ObservabilityConfig \| None` | `None` | OTLP/metrics configuration |

### RetryConfig

```python
RetryConfig(
    max_attempts=3,       # Maximum retry attempts
    base_delay_ms=100,    # Initial backoff delay in ms
    max_delay_ms=10000,   # Maximum backoff delay in ms
    jitter_factor=0.25,   # Jitter factor (0.0–1.0)
)
```

### ObservabilityConfig

```python
ObservabilityConfig(
    otlp_endpoint=None,    # OTLP collector endpoint URL
    service_name="kafpy",  # Service name for tracing/metrics
    sampling_ratio=0.1,    # Trace sampling ratio (0.0–1.0)
    log_format="text",     # Log format: "text" or "json"
)
```

### FailureCategory

```python
FailureCategory.Retryable      # Transient — retry with backoff
FailureCategory.Terminal        # Permanent — send to DLQ immediately
FailureCategory.NonRetryable    # Non-retryable, not DLQ-bound
```

### FailureReason

```python
FailureReason(
    category=FailureCategory.Retryable,  # Failure category
    description="Connection timeout",     # Human-readable description
)
```

## Handler Types Import

```python
from kafpy.handlers import KafkaMessage, HandlerContext, HandlerResult, FailureCategory, FailureReason, register_handler
```

### KafkaMessage

| Attribute | Type | Description |
|-----------|------|-------------|
| `topic` | `str` | Topic name |
| `partition` | `int` | Partition number |
| `offset` | `int` | Message offset |
| `key` | `bytes \| None` | Message key |
| `payload` | `bytes \| None` | Message payload |
| `headers` | `list[tuple[str, bytes \| None]]` | Message headers |
| `timestamp_millis` | `int \| None` | Message timestamp in milliseconds |

**Methods:**
- `get_key() -> bytes | None` — Get message key
- `get_headers() -> list[tuple[str, bytes | None]]` — Get message headers
- `get_timestamp_millis() -> int | None` — Get message timestamp in milliseconds