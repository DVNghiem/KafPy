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
    timestamp: int      # Message timestamp
    headers: list[tuple[str, bytes | None]]  # Message headers
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

