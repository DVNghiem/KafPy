# Best Practices

Practical guidance for building reliable, performant, and maintainable Kafka consumers with KafPy.

## Connection Management

### Use Context Managers for Consumer Lifecycles

Always initialize consumers within a context manager to ensure clean shutdown:

```python
import kafpy

config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="my-group",
    topics=["my-topic"],
)

with kafpy.Consumer(config) as consumer:
    app = kafpy.KafPy(consumer)

    @app.handler(topic="my-topic")
    def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
        return kafpy.HandlerResult(action="ack")

    app.run()
```

The context manager guarantees `stop()` is called on exit, triggering the 4-phase shutdown lifecycle (Running → Draining → Finalizing → Done).

### Configure Session and Heartbeat Timeouts Appropriately

Match these values to your workload characteristics:

```python
config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="my-group",
    topics=["my-topic"],
    session_timeout_ms=45000,       # Long-running handlers need higher values
    heartbeat_interval_ms=3000,      # Keep below session_timeout_ms / 3
    max_poll_interval_ms=600000,    # 10 minutes — allow for slow batch processing
)
```

If handlers frequently fail with rebalancing errors, the session timeout is too low for your processing time.

### Set `enable_auto_commit=False` and Commit Explicitly

KafPy does not auto-commit offsets by default. For at-least-once delivery semantics, always return `HandlerResult(action="ack")` to commit after successful processing:

```python
@app.handler(topic="my-topic")
def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
    try:
        process_message(msg)
        return kafpy.HandlerResult(action="ack")  # commits offset
    except Exception:
        return kafpy.HandlerResult(action="nack")  # triggers retry/DLQ
```

### Pin Bootstrap Servers to a Subset

For large clusters, specify a subset of brokers rather than all servers:

```python
config = kafpy.ConsumerConfig(
    bootstrap_servers="kafka-1:9092,kafka-2:9092,kafka-3:9092",
    # ...
)
```

The client discovers the full cluster metadata from these initial connections.

---

## Error Handling

### Classify Failures with FailureCategory

Inspect failure reasons to route messages correctly:

```python
from kafpy.config import FailureCategory

def classify_error(reason: FailureCategory) -> str:
    if reason.category == FailureCategory.Retryable:
        return "retry"     # Transient — network timeout, broker unavailable
    elif reason.category == FailureCategory.Terminal:
        return "dlq"       # Poison message — malformed payload, deserialization failure
    else:
        return "nack"       # Non-retryable — validation error, business logic rejection
```

### Configure Retry Policies for Transient Failures

Use exponential backoff with jitter for retryable errors:

```python
config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="my-group",
    topics=["my-topic"],
    retry_policy=kafpy.RetryConfig(
        max_attempts=5,
        base_delay=0.5,         # 500ms base delay
        max_delay=30.0,         # Cap at 30 seconds
        jitter_factor=0.2,     # ±20% random jitter
    ),
)
```

### Set Handler Timeouts to Prevent Hangs

Configure `handler_timeout_ms` to cancel handlers that exceed expected duration:

```python
config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="my-group",
    topics=["my-topic"],
    handler_timeout_ms=30000,   # 30 second timeout per handler
)
```

A handler that exceeds this limit raises `HandlerError` and routes the message to DLQ or retry based on your retry policy.

### Decode Payloads Safely

Use the typed accessors which raise `HandlerError` on decode failure:

```python
@app.handler(topic="my-topic")
def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
    try:
        # Raises HandlerError if payload is not valid UTF-8
        payload = msg.get_payload_as_string()
        if payload is None:
            return kafpy.HandlerResult(action="dlq")  # empty payload is terminal
        data = json.loads(payload)
        return kafpy.HandlerResult(action="ack")
    except json.JSONDecodeError:
        return kafpy.HandlerResult(action="dlq")  # malformed JSON is terminal
    except HandlerError:
        return kafpy.HandlerResult(action="dlq")  # encoding error is terminal
```

Accessing `.key` when it is `None` returns `None` silently — no exception is raised.

---

## Handler Patterns

### Prefer Async Handlers for I/O-Bound Workloads

Async handlers avoid blocking the worker thread during external calls:

```python
import asyncio
import aiohttp

@app.handler(topic="my-topic")
async def handle_async(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
    async with aiohttp.ClientSession() as session:
        async with session.get(f"http://api.example.com/data/{msg.key}") as resp:
            data = await resp.json()
            await process(data)
    return kafpy.HandlerResult(action="ack")
```

