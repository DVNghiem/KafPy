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
    message_batch_size=100,
)
```

## Configuration Classes

### RoutingConfig

```python
routing = kafpy.RoutingConfig(
    routing_mode="default",  # "default", "pattern", "header", "key", "python"
    fallback_handler=None,
)
```

### RetryConfig

```python
retry = kafpy.RetryConfig(
    max_attempts=3,
    base_delay=1.0,
    max_delay=None,
    jitter_factor=None,
)
```

### BatchConfig

```python
batch = kafpy.BatchConfig(
    max_batch_size=100,
    max_batch_timeout_ms=1000,
)
```

### ConcurrencyConfig

```python
concurrency = kafpy.ConcurrencyConfig(
    num_workers=4,
)
```

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