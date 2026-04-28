# KafPy - High-Performance Kafka Consumer & Producer for Python

[![Python](https://img.shields.io/badge/python-3.11%2B-blue)](https://www.python.org/)
[![Rust](https://img.shields.io/badge/rust-latest-orange)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-BSD--3--Clause-green)](LICENSE)

KafPy is a high-performance Apache Kafka consumer and producer library for Python, built with Rust using PyO3. It provides an asynchronous, efficient, and easy-to-use interface for consuming messages from Kafka topics.

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

## Table of Contents

- [Installation](#installation)
- [Quick Start](#quick-start)
- [Configuration](#configuration)
- [Usage Examples](#usage-examples)
- [API Reference](#api-reference)
- [Advanced Topics](#advanced-topics)
- [Troubleshooting](#troubleshooting)
- [Contributing](#contributing)
- [License](#license)

## Installation

### Prerequisites

- Python 3.11 or higher
- Rust toolchain (for building from source)
- [librdkafka](https://github.com/confluentinc/librdkafka).

### Install from PyPI (Coming Soon)

```bash
pip install kafpy
```

### Build from Source

1. Clone the repository:
```bash
git clone https://github.com/DVNghiem/KafPy.git
cd KafPy
```

2. Install Maturin (Rust-Python build tool):
```bash
pip install maturin
```

3. Build and install in development mode:
```bash
maturin develop --release
```

Or build a wheel for distribution:
```bash
maturin build --release
pip install target/wheels/kafpy-*.whl
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

## Features

- High-performance Kafka consumer built with Rust and PyO3
- Synchronous, asynchronous, and batch handler support
- Decorator-based and explicit handler registration
- Configurable retry with exponential backoff and DLQ
- Instance-based configuration (no global state)
- Structured logging and observability metrics
- Graceful shutdown with drain
- Observability via OTLP tracing and metrics export
- Failure classification taxonomy (Retryable, Terminal, NonRetryable)
- Worker pool with configurable concurrency
- Auto offset store control
- Configurable handler execution timeout to prevent poll timeouts

## Configuration

KafPy supports two configuration methods: programmatic and environment-based.

### Environment-Based Configuration

Create a `.env` file in your project root:

```env
# Kafka Broker Configuration
KAFKA_BROKERS=localhost:9092
KAFKA_GROUP_ID=my-consumer-group
KAFKA_TOPICS=topic1,topic2,topic3

# Consumer Behavior
KAFKA_AUTO_OFFSET_RESET=earliest
KAFKA_ENABLE_AUTO_COMMIT=false
KAFKA_SESSION_TIMEOUT_MS=30000
KAFKA_HEARTBEAT_INTERVAL_MS=3000
KAFKA_MAX_POLL_INTERVAL_MS=300000

# Performance Tuning
KAFKA_FETCH_MIN_BYTES=1
KAFKA_MAX_PARTITION_FETCH_BYTES=1048576
KAFKA_MESSAGE_BATCH_SIZE=100
KAFKA_RETRY_BACKOFF_MS=100

# Partition Assignment
KAFKA_PARTITION_ASSIGNMENT_STRATEGY=roundrobin

# Security (Optional)
# KAFKA_SECURITY_PROTOCOL=SASL_SSL
# KAFKA_SASL_MECHANISM=PLAIN
# KAFKA_SASL_USERNAME=your-username
# KAFKA_SASL_PASSWORD=your-password

# Application Settings
PROCESSING_TIMEOUT_MS=30000
GRACEFUL_SHUTDOWN_TIMEOUT_MS=10000

# Retry & DLQ
KAFKA_DLQ_TOPIC_PREFIX=dlq.
KAFKA_DRAIN_TIMEOUT_SECS=30
KAFKA_NUM_WORKERS=4
KAFKA_ENABLE_AUTO_OFFSET_STORE=false
```

Then load configuration from environment:

```python
from kafpy import Consumer, ConsumerConfig

# Load configuration from .env file
config = ConsumerConfig.from_env()

# Create consumer
consumer = Consumer(config)
```

### Configuration Parameters

#### Kafka Configuration

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `brokers` | str | `localhost:9092` | Comma-separated list of Kafka brokers |
| `group_id` | str | `rust-consumer-group` | Consumer group ID |
| `topics` | list[str] | `["transactions", "events"]` | List of topics to subscribe to |
| `auto_offset_reset` | str | `earliest` | Where to start reading (`earliest`, `latest`) |
| `enable_auto_commit` | bool | `false` | Enable automatic offset commits |
| `session_timeout_ms` | int | `30000` | Session timeout in milliseconds |
| `heartbeat_interval_ms` | int | `3000` | Heartbeat interval in milliseconds |
| `max_poll_interval_ms` | int | `300000` | Maximum poll interval in milliseconds |
| `fetch_min_bytes` | int | `1` | Minimum bytes to fetch per request |
| `max_partition_fetch_bytes` | int | `1048576` | Maximum bytes per partition |
| `partition_assignment_strategy` | str | `roundrobin` | Partition assignment strategy |
| `retry_backoff_ms` | int | `100` | Retry backoff in milliseconds |
| `message_batch_size` | int | `100` | Number of messages to batch |

#### Security Configuration

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `security_protocol` | str\|None | `None` | Security protocol (`PLAINTEXT`, `SSL`, `SASL_PLAINTEXT`, `SASL_SSL`) |
| `sasl_mechanism` | str\|None | `None` | SASL mechanism (`PLAIN`, `SCRAM-SHA-256`, `SCRAM-SHA-512`) |
| `sasl_username` | str\|None | `None` | SASL username |
| `sasl_password` | str\|None | `None` | SASL password |

#### Application Configuration

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `processing_timeout_ms` | int | `30000` | Message processing timeout |
| `graceful_shutdown_timeout_ms` | int | `10000` | Graceful shutdown timeout |

#### Retry Configuration

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `default_retry_policy` | RetryConfig \| None | `None` | Retry policy for failed messages |
| `dlq_topic_prefix` | str \| None | `"dlq."` | Prefix for DLQ topic names |
| `drain_timeout_secs` | int \| None | `30` | Graceful shutdown drain timeout in seconds |
| `num_workers` | int \| None | `4` | Number of worker threads for processing |
| `enable_auto_offset_store` | bool \| None | `False` | Auto store offsets after processing |

**RetryConfig fields:**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `max_attempts` | int | `3` | Maximum retry attempts |
| `base_delay_ms` | int | `100` | Base delay in milliseconds |
| `max_delay_ms` | int | `10000` | Maximum delay in milliseconds |
| `jitter_factor` | float | `0.25` | Jitter factor (0.0–1.0) for backoff |

#### Observability Configuration

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `observability_config` | ObservabilityConfig \| None | `None` | OTLP/metrics configuration |

**ObservabilityConfig fields:**

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `otlp_endpoint` | str \| None | `None` | OTLP collector endpoint URL |
| `service_name` | str | `"kafpy"` | Service name for tracing/metrics |
| `sampling_ratio` | float | `0.1` | Trace sampling ratio (0.0–1.0) |
| `log_format` | str | `"text"` | Log format (`"text"` or `"json"`) |

#### Failure Classification

KafPy classifies processing failures into three categories:

| Category | Description |
|----------|-------------|
| `FailureCategory.Retryable` | Transient failures that should be retried |
| `FailureCategory.Terminal` | Permanent failures that go directly to DLQ |
| `FailureCategory.NonRetryable` | Failures that skip retry but aren't DLQ-bound |

**FailureReason** provides structured details:

| Field | Type | Description |
|-------|------|-------------|
| `category` | FailureCategory | Failure category |
| `description` | str | Human-readable description |

## Usage Examples

### Basic Consumer

```python
import asyncio
from kafpy import Consumer, ConsumerConfig, KafkaMessage

def process_message(message: KafkaMessage):
    payload = message.payload.decode('utf-8') if message.payload else ""
    print(f"Processing: {payload}")

async def main():
    config = ConsumerConfig.from_env()
    consumer = Consumer(config)
    consumer.add_handler("my-topic", process_message)
    await consumer.start()

asyncio.run(main())
```

### Multiple Topic Handlers

```python
from kafpy import Consumer, ConsumerConfig, KafkaMessage

def handle_orders(message: KafkaMessage):
    print(f"Order received: {message.payload}")

def handle_payments(message: KafkaMessage):
    print(f"Payment received: {message.payload}")

def handle_notifications(message: KafkaMessage):
    print(f"Notification received: {message.payload}")

async def main():
    config = ConsumerConfig.from_env()
    consumer = Consumer(config)
    
    # Register different handlers for different topics
    consumer.add_handler("orders", handle_orders)
    consumer.add_handler("payments", handle_payments)
    consumer.add_handler("notifications", handle_notifications)
    
    await consumer.start()

asyncio.run(main())
```

### Processing Message Headers and Keys

```python
from kafpy import Consumer, ConsumerConfig, KafkaMessage

def process_with_metadata(message: KafkaMessage):
    # Access message key
    key = message.get_key()
    if key:
        print(f"Message key: {key.decode('utf-8')}")
    
    # Access headers
    headers = message.get_headers()
    for name, value in headers:
        if value:
            print(f"Header {name}: {value.decode('utf-8')}")
    
    # Access payload
    if message.payload:
        payload = message.payload.decode('utf-8')
        print(f"Payload: {payload}")
    
    # Access metadata
    print(f"Topic: {message.topic}")
    print(f"Partition: {message.partition}")
    print(f"Offset: {message.offset}")

async def main():
    config = ConsumerConfig.from_env()
    consumer = Consumer(config)
    consumer.add_handler("my-topic", process_with_metadata)
    await consumer.start()

asyncio.run(main())
```

### JSON Message Processing

```python
import json
from kafpy import Consumer, ConsumerConfig, KafkaMessage

def process_json_message(message: KafkaMessage):
    try:
        if message.payload:
            data = json.loads(message.payload.decode('utf-8'))
            print(f"Received JSON: {data}")
            
            # Process your data
            user_id = data.get('user_id')
            action = data.get('action')
            print(f"User {user_id} performed {action}")
    except json.JSONDecodeError as e:
        print(f"Failed to decode JSON: {e}")

async def main():
    config = ConsumerConfig.from_env()
    consumer = Consumer(config)
    consumer.add_handler("events", process_json_message)
    await consumer.start()

asyncio.run(main())
```

### Graceful Shutdown

```python
import asyncio
import signal
from kafpy import Consumer, ConsumerConfig, KafkaMessage

consumer = None

def handle_message(message: KafkaMessage):
    print(f"Processing message: {message.payload}")

def signal_handler(sig, frame):
    print("Shutting down gracefully...")
    if consumer:
        consumer.stop()

async def main():
    global consumer
    
    # Register signal handlers
    signal.signal(signal.SIGINT, signal_handler)
    signal.signal(signal.SIGTERM, signal_handler)
    
    config = ConsumerConfig.from_env()
    consumer = Consumer(config)
    consumer.add_handler("my-topic", handle_message)
    
    try:
        await consumer.start()
    except Exception as e:
        print(f"Error: {e}")
    finally:
        print("Consumer stopped")

asyncio.run(main())
```

### Error Handling

```python
from kafpy import Consumer, ConsumerConfig, KafkaMessage
import logging

logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

def process_with_error_handling(message: KafkaMessage):
    try:
        if not message.payload:
            logger.warning(f"Empty message at offset {message.offset}")
            return
        
        # Your processing logic here
        data = message.payload.decode('utf-8')
        # Process data...
        
    except UnicodeDecodeError:
        logger.error(f"Failed to decode message at offset {message.offset}")
    except Exception as e:
        logger.error(f"Error processing message: {e}", exc_info=True)
        # Implement retry logic or send to dead letter queue

async def main():
    try:
        config = ConsumerConfig.from_env()
        consumer = Consumer(config)
        consumer.add_handler("my-topic", process_with_error_handling)
        await consumer.start()
    except Exception as e:
        logger.error(f"Fatal error: {e}", exc_info=True)

asyncio.run(main())
```

## API Reference

### Classes

#### `ConsumerConfig`

Configuration for Kafka consumer.

**Constructor:**
```python
ConsumerConfig(
    brokers: str,
    group_id: str,
    topics: list[str],
    auto_offset_reset: str,
    enable_auto_commit: bool,
    session_timeout_ms: int,
    heartbeat_interval_ms: int,
    max_poll_interval_ms: int,
    security_protocol: str | None,
    sasl_mechanism: str | None,
    sasl_username: str | None,
    sasl_password: str | None,
    fetch_min_bytes: int,
    max_partition_fetch_bytes: int,
    partition_assignment_strategy: str,
    retry_backoff_ms: int,
    message_batch_size: int,
    # New optional parameters
    default_retry_policy: RetryConfig | None = None,
    dlq_topic_prefix: str | None = "dlq.",
    drain_timeout_secs: int | None = 30,
    num_workers: int | None = 4,
    enable_auto_offset_store: bool | None = False,
    observability_config: ObservabilityConfig | None = None,
    handler_timeout_ms: int | None = None,
)
```

#### `KafkaMessage`

Represents a Kafka message.

**Attributes:**
- `topic: str` - Topic name
- `partition: int` - Partition number
- `offset: int` - Message offset
- `key: bytes | None` - Message key
- `payload: bytes | None` - Message payload
- `headers: list[tuple[str, bytes | None]]` - Message headers
- `timestamp_millis: int | None` - Message timestamp in milliseconds

**Methods:**
- `get_key() -> bytes | None`: Get message key
- `get_headers() -> list[tuple[str, bytes | None]]`: Get message headers
- `get_timestamp_millis() -> int | None`: Get message timestamp in milliseconds

#### `Consumer`

Kafka consumer instance.

**Constructor:**
```python
Consumer(config: ConsumerConfig)
```

**Methods:**
- `add_handler(topic: str, handler: callable[[KafkaMessage], None]) -> None`: Register a message handler for a specific topic
- `async start() -> None`: Start consuming messages (async method)
- `stop() -> None`: Stop the consumer gracefully
- `status() -> dict`: Get runtime snapshot with consumer state, offsets, and queue depths
- `set_handler_timeout(ms: int) -> None`: Set handler execution timeout in milliseconds (overrides config default)

## Advanced Topics

### Performance Tuning

For high-throughput scenarios, adjust these parameters:

```python
kafka_config = ConsumerConfig(
    # ... other params ...
    fetch_min_bytes=1048576,           # 1MB - fetch more data per request
    max_partition_fetch_bytes=10485760, # 10MB - larger partition fetch
    message_batch_size=1000,            # Process 1000 messages per batch
    max_poll_interval_ms=600000,        # 10 minutes for heavy processing
)
```

### Retry and DLQ

Configure retry behavior and dead-letter queue routing:

```python
from kafpy import ConsumerConfig, RetryConfig, ObservabilityConfig

config = ConsumerConfig(
    # ... other params ...
    default_retry_policy=RetryConfig(
        max_attempts=5,
        base_delay_ms=200,
        max_delay_ms=30000,
        jitter_factor=0.3,
    ),
    dlq_topic_prefix="dlq.",           # Prefix for DLQ topics
    num_workers=8,                      # Increase worker threads for throughput
)
```

### Observability

Enable OTLP tracing and metrics:

```python
from kafpy import ConsumerConfig, ObservabilityConfig

config = ConsumerConfig(
    # ... other params ...
    observability_config=ObservabilityConfig(
        otlp_endpoint="http://localhost:4317",
        service_name="my-service",
        sampling_ratio=0.1,
        log_format="json",
    ),
)
```

### Failure Classification

Use `FailureCategory` and `FailureReason` for structured error handling:

```python
from kafpy.handlers import FailureCategory, FailureReason

@app.handler(topic="orders")
def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext):
    try:
        process_order(msg)
        return kafpy.HandlerResult(action="ack")
    except TemporaryError as e:
        reason = FailureReason(
            category=FailureCategory.Retryable,
            description=str(e),
        )
        print(f"Retryable failure: {reason.description}")
        return kafpy.HandlerResult(action="nack")
    except PermanentError as e:
        reason = FailureReason(
            category=FailureCategory.Terminal,
            description=str(e),
        )
        print(f"Terminal failure: {reason.description}")
        return kafpy.HandlerResult(action="dlq")
```

### Manual Offset Commit

When `enable_auto_commit=False`, KafPy handles offset commits after successful message processing:

```python
kafka_config = ConsumerConfig(
    # ... other params ...
    enable_auto_commit=False,  # Manual offset control
)
```

Messages are committed asynchronously after your handler returns successfully. If your handler raises an exception, the offset won't be committed.

### Partition Assignment Strategies

KafPy supports different partition assignment strategies:

- `roundrobin`: Distributes partitions evenly across consumers
- `range`: Assigns consecutive partitions to consumers
- `cooperative-sticky`: Minimizes partition movement during rebalance

```python
kafka_config = ConsumerConfig(
    # ... other params ...
    partition_assignment_strategy="cooperative-sticky",
)
```

### Security Configuration

#### SASL/PLAIN Authentication

```python
kafka_config = ConsumerConfig(
    brokers="kafka.example.com:9093",
    security_protocol="SASL_SSL",
    sasl_mechanism="PLAIN",
    sasl_username="your-username",
    sasl_password="your-password",
    # ... other params ...
)
```

#### SASL/SCRAM Authentication

```python
kafka_config = ConsumerConfig(
    brokers="kafka.example.com:9093",
    security_protocol="SASL_SSL",
    sasl_mechanism="SCRAM-SHA-256",
    sasl_username="your-username",
    sasl_password="your-password",
    # ... other params ...
)
```

## Troubleshooting

### Common Issues

#### Consumer Not Receiving Messages

1. Check that topics exist and have messages
2. Verify broker connectivity: `telnet broker-host 9092`
3. Check consumer group status using `kafka-consumer-groups`
4. Ensure `auto_offset_reset` is set correctly

#### Connection Timeout

```
Error: Connection timeout
```

**Solution:** 
- Verify `KAFKA_BROKERS` is correct
- Check network connectivity
- Increase `session_timeout_ms`

#### Rebalancing Issues

```
Error: Group rebalancing
```

**Solution:**
- Increase `max_poll_interval_ms` if processing takes long
- Reduce `message_batch_size`
- Check `heartbeat_interval_ms` and `session_timeout_ms` ratio

#### Authentication Failures

```
Error: Authentication failed
```

**Solution:**
- Verify `sasl_username` and `sasl_password`
- Check `security_protocol` matches broker configuration
- Ensure `sasl_mechanism` is supported by broker

### Debugging

Enable detailed logging:

```python
import logging
logging.basicConfig(level=logging.DEBUG)
```

KafPy uses Rust's `tracing` framework internally. Set environment variable:

```bash
RUST_LOG=debug python your_app.py
```

## Performance Considerations

- KafPy uses Rust for the core consumer logic, providing near-native performance
- Message handlers run in Python and are called synchronously
- For CPU-intensive processing, consider using multiprocessing
- For I/O-bound operations, handlers can be async-compatible by spawning tasks

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## License

This project is licensed under the BSD-3-Clause License. See the LICENSE file for details.

## Acknowledgments

- Built with [PyO3](https://pyo3.rs/) for Python-Rust interoperability
- Uses [rdkafka](https://github.com/fede1024/rust-rdkafka) for Kafka protocol implementation
- Powered by [Tokio](https://tokio.rs/) for async runtime

## Support

For issues, questions, or contributions, please visit the [GitHub repository](https://github.com/DVNghiem/KafPy).