# Consumer

## Creating a Consumer

```python
import kafpy

config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="my-group",
    topics=["topic1", "topic2"],
)

consumer = kafpy.Consumer(config)
```

## Consumer Methods

| Method | Description |
|--------|-------------|
| `start()` | Start consuming messages |
| `stop()` | Stop the consumer gracefully |
| `add_handler(topic, handler)` | Register a handler for a topic |
| `status()` | Get runtime snapshot dict with consumer state, offsets, and queue depths |
| `set_handler_timeout(ms)` | Set handler execution timeout in milliseconds (overrides config default) |

## Full Example

```python
import kafpy

config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="my-group",
    topics=["my-topic"],
)

consumer = kafpy.Consumer(config)
app = kafpy.KafPy(consumer)

@app.handler(topic="my-topic")
def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext):
    print(f"Received: {msg.get_payload_as_string()}")
    return kafpy.HandlerResult(action="ack")

# Start in background
app.start()

# Or block with run()
# app.run()
```

## Runtime Status

Use `status()` to inspect the running consumer:

```python
snapshot = consumer.status()
print(snapshot)
# {
#   "state": "running",
#   "assigned_partitions": [...],
#   "offsets": {...},
#   "queue_depths": {...},
# }
```

## Graceful Shutdown with Drain

The `drain_timeout_secs` configuration controls how long the consumer waits for in-flight messages during shutdown:

```python
config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="my-group",
    topics=["my-topic"],
    drain_timeout_secs=30,  # Wait up to 30s for in-flight messages
)
```

## Handler Timeout

The `handler_timeout_ms` config prevents Python handlers from running indefinitely. If a handler exceeds the timeout, it is cancelled and the message is treated as a `Terminal(HandlerPanic)` failure, then routed to retry or DLQ.

The default RDKafka `max_poll_interval_ms` is 300000ms (5 minutes). Set `handler_timeout_ms` lower than this to avoid being kicked out of the consumer group:

```python
config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="my-group",
    topics=["my-topic"],
    handler_timeout_ms=120000,  # 2 minutes — well below the 5-min default poll interval
)
```

You can also override the timeout per-handler at runtime:

```python
consumer.set_handler_timeout(60000)  # Override to 60s
```

When `handler_timeout_ms` is `None` (the default), handlers can run indefinitely — which risks exceeding `max_poll_interval_ms` and triggering a rebalance.