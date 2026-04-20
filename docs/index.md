# KafPy Documentation

## Quick Start

KafPy is a high-performance Kafka consumer framework for Python, built with Rust.

### Prerequisites

- Python 3.11+
- librdkafka installed on your system

**librdkafka installation:**

```bash
# macOS
brew install librdkafka

# Ubuntu / Debian
apt install librdkafka-dev

# Fedora / RHEL
dnf install librdkafka-devel
```

### Installation

```bash
pip install maturin
maturin develop --release
```

### Your First Consumer

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
    print(msg.payload)
    return kafpy.HandlerResult(action="ack")

app.run()
```

## Guides

- [Getting Started](../kafpy/guides/getting-started.md)
- [Handler Patterns](../kafpy/guides/handler-patterns.md)
- [Configuration](../kafpy/guides/configuration.md)
- [Error Handling](../kafpy/guides/error-handling.md)