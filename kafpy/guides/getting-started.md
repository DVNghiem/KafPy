# Getting Started with KafPy

## Installation

KafPy requires Python 3.11+ and librdkafka.

### Prerequisites

**librdkafka** must be installed on your system:

```bash
# macOS
brew install librdkafka

# Ubuntu / Debian
apt install librdkafka-dev

# Fedora / RHEL
dnf install librdkafka-devel
```

### Install KafPy

Once librdkafka is installed:

```bash
pip install maturin
maturin develop --release
```

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
def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
    print(msg.payload)
    return kafpy.HandlerResult(action="ack")

app.run()
```

## Next Steps

- Read the [handler patterns guide](./handler-patterns.md) for sync, async, and batch handlers
- Read the [configuration guide](./configuration.md) for all config options
- Read the [error handling guide](./error-handling.md) for exception handling