### Use Batch Handlers for High-Throughput Workloads

Batch handlers accumulate messages and process them in bulk, reducing per-message overhead:

```python
@app.handler(
    topic="my-topic",
    batch=True,
    batch_max_size=100,
    batch_max_wait_ms=500,   # Wait up to 500ms for a full batch
)
def handle_batch(messages: list[kafpy.KafkaMessage], ctx) -> kafpy.HandlerResult:
    records = [json.loads(msg.get_payload_as_string()) for msg in messages]
    bulk_insert(records)
    return kafpy.HandlerResult(action="ack")
```

### Fan-Out for Parallel Processing Pipelines

Route a single message to multiple downstream topics concurrently:

```python
consumer = kafpy.Consumer(config)
builder = consumer.register_fanout(
    group_name="enrichment",
    sink_topics=["enriched-topic-a", "enriched-topic-b"],
    handler=enrich_handler,
)
registration = builder.max_fan_out(8).register()
```

The `max_fan_out(8)` caps concurrent sink branches to 8. Default is 4, maximum is 64.

### Fan-In for Aggregating Multiple Topics

Merge messages from multiple source topics into a single handler:

```python
consumer = kafpy.Consumer(config)
reg = consumer.register_fanin(
    handler_key="aggregator",
    sources=["topic-a", "topic-b", "topic-c"],
    handler=aggregate_handler,
)
```

Messages from all sources are delivered in round-robin order to `aggregate_handler`.

---

## Resource Cleanup

### Always Stop the Consumer Before Exit

If you are not using a context manager, call `stop()` explicitly:

```python
consumer = kafpy.Consumer(config)
app = kafpy.KafPy(consumer)
app.start()

try:
    # ... work ...
finally:
    consumer.stop()   # initiates graceful drain
```

### Configure Drain Timeout for Graceful Shutdown

Set `drain_timeout_secs` to match your longest expected handler duration:

```python
config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="my-group",
    topics=["my-topic"],
    drain_timeout_secs=60,   # Wait up to 60s for in-flight messages to complete
)
```

If drain times out, in-flight messages are not committed and will be reprocessed on restart.

### Release Resources in Middleware

If your middleware acquires resources (connections, file handles), clean them up:

```python
class DBConnectionMiddleware(kafpy.BaseMiddleware):
    def __init__(self):
        self.conn = None

    def before(self, ctx):
        self.conn = db.connect()

    def after(self, ctx, result, elapsed_ms):
        if self.conn:
            self.conn.close()
            self.conn = None

    def on_error(self, ctx, result):
        if self.conn:
            self.conn.close()
            self.conn = None
```

---

## Testing Strategies

### Test Handlers in Isolation

Mock `KafkaMessage` and `HandlerContext` to test handler logic without Kafka:

```python
import pytest
from kafpy.handlers import KafkaMessage, HandlerContext, HandlerResult

def test_my_handler_decodes_json():
    msg = KafkaMessage(
        topic="test-topic",
        partition=0,
        offset=100,
        key=b"key-1",
        payload=b'{"id": 1, "name": "Alice"}',
        headers=[],
    )
    ctx = HandlerContext(
        topic="test-topic",
        partition=0,
        offset=100,
        timestamp=0,
        headers={},
    )

    result = my_handler(msg, ctx)

    assert result.action == "ack"
```

### Use Table-Driven Tests for Handler Variants

```python
@pytest.mark.parametrize("payload,expected_action", [
    (b'{"id": 1}', "ack"),
    (b'{"id": 2}', "ack"),
    (b'not json', "dlq"),
    (b'{"id": null}', "dlq"),
])
def test_handler_routes_correctly(payload, expected_action):
    msg = KafkaMessage("test-topic", 0, 0, None, payload, [])
    result = my_handler(msg, mock_ctx)
    assert result.action == expected_action
```

### Test Error Paths Explicitly

```python
def test_handler_raises_handler_error_on_invalid_utf8():
    msg = KafkaMessage(
        topic="test-topic",
        partition=0,
        offset=0,
        key=b"\xff\xfe invalid",
        payload=b"valid",
        headers=[],
    )
    with pytest.raises(HandlerError) as exc_info:
        msg.get_key_as_string()
    assert exc_info.value.partition == 0
    assert exc_info.value.topic == "test-topic"
```

### Test with Real Kafka for Integration

Use Testcontainers or a local Kafka for integration tests:

