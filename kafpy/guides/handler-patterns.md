# Handler Patterns

KafPy supports three handler types: sync, async, and batch.

## Sync Handler

A plain Python function receives each message individually:

```python
@app.handler(topic="events")
def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
    print(f"Received: {msg.payload}")
    return kafpy.HandlerResult(action="ack")
```

## Async Handler

An `async def` function handles messages concurrently:

```python
@app.handler(topic="events")
async def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
    result = await process_message(msg.payload)
    return kafpy.HandlerResult(action="ack")
```

## Batch Handler

A generator function receives batches of messages:

```python
@app.handler(topic="events", routing=kafpy.RoutingConfig(routing_mode="python"))
async def handle_batch(messages: list[kafpy.KafkaMessage], ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
    for msg in messages:
        print(f"Batch item: {msg.payload}")
    return kafpy.HandlerResult(action="ack")
```

## Error Handling in Handlers

Raise a `HandlerError` for Python-side errors:

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

The runtime catches `HandlerError` and treats the handler as failed, triggering retry or DLQ.