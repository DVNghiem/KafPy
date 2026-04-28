# Handlers

## Handler Registration

### Decorator Style

```python
@app.handler(topic="my-topic")
def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext):
    return kafpy.HandlerResult(action="ack")
```

### Explicit Registration

```python
def handle(msg, ctx):
    return kafpy.HandlerResult(action="ack")

app.register_handler("my-topic", handle)
```

## HandlerResult Actions

| Action | Description |
|--------|-------------|
| `"ack"` | Acknowledge message, commit offset |
| `"nack"` | Negative acknowledge, will be redelivered |
| `"dlq"` | Send to Dead Letter Queue |

## Context

HandlerContext provides metadata about the message:

```python
@dataclass(frozen=True)
class HandlerContext:
    topic: str           # Topic name
    partition: int       # Partition number
    offset: int          # Message offset
    timestamp_millis: int | None  # Message timestamp in milliseconds (or None if unavailable)
    headers: list[tuple[str, bytes | None]]  # Message headers
```

> **Note:** The `timestamp` field has been renamed to `timestamp_millis` and its type changed from `int` to `int | None` to handle cases where the Kafka message timestamp is unavailable.

## Failure Classification

KafPy provides structured failure classification for handler errors:

### FailureCategory

```python
from kafpy.handlers import FailureCategory
# or
from kafpy.config import FailureCategory
```

| Category | Description |
|----------|-------------|
| `FailureCategory.Retryable` | Transient failure that should be retried with backoff |
| `FailureCategory.Terminal` | Permanent failure that should go directly to DLQ |
| `FailureCategory.NonRetryable` | Failure that should not be retried but isn't DLQ-bound |

### FailureReason

A dataclass providing detailed failure information:

```python
from kafpy.handlers import FailureReason
# or
from kafpy.config import FailureReason
```

| Field | Type | Description |
|-------|------|-------------|
| `category` | `FailureCategory` | The failure category |
| `description` | `str` | Human-readable description of the failure |

### Usage Example

```python
from kafpy.handlers import FailureCategory, FailureReason

@app.handler(topic="my-topic")
def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext):
    try:
        process_message(msg)
        return kafpy.HandlerResult(action="ack")
    except TemporaryError as e:
        reason = FailureReason(
            category=FailureCategory.Retryable,
            description=f"Temporary failure: {e}",
        )
        return kafpy.HandlerResult(action="nack")
    except PermanentError as e:
        reason = FailureReason(
            category=FailureCategory.Terminal,
            description=f"Permanent failure: {e}",
        )
        return kafpy.HandlerResult(action="dlq")
```

## Error Handling

```python
@app.handler(topic="my-topic")
def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext):
    try:
        process_message(msg)
        return kafpy.HandlerResult(action="ack")
    except Exception as e:
        print(f"Error processing message: {e}")
        return kafpy.HandlerResult(action="nack")
```