```python
import pytest
from kafpy import ConsumerConfig, Consumer, KafPy

@pytest.fixture
def kafka_consumer():
    config = ConsumerConfig(
        bootstrap_servers="localhost:9092",
        group_id="test-group",
        topics=["test-topic"],
    )
    consumer = Consumer(config)
    yield consumer
    consumer.stop()
```

---

## Security Considerations

### Protect SASL Credentials

Never hardcode credentials. Load from environment variables:

```python
import os

config = kafpy.ConsumerConfig(
    bootstrap_servers=os.environ["KAFKA_BOOTSTRAP_SERVERS"],
    group_id="my-group",
    topics=["my-topic"],
    security_protocol="SASL_SSL",
    sasl_mechanism="SCRAM-SHA-512",
    sasl_username=os.environ["KAFKA_USERNAME"],
    sasl_password=os.environ["KAFKA_PASSWORD"],
)
```

Add credentials to `.env` (gitignored) during development. In production, use a secrets manager (AWS Secrets Manager, HashiCorp Vault).

### Validate Message Payloads Before Processing

Treat all incoming messages as untrusted input:

```python
import pydantic

class MyEvent(pydantic.BaseModel):
    id: int
    name: str

@app.handler(topic="my-topic")
def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
    try:
        payload = json.loads(msg.get_payload_as_string())
        event = MyEvent.model_validate(payload)
        process(event)
        return kafpy.HandlerResult(action="ack")
    except (json.JSONDecodeError, pydantic.ValidationError):
        return kafpy.HandlerResult(action="dlq")
```

### Limit Consumer Permissions with SASL

Use the minimum required permissions for consumer credentials. A consumer should only be able to read from its subscribed topics and write to its DLQ topic.

### Do Not Log Message Payloads in Production

Message payloads may contain sensitive data (PII, tokens). Log only message metadata (topic, partition, offset, key) during normal operation:

```python
@app.handler(topic="my-topic")
def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
    logger.info(f"Processing message key={msg.key} offset={ctx.offset}")
    # NOT: logger.info(f"Processing payload={msg.payload}")
    process(msg)
    return kafpy.HandlerResult(action="ack")
```

---

## Observability

### Enable Structured Logging

Configure the `kafpy` logger to emit structured logs:

```python
import logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s"
)
logging.getLogger("kafpy").setLevel(logging.DEBUG)
```

### Use Built-in Middleware for Metrics

Register `Logging()` and `Metrics()` middleware to automatically instrument handlers:

```python
@app.handler(
    topic="my-topic",
    middleware=[kafpy.Logging(), kafpy.Metrics()],
)
def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
    return kafpy.HandlerResult(action="ack")
```

`Metrics()` records `kafpy.handler.latency` histogram and `kafpy.message.throughput` counter per handler invocation.

### Configure OTLP Tracing

For distributed tracing across services:

```python
config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="my-group",
    topics=["my-topic"],
    observability_config=kafpy.ObservabilityConfig(
        otlp_endpoint="http://localhost:4317",
        service_name="my-consumer",
        sampling_ratio=0.1,   # Sample 10% of messages in production
        log_format="json",
    ),
)
```

### Monitor Consumer Lag

Use `consumer.status()` to inspect consumer health:

```python
status = consumer.status()
print(status["consumer_lag_summary"])
print(status["worker_states"])
```

Call `status()` periodically (e.g., via a health check endpoint) rather than on every message.

---

## Performance Tips

### Set `fetch_min_bytes` for Throughput

Increase batch efficiency by raising the minimum fetch size:

```python
config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="my-group",
    topics=["my-topic"],
    fetch_min_bytes=1024 * 10,   # Wait for at least 10KB before returning
)
```

Lower values reduce latency but increase broker round-trips.

### Tune `num_workers` for CPU-Bound Workloads

Increase worker threads for processing-intensive workloads:

```python
config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="my-group",
    topics=["my-topic"],
    num_workers=8,   # 8 concurrent worker threads
)
```

Keep `num_workers` below the number of CPU cores to avoid context-switching overhead.

### Avoid Mutating `KafkaMessage` Fields

`KafkaMessage` is a frozen dataclass — its fields are immutable. Extract data you need and work with copies:

```python
@app.handler(topic="my-topic")
def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
    payload = msg.get_payload_as_string()  # Extract once
    data = json.loads(payload)              # Work with parsed data
    # ...
    return kafpy.HandlerResult(action="ack")
```
