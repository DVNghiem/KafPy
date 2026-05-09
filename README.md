# KafPy

[![Python](https://img.shields.io/badge/python-3.11%2B-blue)](https://www.python.org/)
[![Rust](https://img.shields.io/badge/rust-latest-orange)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-BSD--3--Clause-green)](LICENSE)

A Python Kafka library built with Rust and PyO3 for high-performance message consumption.

## Overview

KafPy provides a handler-based API for building Kafka consumers in Python. It combines Rust's performance for the Kafka core with a clean Python interface for writing business logic.

**Key capabilities:**

- Sync and async handlers
- Batch message processing
- Built-in retry and dead-letter queue (DLQ) support
- Prometheus metrics and OTLP tracing
- Middleware for logging, metrics, and custom extensions

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
    print(f"Received: {msg.key} @ {ctx.topic}:{ctx.partition}:{ctx.offset}")
    return kafpy.HandlerResult(action="ack")

app.run()
```

## Installation

### Prerequisites

- Python 3.11+
- Rust toolchain
- `librdkafka`

**Install `librdkafka` before building:**

```bash
# macOS
brew install librdkafka

# Ubuntu / Debian
apt install librdkafka-dev

# Fedora
dn install librdkafka-devel
```

### From PyPI

```bash
pip install kafpy
```

### From source

```bash
git clone https://github.com/DVNghiem/KafPy.git
cd KafPy
pip install maturin
maturin develop --release
```

## Full Documentation

Detailed guides and API reference are available at the [KafPy documentation site](https://DVNghiem.github.io/KafPy/).

| Guide | Description |
|-------|-------------|
| [Getting Started](https://DVNghiem.github.io/KafPy/tutorial/) | Build your first Kafka consumer |
| [Configuration](https://DVNghiem.github.io/KafPy/installation/) | Consumer and retry configuration |
| [Handlers](https://DVNghiem.github.io/KafPy/guides/) | Writing sync, async, and batch handlers |
| [Error Handling](https://DVNghiem.github.io/KafPy/best-practices/) | Retry, DLQ, and timeout strategies |
| [API Reference](https://DVNghiem.github.io/KafPy/api/) | Full API documentation |

## License

BSD-3-Clause. See `LICENSE`.
