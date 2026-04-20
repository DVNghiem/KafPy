# Error Handling

KafPy has a structured exception hierarchy rooted at `KafPyError`.

## Exception Hierarchy

- `KafPyError` -- base for all KafPy errors
- `ConsumerError` -- Kafka errors, subscription issues, message receive failures
- `HandlerError` -- Python handler exceptions, wrong-type message field access
- `ConfigurationError` -- invalid config values, missing required fields

## Importing Exceptions

```python
from kafpy.exceptions import KafPyError, ConsumerError, HandlerError, ConfigurationError
```

## Raising HandlerError

```python
from kafpy.exceptions import HandlerError

@app.handler(topic="events")
def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
    if not msg.payload:
        raise HandlerError(
            message="empty payload",
            error_code=None,
            partition=msg.partition,
            topic=msg.topic,
        )
    return kafpy.HandlerResult(action="ack")
```

## KafkaMessage Type Errors

Accessing message fields with the wrong type raises `HandlerError`:

```python
# If key is bytes, calling get_key_as_string() returns None silently
key = msg.get_key_as_string()  # returns str or None

# If payload is not valid UTF-8, raises HandlerError
text = msg.get_payload_as_string()  # raises HandlerError if not UTF-8
```