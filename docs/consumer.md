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