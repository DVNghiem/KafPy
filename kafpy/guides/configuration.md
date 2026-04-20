# Configuration

KafPy uses instance-based configuration. All config objects are frozen dataclasses.

## ConsumerConfig

```python
config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="my-group",
    topics=["my-topic"],
    auto_offset_reset="earliest",
    enable_auto_commit=False,
)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| bootstrap_servers | str | required | Comma-separated broker addresses |
| group_id | str | required | Consumer group ID |
| topics | list[str] | required | Topics to subscribe to |
| auto_offset_reset | str | "earliest" | Where to start ("earliest" or "latest") |
| enable_auto_commit | bool | False | Auto-commit offsets |
| session_timeout_ms | int | 30000 | Session timeout |
| heartbeat_interval_ms | int | 3000 | Heartbeat interval |
| max_poll_interval_ms | int | 300000 | Max poll interval |

## RoutingConfig

```python
routing = kafpy.RoutingConfig(
    routing_mode="pattern",
    fallback_handler="default",
)
```

## RetryConfig

```python
retry = kafpy.RetryConfig(
    max_attempts=3,
    base_delay=1.0,
    max_delay=60.0,
    jitter_factor=0.1,
)
```

## BatchConfig

```python
batch = kafpy.BatchConfig(
    max_batch_size=100,
    max_batch_timeout_ms=1000,
)
```

## ConcurrencyConfig

```python
concurrency = kafpy.ConcurrencyConfig(num_workers=4)
```