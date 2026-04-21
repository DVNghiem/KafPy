# Getting Started

## Basic Usage

```python
import kafpy

# Create consumer configuration
config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="my-group",
    topics=["my-topic"],
)

# Create consumer and runtime
consumer = kafpy.Consumer(config)
app = kafpy.KafPy(consumer)

# Register handler
@app.handler(topic="my-topic")
def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
    print(f"Received: {msg.get_payload_as_string()}")
    return kafpy.HandlerResult(action="ack")

# Start consuming
app.run()
```

## Handler Types

KafPy supports multiple handler modes:

### Sync Handler

```python
def sync_handler(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext):
    return kafpy.HandlerResult(action="ack")
```

### Async Handler

```python
async def async_handler(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext):
    return kafpy.HandlerResult(action="ack")
```

### Batch Handler (Sync)

```python
def batch_handler(messages: list[kafpy.KafkaMessage], ctx: kafpy.HandlerContext):
    for msg in messages:
        print(msg.get_payload_as_string())
    return kafpy.HandlerResult(action="ack")
```

## Message Types

### KafkaMessage

| Attribute | Type | Description |
|-----------|------|-------------|
| `topic` | `str` | Kafka topic name |
| `partition` | `int` | Partition number |
| `offset` | `int` | Message offset |
| `key` | `bytes \| None` | Message key |
| `payload` | `bytes \| None` | Message payload |
| `headers` | `list[tuple[str, bytes \| None]]` | Message headers |
| `timestamp` | `int` | Timestamp in milliseconds |

### Methods

- `get_key_as_string() -> str | None` - Decode key as UTF-8
- `get_payload_as_string() -> str | None` - Decode payload as UTF-8