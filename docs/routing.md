# Routing

## Routing Modes

KafPy supports multiple routing strategies:

| Mode | Description |
|------|-------------|
| `"default"` | Round-robin across handlers |
| `"pattern"` | Topic pattern matching |
| `"header"` | Route by message header |
| `"key"` | Route by message key |
| `"python"` | Custom Python routing logic |

## Pattern Routing

```python
routing = kafpy.RoutingConfig(
    routing_mode="pattern",
)

@app.handler(topic="orders.*", routing=routing)
def handle_orders(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext):
    return kafpy.HandlerResult(action="ack")
```

## Header Routing

```python
routing = kafpy.RoutingConfig(
    routing_mode="header",
)

@app.handler(topic="events", routing=routing)
def handle_events(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext):
    priority = ctx.headers.get("priority", "normal")
    # Route based on priority
    return kafpy.HandlerResult(action="ack")
```

## Key Routing

```python
routing = kafpy.RoutingConfig(
    routing_mode="key",
)

@app.handler(topic="partitioned", routing=routing)
def handle_partitioned(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext):
    key = msg.get_key_as_string()
    # Route based on key
    return kafpy.HandlerResult(action="ack")
```