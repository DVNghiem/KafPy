# Tutorial

Get started with KafPy by building your first Kafka consumer application.

## Prerequisites

- Python 3.11 or later
- [librdkafka](https://github.com/confluentinc/librdkafka) installed on your system
- A running Kafka broker (or use the docker-compose setup below)

## Install KafPy

Install KafPy from PyPI:

```bash
pip install kafpy
```

Or install from source:

```bash
git clone https://github.com/DVNghiem/KafPy.git
cd KafPy
pip install maturin
maturin develop --release
```

## Start Kafka

The quickest way to get Kafka running locally is with Docker Compose. A `docker-compose.yaml` is included in the repository.

```bash
docker-compose up -d
```

This starts a 3-broker Kafka cluster and Kafka UI. Verify everything is healthy:

```bash
docker-compose ps
```

Kafka UI is available at [http://localhost:8080](http://localhost:8080) for browsing topics and messages.

## Create a Topic

Create a topic for your first application:

```bash
docker-compose exec kafka-1 kafka-topics.sh \
  --create \
  --topic my-topic \
  --bootstrap-server localhost:19092 \
  --partitions 3 \
  --replication-factor 1
```

## Your First Consumer

Create a file `consumer.py` with the following:

```python
import kafpy

config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:19092",
    group_id="my-group",
    topics=["my-topic"],
)

consumer = kafpy.Consumer(config)
app = kafpy.KafPy(consumer)


@app.handler(topic="my-topic")
def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
    print(f"Received: {msg.get_payload_as_string()} @ {ctx.topic}:{ctx.partition}:{ctx.offset}")
    return kafpy.HandlerResult(action="ack")


if __name__ == "__main__":
    app.start()
```

Run the consumer:

```bash
python consumer.py
```

The consumer will start and wait for messages. Leave it running.

## Produce Messages

In a separate terminal, produce some messages using Kafka's console producer:

```bash
docker-compose exec kafka-1 kafka-console-producer.sh \
  --topic my-topic \
  --bootstrap-server localhost:19092
```

Type a few messages and press Enter after each:

```
Hello KafPy
First message
Testing 1 2 3
```

Your consumer terminal should output:

```
Received: Hello KafPy @ my-topic:0:0
Received: First message @ my-topic:0:1
Received: Testing 1 2 3 @ my-topic:0:2
```

Press `Ctrl+C` to stop the consumer.

## Configuration Basics

`ConsumerConfig` accepts several options to tune behavior:

```python
import kafpy

config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:19092,localhost:29092,localhost:39092",  # Multiple brokers
    group_id="my-group",
    topics=["my-topic"],
    auto_offset_reset="earliest",  # Start from earliest offset, not latest
    enable_auto_commit=False,       # Manual commit for at-least-once delivery
    session_timeout_ms=45000,
    heartbeat_interval_ms=5000,
)
```

### Key Configuration Options

| Option | Default | Description |
|--------|---------|-------------|
| `bootstrap_servers` | Required | Comma-separated broker addresses |
| `group_id` | Required | Consumer group identifier |
| `topics` | Required | List of topics to subscribe to |
| `auto_offset_reset` | `"earliest"` | Where to start if no offset exists |
| `enable_auto_commit` | `False` | Whether to auto-commit offsets |
| `session_timeout_ms` | `30000` | Session timeout in milliseconds |
| `max_poll_interval_ms` | `300000` | Maximum poll interval in milliseconds |

## Batch Handlers

For high-throughput scenarios, process messages in batches using `batch_handler`:

```python
import kafpy

config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:19092",
    group_id="batch-group",
    topics=["my-topic"],
)
consumer = kafpy.Consumer(config)
app = kafpy.KafPy(consumer)


@app.batch_handler(topic="my-topic", max_size=50, max_wait_ms=500)
def handle_batch(messages: list[kafpy.KafkaMessage], ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
    print(f"Processing batch of {len(messages)} messages")
    for msg in messages:
        print(f"  Offset {ctx.offset}: {msg.get_payload_as_string()}")
    return kafpy.HandlerResult(action="ack")


if __name__ == "__main__":
    app.start()
```

## Retry and Error Handling

Configure automatic retry with exponential backoff:

```python
import kafpy

config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:19092",
    group_id="retry-group",
    topics=["my-topic"],
    retry_policy=kafpy.RetryConfig(
        max_attempts=3,
        base_delay=0.5,      # 500ms base delay
        max_delay=30.0,      # 30s max delay
        jitter_factor=0.1,    # 10% random jitter
    ),
    dlq_topic_prefix="dlq.",  # Dead letter queue for failed messages
)


@app.handler(topic="my-topic")
def handle_with_retry(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
    try:
        process_message(msg)
        return kafpy.HandlerResult(action="ack")
    except TransientError:
        return kafpy.HandlerResult(action="retry")
    except PermanentError:
        return kafpy.HandlerResult(action="dlq")


if __name__ == "__main__":
    app.start()
```

### Handler Actions

Handlers return an action that tells the runtime how to proceed:

| Action | Description |
|--------|-------------|
| `"ack"` | Message processed successfully — commit offset |
| `"nack"` | Processing failed — trigger retry if configured |
| `"retry"` | Explicit retry with backoff |
| `"dlq"` | Route to dead letter queue |

## Middleware

Add built-in middleware for logging and metrics:

```python
import kafpy

config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:19092",
    group_id="middleware-group",
    topics=["my-topic"],
)
consumer = kafpy.Consumer(config)
app = kafpy.KafPy(consumer)


@app.handler(topic="my-topic", middleware=[kafpy.Logging(), kafpy.Metrics()])
def handle_with_middleware(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
    return kafpy.HandlerResult(action="ack")


if __name__ == "__main__":
    app.start()
```

## Running with Context Manager

Use the context manager for automatic cleanup on shutdown:

```python
import kafpy

config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:19092",
    group_id="context-manager-group",
    topics=["my-topic"],
)

with kafpy.Consumer(config) as consumer:
    app = kafpy.KafPy(consumer)

    @app.handler(topic="my-topic")
    def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
        return kafpy.HandlerResult(action="ack")

    app.start()
```

The consumer stops gracefully when the `with` block exits.

## Next Steps

- **[Configuration](installation.md)** — Explore all consumer configuration options
- **[Guides](guides.md)** — Learn about sync and batch handlers
- **[Best Practices](best-practices.md)** — Retry strategies, DLQ handling, and timeouts
- **[API Reference](api.md)** — Complete API documentation
