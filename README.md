# KafPy

[![Python](https://img.shields.io/badge/python-3.11%2B-blue)](https://www.python.org/)
[![Rust](https://img.shields.io/badge/rust-latest-orange)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-BSD--3--Clause-green)](LICENSE)

High-performance Kafka runtime for Python, backed by Rust + PyO3.

## Installation

### Prerequisites

- Python 3.11+
- Rust toolchain
- `librdkafka`

### Build from source

```bash
git clone https://github.com/DVNghiem/KafPy.git
cd KafPy
pip install maturin
maturin develop --release
```

## Quick Start

```python
import kafpy

config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="orders-consumer",
    topics=["orders"],
)

consumer = kafpy.Consumer(config)
app = kafpy.KafPy(consumer)

@app.handler(topic="orders")
def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
    order_id = msg.get_key_as_string()
    payload = msg.get_payload_as_string()
    print(f"topic={ctx.topic} offset={ctx.offset} key={order_id} payload={payload}")
    # Keep return value explicit for readability; failures should raise exceptions.
    return kafpy.HandlerResult(action="ack")

app.run()
```

## Core Concepts

- `ConsumerConfig` defines Kafka + runtime behavior (retry, DLQ, worker count, timeout).
- `Consumer` bridges Python config into the Rust runtime.
- `KafPy` provides decorator-based handler registration and run lifecycle.
- `KafkaMessage` gives typed accessors (`get_key_as_string()`, `get_payload_as_string()`).
- `HandlerContext` carries topic/partition/offset/timestamp/headers metadata.

## Configuration

`ConsumerConfig` (Python surface) uses `bootstrap_servers` and optional runtime controls:

```python
config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="my-group",
    topics=["orders", "payments"],
    auto_offset_reset="earliest",
    retry_policy=kafpy.RetryConfig(
        max_attempts=3,
        base_delay=0.1,      # seconds
        max_delay=30.0,      # seconds
        jitter_factor=0.1,
    ),
    dlq_topic_prefix="dlq.",
    num_workers=4,
    handler_timeout_ms=120000,
)
```

Load from environment when preferred:

```python
config = kafpy.ConsumerConfig.from_env()
```

Useful env vars:

- `KAFKA_BROKERS`
- `KAFKA_GROUP_ID`
- `KAFKA_TOPICS`
- `KAFKA_DLQ_TOPIC_PREFIX`
- `KAFKA_DRAIN_TIMEOUT_SECS`
- `KAFKA_NUM_WORKERS`
- `KAFPY_ROUTING_PY_CALLBACK_HANDLER`

## Handler Behavior Notes

- Successful handler execution is treated as processed.
- Retry/DLQ flow is driven by failure classification on exceptions/timeouts.
- Use raised exceptions for failure paths; do not rely on return values alone to force retry/DLQ behavior.

## Producer

`Producer` and `ProducerConfig` are exported when the extension module is built:

```python
import kafpy

if kafpy.Producer is not None:
    producer = kafpy.Producer(
        kafpy.ProducerConfig(
            brokers="localhost:9092",
        )
    )
```

## Documentation

- [Docs index](docs/index.md)
- [Getting started](docs/getting-started.md)
- [Configuration](docs/configuration.md)
- [Routing](docs/routing.md)
- [Handlers](docs/handlers.md)
- [Error handling](docs/error-handling.md)
- [API reference](docs/api/kafpy.md)

## License

BSD-3-Clause. See `LICENSE`.