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

## Retry Logic

Configure automatic retries:

```python
retry_config = kafpy.RetryConfig(
    max_attempts=3,
    base_delay=1.0,     # 1 second base delay
    max_delay=30.0,    # Maximum 30 seconds
    jitter_factor=0.1, # 10% random jitter
)
```

## Dead Letter Queue

Messages that fail after all retry attempts can be routed to a DLQ:

```python
from kafpy.exceptions import HandlerError

@app.handler(topic="my-topic")
def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext):
    try:
        risky_operation(msg)
        return kafpy.HandlerResult(action="ack")
    except PermanentFailure:
        return kafpy.HandlerResult(action="dlq")
    except RecoveryPossible:
        return kafpy.HandlerResult(action="nack")
```

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