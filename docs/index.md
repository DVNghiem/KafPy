# KafPy

High-performance Kafka consumer & producer for Python, built with Rust using PyO3.

## Features

- **High Performance**: Built in Rust for maximum throughput and low latency
- **Async Support**: Full support for async/await handlers
- **Batch Processing**: Efficient batch message handling
- **Type Safe**: Full type annotations for Python
- **Retry & DLQ**: Built-in retry logic and dead letter queue support
- **Flexible Routing**: Multiple routing strategies (pattern, header, key, python)

## Quick Start

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

app.run()
```

## Installation

See the [Installation Guide](installation.md) for detailed setup instructions.

## Documentation

- [Getting Started](getting-started.md) - First steps with KafPy
- [Configuration](configuration.md) - All configuration options
- [Handlers](handlers.md) - Handler types and registration
- [Consumer](consumer.md) - Consumer usage
- [Error Handling](error-handling.md) - Retry and DLQ
- [Routing](routing.md) - Routing strategies
- [Benchmark](benchmark.md) - Performance benchmarking
- [API Reference](api/kafpy.md) - Complete API documentation

## License

BSD-3-Clause