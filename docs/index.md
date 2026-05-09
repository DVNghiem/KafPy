# KafPy

KafPy is a Python library for building high-performance Kafka consumers. Built with Rust and PyO3, it combines Rust's throughput with a clean Python API for writing business logic.

## Key capabilities

- **Handler-based API** — register functions to process messages from any topic
- **Sync and async handlers** — use whichever model fits your code
- **Batch processing** — process messages in configurable batches for throughput
- **Retry and DLQ** — built-in exponential backoff with dead-letter queue routing
- **Middleware** — logging, metrics, and custom extensions via a simple interface
- **Prometheus metrics and OTLP tracing** — observability out of the box
- **Fan-out and fan-in** — route messages to multiple topics or aggregate multiple sources
- **Graceful shutdown** — drain in-flight messages before exiting

## Quick start

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
def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
    print(f"Received: {msg.key} @ {ctx.topic}:{ctx.partition}:{ctx.offset}")
    return kafpy.HandlerResult(action="ack")

app.run()
```

## Next steps

- [Installation](installation.md) — install from PyPI or build from source
- [Tutorial](tutorial.md) — build your first Kafka consumer with KafPy
