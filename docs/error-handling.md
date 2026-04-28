# Error Handling

## Exception Types

KafPy provides exception types in `kafpy.exceptions`:

```python
from kafpy.exceptions import (
    KafPyError,       # Base exception
    ConsumerError,    # Consumer-related errors
    HandlerError,    # Handler execution errors
    ConfigurationError,  # Configuration errors
)
```

## Failure Classification

KafPy classifies processing failures using a structured taxonomy:

### FailureCategory

```python
from kafpy.handlers import FailureCategory

# Three failure categories:
FailureCategory.Retryable     # Transient failures — retry with backoff
FailureCategory.Terminal      # Permanent failures — send to DLQ
FailureCategory.NonRetryable  # Non-retryable but not DLQ-bound
```

| Category | Retry? | DLQ? | Description |
|----------|--------|------|-------------|
| `Retryable` | Yes | Only after exhausting retries | Transient errors (network timeout, rate limit) |
| `Terminal` | No | Yes, immediately | Permanent errors (schema violation, invalid data) |
| `NonRetryable` | No | No | Skipped or logged (duplicate, stale event) |

### FailureReason

`FailureReason` pairs a category with a human-readable description:

```python
from kafpy.handlers import FailureCategory, FailureReason

reason = FailureReason(
    category=FailureCategory.Retryable,
    description="Connection timeout to downstream service",
)
print(reason.category)     # FailureCategory.Retryable
print(reason.description)  # "Connection timeout to downstream service"
```

| Field | Type | Description |
|-------|------|-------------|
| `category` | `FailureCategory` | The failure category (Retryable, Terminal, or NonRetryable) |
| `description` | `str` | Human-readable description of the failure |

## Dead Letter Queue

Messages that fail after all retry attempts, or are classified as terminal failures, can be routed to a DLQ topic.

### DLQ Configuration

Configure the DLQ topic prefix via `ConsumerConfig`:

```python
from kafpy import ConsumerConfig

config = ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="my-group",
    topics=["my-topic"],
    dlq_topic_prefix="dlq.",  # Failed messages go to dlq.<original-topic>
)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `dlq_topic_prefix` | str \| None | `"dlq."` | Prefix prepended to original topic name |

For example, a message from topic `"orders"` that fails will be sent to `"dlq.orders"`.

### DLQ with Failure Classification

```python
from kafpy.handlers import FailureCategory, FailureReason

@app.handler(topic="my-topic")
def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext):
    try:
        risky_operation(msg)
        return kafpy.HandlerResult(action="ack")
    except PermanentFailure as e:
        reason = FailureReason(
            category=FailureCategory.Terminal,
            description=str(e),
        )
        return kafpy.HandlerResult(action="dlq")
    except TemporaryFailure as e:
        reason = FailureReason(
            category=FailureCategory.Retryable,
            description=str(e),
        )
        return kafpy.HandlerResult(action="nack")
```

### DLQ with Retry Policy

Configure retry behavior alongside DLQ:

```python
from kafpy import ConsumerConfig, RetryConfig

config = ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="my-group",
    topics=["my-topic"],
    default_retry_policy=RetryConfig(
        max_attempts=3,
        base_delay_ms=100,
        max_delay_ms=10000,
        jitter_factor=0.25,
    ),
    dlq_topic_prefix="dlq.",
)
```

Messages that exhaust all retry attempts are automatically routed to the DLQ.

## Graceful Shutdown

```python
import signal
import sys

def signal_handler(signum, frame):
    print("Shutdown signal received, stopping consumer...")
    app.stop()
    sys.exit(0)

signal.signal(signal.SIGINT, signal_handler)
signal.signal(signal.SIGTERM, signal_handler)

app.run()
```

The `drain_timeout_secs` configuration parameter controls how long the consumer waits for in-flight messages to complete during shutdown:

```python
from kafpy import ConsumerConfig

config = ConsumerConfig(
    # ... other params ...
    drain_timeout_secs=30,  # Wait up to 30 seconds for in-flight messages
)
```