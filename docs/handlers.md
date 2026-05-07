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

`HandlerResult` is available as a typed return shape for handler code readability:

| Action | Typical intent |
|--------|----------------|
| `"ack"` | Successful processing |
| `"nack"` | Retry-style intent |
| `"dlq"` | Terminal intent |

Current runtime behavior is driven by execution outcome (success vs raised exception/timeout).  
Use raised exceptions for failure paths that must trigger retry/DLQ classification.

## Context

HandlerContext provides metadata about the message:

```python
@dataclass(frozen=True)
class HandlerContext:
    topic: str           # Topic name
    partition: int       # Partition number
    offset: int          # Message offset
    timestamp: int       # Message timestamp in milliseconds
    headers: dict[str, str]  # Message headers
```

## Failure Classification

KafPy provides structured failure classification for handler errors:

### FailureCategory

```python
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
from kafpy.config import FailureReason
```

| Field | Type | Description |
|-------|------|-------------|
| `category` | `FailureCategory` | The failure category |
| `description` | `str` | Human-readable description of the failure |

### Usage Example

```python
from kafpy.config import FailureCategory, FailureReason

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
        raise RuntimeError(reason.description) from e
    except PermanentError as e:
        reason = FailureReason(
            category=FailureCategory.Terminal,
            description=f"Permanent failure: {e}",
        )
        raise RuntimeError(reason.description) from e
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
        raise
```

