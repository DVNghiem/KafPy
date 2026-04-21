# Best Practices

## Performance

### Batch Processing

For high-throughput scenarios, use batch handlers:

```python
@app.handler(topic="high-volume", mode="batch")
def handle_batch(messages: list[kafpy.KafkaMessage], ctx: kafpy.HandlerContext):
    # Process messages in bulk
    results = bulk_process([msg.payload for msg in messages])
    return kafpy.HandlerResult(action="ack")
```

### Connection Pooling

Reuse consumer instances:

```python
# GOOD: Single consumer instance
consumer = kafpy.Consumer(config)
app = kafpy.KafPy(consumer)

# BAD: Creating new consumer per message
def handler(msg, ctx):
    consumer = kafpy.Consumer(config)  # Don't do this!
```

### Worker Configuration

```python
concurrency = kafpy.ConcurrencyConfig(
    num_workers=4,  # Match to CPU cores for CPU-bound
    # For I/O-bound, can go higher
)
```

## Reliability

### Graceful Shutdown

```python
import signal
import sys

class ShutdownManager:
    def __init__(self, app):
        self.app = app
        self.shutdown_requested = False

    def setup(self):
        signal.signal(signal.SIGINT, self.handle_signal)
        signal.signal(signal.SIGTERM, self.handle_signal)

    def handle_signal(self, signum, frame):
        print("Shutdown signal received...")
        self.app.stop()
        sys.exit(0)

manager = ShutdownManager(app)
manager.setup()
app.run()
```

### Idempotent Handlers

Design handlers to be safe for reprocessing:

```python
@app.handler(topic="orders")
def handle_order(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext):
    order_id = extract_order_id(msg)

    # Idempotent: Check if already processed
    if order_already_processed(order_id):
        return kafpy.HandlerResult(action="ack")

    process_order(msg)
    mark_order_processed(order_id)
    return kafpy.HandlerResult(action="ack")
```

### Dead Letter Queue

Always route unprocessable messages to DLQ:

```python
@app.handler(topic="critical")
def handle_critical(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext):
    try:
        process_critical_data(msg)
        return kafpy.HandlerResult(action="ack")
    except UnrecoverableError:
        # Route to DLQ for manual review
        return kafpy.HandlerResult(action="dlq")
    except RecoverableError:
        # Will be retried
        return kafpy.HandlerResult(action="nack")
```

## Security

### Environment Variables

Never hardcode credentials:

```python
import os
from dotenv import load_dotenv

load_dotenv()

config = kafpy.ConsumerConfig(
    bootstrap_servers=os.environ["KAFKA_BROKERS"],
    group_id=os.environ["KAFKA_GROUP_ID"],
    sasl_username=os.environ["KAFKA_USERNAME"],
    sasl_password=os.environ["KAFKA_PASSWORD"],
    security_protocol="SASL_SSL",
    sasl_mechanism="SCRAM-SHA-512",
)
```

### TLS/SSL

Always use TLS in production:

```python
config = kafpy.ConsumerConfig(
    bootstrap_servers="secure-kafka.example.com:9093",
    security_protocol="SSL",
    # For custom CA certificates:
    # ssl_ca_location="/path/to/ca.crt",
)
```

## Monitoring

### Metrics

```python
# Configure observability
observability_config = kafpy.ObservabilityConfig(
    otlp_endpoint="http://otel-collector:4317",
    service_name="kafpy-consumer",
    sampling_ratio=1.0,  # 100% sampling for critical services
)
```

### Health Checks

```python
from fastapi import FastAPI

app = FastAPI()

@app.get("/health")
def health_check():
    return {
        "status": "healthy",
        "consumer_running": not kafpy_app._consumer._stopping,
        "handlers_registered": len(kafpy_app._handlers),
    }
```

## Error Handling

### Structured Errors

```python
from kafpy.exceptions import HandlerError

class ProcessingError(HandlerError):
    pass

@app.handler(topic="data")
def handle_data(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext):
    try:
        validate_message(msg)
        process_message(msg)
    except ValidationError as e:
        raise HandlerError(
            message=f"Validation failed: {e}",
            error_code="VALIDATION_ERROR",
            partition=ctx.partition,
            topic=ctx.topic,
        )
```

### Retry Budget

```python
retry = kafpy.RetryConfig(
    max_attempts=5,
    base_delay=0.5,    # Start with 500ms
    max_delay=30.0,   # Cap at 30 seconds
    jitter_factor=0.2,  # 20% jitter to prevent thundering herd
)
```

## Testing

### Unit Testing Handlers

```python
import pytest

def test_handler_success():
    msg = kafpy.KafkaMessage(
        topic="test",
        partition=0,
        offset=1,
        key=b"key",
        payload=b"test data",
        headers=[],
        timestamp=1234567890,
    )
    ctx = kafpy.HandlerContext(
        topic="test",
        partition=0,
        offset=1,
        timestamp=1234567890,
        headers={},
    )

    result = my_handler(msg, ctx)
    assert result.action == "ack"

def test_handler_failure():
    msg = create_failing_message()
    ctx = create_context()

    result = my_handler(msg, ctx)
    assert result.action == "nack" or result.action == "dlq"
```

### Integration Testing

```python
@pytest.fixture
def test_consumer():
    config = kafpy.ConsumerConfig(
        bootstrap_servers="localhost:9092",
        group_id="test-group",
        topics=["test-topic"],
    )
    consumer = kafpy.Consumer(config)
    app = kafpy.KafPy(consumer)

    # Setup test data
    produce_test_messages()

    yield app

    # Cleanup
    app.stop()
    cleanup_test_data()
```

## Production Checklist

- [ ] Use TLS/SSL for all connections
- [ ] Store secrets in environment variables
- [ ] Configure proper retry limits
- [ ] Implement DLQ for failed messages
- [ ] Add health check endpoints
- [ ] Set up metrics/observability
- [ ] Configure graceful shutdown
- [ ] Use idempotent handlers
- [ ] Test with production-like data volumes
- [ ] Monitor consumer lag
- [ ] Set appropriate worker count
- [ ] Use batch processing for high-volume topics