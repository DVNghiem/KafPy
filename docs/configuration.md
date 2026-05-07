# Configuration

## ConsumerConfig

Main configuration for the Kafka consumer.

```python
config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="my-group",
    topics=["my-topic"],
    auto_offset_reset="earliest",
    enable_auto_commit=False,
    session_timeout_ms=30000,
    heartbeat_interval_ms=3000,
    max_poll_interval_ms=300000,
    security_protocol=None,
    sasl_mechanism=None,
    sasl_username=None,
    sasl_password=None,
    fetch_min_bytes=1,
    max_partition_fetch_bytes=1048576,
    partition_assignment_strategy="roundrobin",
    retry_backoff_ms=100,
    # Retry & DLQ
    retry_policy=None,
    dlq_topic_prefix="dlq.",
    drain_timeout_secs=30,
    num_workers=4,
    enable_auto_offset_store=False,
    # Observability
    observability_config=None,
    handler_timeout_ms=None,
)
```

## Retry Configuration

Configure retry behavior for failed message processing:

```python
from kafpy import ConsumerConfig, RetryConfig

config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="my-group",
    topics=["my-topic"],
    retry_policy=RetryConfig(
        max_attempts=3,        # Maximum retry attempts
        base_delay=0.1,       # Initial backoff delay in seconds
        max_delay=10.0,       # Maximum backoff delay in seconds
        jitter_factor=0.25,   # Jitter factor (0.0–1.0)
    ),
)
```

### RetryConfig Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `max_attempts` | int | `3` | Maximum number of retry attempts before sending to DLQ |
| `base_delay` | float | `0.1` | Base delay in seconds for exponential backoff |
| `max_delay` | float \| None | `None` | Cap on backoff delay in seconds (`None` defaults to 30s) |
| `jitter_factor` | float | `0.25` | Random jitter factor (0.0–1.0) to spread retries |

## DLQ Configuration

Dead Letter Queue routing for messages that exhaust retries or are classified as terminal failures:

```python
from kafpy import ConsumerConfig

config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="my-group",
    topics=["my-topic"],
    dlq_topic_prefix="dlq.",  # Failed messages go to dlq.<original-topic>
)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `dlq_topic_prefix` | str \| None | `"dlq."` | Prefix prepended to original topic name for DLQ topic |

## Observability Configuration

Enable OTLP tracing, metrics export, and structured logging:

```python
from kafpy import ConsumerConfig, ObservabilityConfig

config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="my-group",
    topics=["my-topic"],
    observability_config=ObservabilityConfig(
        otlp_endpoint="http://localhost:4317",
        service_name="my-service",
        sampling_ratio=0.1,
        log_format="json",
    ),
)
```

### ObservabilityConfig Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `otlp_endpoint` | str \| None | `None` | OTLP collector endpoint URL (e.g. `http://localhost:4317`) |
| `service_name` | str | `"kafpy"` | Service name for tracing and metrics |
| `sampling_ratio` | float | `0.1` | Trace sampling ratio (0.0–1.0) |
| `log_format` | str | `"pretty"` | Log format: `"pretty"`, `"json"`, or `"simple"` |

## Worker & Drain Configuration

Control concurrency and graceful shutdown behavior:

```python
from kafpy import ConsumerConfig

config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="my-group",
    topics=["my-topic"],
    num_workers=8,                # Number of worker threads
    drain_timeout_secs=60,       # Graceful shutdown drain timeout
    enable_auto_offset_store=True, # Auto store offsets after processing
)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `num_workers` | int \| None | `4` | Number of worker threads processing messages concurrently |
| `drain_timeout_secs` | int \| None | `30` | Seconds to wait for in-flight messages during shutdown |
| `enable_auto_offset_store` | bool \| None | `False` | Automatically store offsets after successful processing |

## Handler Timeout

The `handler_timeout_ms` field sets a maximum execution time for Python message handlers. If a handler runs longer than this threshold, it is cancelled and the message is treated as a `Terminal(HandlerPanic)` failure, then routed to the retry/DLQ mechanism.

Always set `handler_timeout_ms` lower than `max_poll_interval_ms` (default 300000ms / 5 minutes) to prevent the consumer from being kicked out of the group:

```python
config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="my-group",
    topics=["my-topic"],
    handler_timeout_ms=120000,       # Cancel handlers after 2 minutes
    max_poll_interval_ms=600000,     # Bump poll interval to 10 minutes
)
```

When unset (`None`), handlers can run indefinitely — which may cause long poll intervals and trigger rebalances.

## Environment Variables

Additional environment variables for retry, DLQ, worker, and observability settings:

| Variable | Description |
|----------|-------------|
| `KAFKA_DLQ_TOPIC_PREFIX` | Prefix for DLQ topic names (default: `dlq.`) |
| `KAFKA_DRAIN_TIMEOUT_SECS` | Graceful shutdown drain timeout in seconds (default: `30`) |
| `KAFKA_NUM_WORKERS` | Number of worker threads (default: `4`) |
| `KAFKA_ENABLE_AUTO_OFFSET_STORE` | Enable auto offset store (default: `false`) |
| `KAFPY_ROUTING_PY_CALLBACK_HANDLER` | Optional registered handler key used as Python routing callback when routing rules are enabled |

## Runtime wiring note

`ConsumerConfig` exposes a broad API. In the current runtime builder path, only a subset is explicitly threaded into Rust config assembly before consumer startup (for example brokers/group/topics, poll/session controls, retry policy, DLQ prefix, drain timeout). Other fields may be accepted on the Python surface and still evolve in wiring coverage across releases.

## Security

### SASL Authentication

```python
config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="my-group",
    topics=["my-topic"],
    security_protocol="SASL_SSL",
    sasl_mechanism="SCRAM-SHA-256",
    sasl_username="your_username",
    sasl_password="your_password",
)
```

### SSL

```python
config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="my-group",
    topics=["my-topic"],
    security_protocol="SSL",
)
```