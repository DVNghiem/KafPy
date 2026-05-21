# API Reference

KafPy provides a handler-based API for building Kafka consumers in Python. All public types are exported from the `kafpy` package.

## Overview

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
    return kafpy.HandlerResult(action="ack")

app.run()
```

## Public API Summary

| Class | Purpose |
|-------|---------|
| [`ConsumerConfig`](#consumerconfig) | Configuration for Kafka consumer |
| [`Consumer`](#consumer) | Python wrapper around the Rust Kafka consumer |
| [`KafPy`](#kafpy) | Main runtime with handler registration |
| [`KafkaMessage`](#kafkamessage) | Incoming Kafka message with typed accessors |
| [`HandlerContext`](#handlercontext) | Metadata for a handler invocation |
| [`HandlerResult`](#handlerresult) | Handler return value directing runtime behavior |
| [`HandlerAction`](#handleraction) | Enum of possible handler actions |
| [`FanOutBuilder`](#fanoutbuilder) | Builder for fan-out handler registration |
| [`FanOutRegistration`](#fanoutregistration) | Result of a fan-out registration |
| [`FanInRegistration`](#faninregistration) | Result of a fan-in handler registration |
| [`RetryConfig`](#retryconfig) | Retry configuration for handler failures |
| [`ObservabilityConfig`](#observabilityconfig) | OTLP tracing and metrics configuration |
| [`BatchConfig`](#batchconfig) | Batch processing configuration |
| [`FailureCategory`](#failurecategory) | High-level failure category |
| [`FailureReason`](#failurereason) | A specific failure reason with category and description |
| [`BaseMiddleware`](#basemiddleware) | Base class for user-defined middleware |
| [`Logging`](#logging) | Built-in logging middleware |
| [`Metrics`](#metrics) | Built-in metrics middleware |
| [`KafPyError`](#kafpyerror) | Base exception for all KafPy errors |
| [`ConsumerError`](#consumererror) | Consumer-level errors |
| [`HandlerError`](#handlererror) | Handler processing errors |
| [`ConfigurationError`](#configurationerror) | Configuration errors |

---

## Configuration

### `ConsumerConfig`

```python
config = kafpy.ConsumerConfig(
    bootstrap_servers="localhost:9092",
    group_id="my-group",
    topics=["my-topic"],
)
```

**Attributes:**

| Name | Type | Default | Description |
|------|------|---------|-------------|
| `bootstrap_servers` | `str` | Required | Comma-separated Kafka broker addresses |
| `group_id` | `str` | Required | Consumer group identifier |
| `topics` | `list[str]` | Required | Topics to subscribe to |
| `auto_offset_reset` | `str` | `"earliest"` | Where to start if no offset exists (`"earliest"` or `"latest"`) |
| `enable_auto_commit` | `bool` | `False` | Whether to auto-commit offsets |
| `session_timeout_ms` | `int` | `30000` | Session timeout in milliseconds |
| `bootstrap_timeout_ms` | `int \| None` | `None` | Bootstrap timeout in milliseconds (None uses rdkafka default) |
| `heartbeat_interval_ms` | `int` | `3000` | Heartbeat interval in milliseconds |
| `max_poll_interval_ms` | `int` | `300000` | Maximum poll interval in milliseconds |
| `security_protocol` | `str \| None` | `None` | Security protocol (`"PLAINTEXT"`, `"SSL"`, `"SASL_PLAINTEXT"`, `"SASL_SSL"`) |
| `sasl_mechanism` | `str \| None` | `None` | SASL mechanism (`"PLAIN"`, `"SCRAM-SHA-256"`, `"SCRAM-SHA-512"`) |
| `sasl_username` | `str \| None` | `None` | SASL username |
| `sasl_password` | `str \| None` | `None` | SASL password |
| `fetch_min_bytes` | `int` | `1` | Minimum bytes to fetch per request |
| `max_partition_fetch_bytes` | `int` | `1048576` | Maximum bytes per partition to fetch |
| `partition_assignment_strategy` | `str` | `"roundrobin"` | Assignment strategy (`"roundrobin"`, `"range"`, `"cooperative-sticky"`) |
| `retry_backoff_ms` | `int` | `100` | Retry backoff interval in milliseconds |
| `message_batch_size` | `int` | `100` | Number of messages per batch |
| `retry_policy` | [`RetryConfig`](#retryconfig) \| `None` | `None` | Retry configuration for handler failures |
| `dlq_topic_prefix` | `str \| None` | `None` | Prefix for DLQ topic names (default: `"dlq."`) |
| `drain_timeout_secs` | `int \| None` | `None` | Graceful shutdown drain timeout in seconds (default: `30`) |
| `num_workers` | `int \| None` | `None` | Number of concurrent worker threads (default: `4`) |
| `enable_auto_offset_store` | `bool \| None` | `None` | Enable auto offset store (default: `False`) |
| `observability_config` | [`ObservabilityConfig`](#observabilityconfig) \| `None` | `None` | OTLP tracing and metrics configuration |
| `handler_timeout_ms` | `int \| None` | `None` | Per-handler execution timeout in milliseconds |

### Commit Behavior

KafPy uses a **signal-driven commit architecture** that provides low-latency offset persistence without busy-waiting:

1. **On each message ack** — the offset tracker signals the committer immediately via an internal watch channel
2. **Commiter throttle check** — if throttle conditions are met (interval elapsed OR batch threshold reached), it commits that specific topic-partition
3. **Interval fallback** — a periodic tick (every 100ms) ensures all pending offsets are committed even if signals are missed

This hybrid approach balances responsiveness (immediate commit after ack) with robustness (safety-net interval tick). Default throttle settings:

| Parameter | Default | Description |
|-----------|---------|-------------|
| `commit_interval_ms` | 100 | Minimum interval between commit cycles |
| `commit_max_messages` | 100 | Messages accumulated before forcing a commit |

**Raises:** `ValueError` for invalid values (negative timeouts, invalid `auto_offset_reset`, etc.)

**Methods:**

- `to_rust()` — Converts to Rust ConsumerConfig for use with the runtime.

---

### `RetryConfig`

```python
retry_config = kafpy.RetryConfig(
    max_attempts=3,
    base_delay=0.1,
    max_delay=30.0,
    jitter_factor=0.1,
)
```

**Attributes:**

| Name | Type | Default | Description |
|------|------|---------|-------------|
| `max_attempts` | `int` | `3` | Maximum retry attempts (0 = no retry) |
| `base_delay` | `float` | `0.1` | Base delay in seconds between retries (exponential backoff) |
| `max_delay` | `float \| None` | `None` | Maximum delay in seconds (default: `30`) |
| `jitter_factor` | `float \| None` | `None` | Random jitter factor 0.0–1.0 added to delay (default: `0.1`) |

---

### `ObservabilityConfig`

```python
obs_config = kafpy.ObservabilityConfig(
    otlp_endpoint="http://localhost:4317",
    service_name="my-consumer",
    sampling_ratio=1.0,
    log_format="json",
)
```

**Attributes:**

| Name | Type | Default | Description |
|------|------|---------|-------------|
| `otlp_endpoint` | `str \| None` | `None` | OTLP exporter endpoint. `None` disables tracing (zero-cost). |
| `service_name` | `str` | `"kafpy"` | Service name for OTLP resource |
| `sampling_ratio` | `float` | `1.0` | Sampling ratio 0.0–1.0 (1.0 = sample everything) |
| `log_format` | `str` | `"pretty"` | Log format: `"json"`, `"pretty"`, or `"simple"` |

---

### `BatchConfig`

```python
batch_config = kafpy.BatchConfig(
    max_batch_size=100,
    max_batch_timeout_ms=1000,
)
```

**Attributes:**

| Name | Type | Default | Description |
|------|------|---------|-------------|
| `max_batch_size` | `int` | `100` | Maximum messages per batch |
| `max_batch_timeout_ms` | `int` | `1000` | Maximum wait time for a batch in milliseconds |

---

## Handler Types

### `KafkaMessage`

Kafka message with typed field access.

```python
@dataclass(frozen=True)
class KafkaMessage:
    topic: str
    partition: int
    offset: int
    key: bytes | None
    payload: bytes | None
    headers: list[tuple[str, bytes | None]]
    timestamp_millis: int | None = None
    _trace_context: dict[str, str] | None = None
```

**Attributes:**

| Name | Type | Description |
|------|------|-------------|
| `topic` | `str` | Kafka topic this message was consumed from |
| `partition` | `int` | Partition number |
| `offset` | `int` | Message offset in partition |
| `key` | `bytes \| None` | Message key as bytes, or None if not set |
| `payload` | `bytes \| None` | Message payload as bytes, or None if not set |
| `headers` | `list[tuple[str, bytes \| None]]` | List of (key, value) header tuples |
| `timestamp_millis` | `int \| None` | Message timestamp in milliseconds since epoch |
| `_trace_context` | `dict[str, str] \| None` | Internal trace context (not for public use) |

**Methods:**

#### `get_key_as_string()`

```python
key = msg.get_key_as_string()  # str | None
```

Decode key as UTF-8 string.

**Returns:** `str | None` — the decoded key, or `None` if key is absent.

**Raises:** `HandlerError` if the key cannot be decoded as UTF-8.

#### `get_payload_as_string()`

```python
payload = msg.get_payload_as_string()  # str | None
```

Decode payload as UTF-8 string.

**Returns:** `str | None` — the decoded payload, or `None` if payload is absent.

**Raises:** `HandlerError` if the payload cannot be decoded as UTF-8.

#### `from_dict(data: dict) -> KafkaMessage`

Construct a `KafkaMessage` from a dictionary (used internally by the runtime).

---

### `HandlerContext`

Context for a handler invocation, providing metadata about the Kafka message.

```python
@dataclass(frozen=True)
class HandlerContext:
    topic: str           # Kafka topic name
    partition: int       # Partition number
    offset: int          # Message offset in partition
    timestamp: int       # Timestamp in milliseconds since epoch
    headers: dict[str, str]  # Message headers
```

---

### `HandlerResult`

Result of a handler invocation, directing the runtime's next action.

```python
result = kafpy.HandlerResult(action="ack")
```

**Attributes:**

| Name | Type | Description |
|------|------|-------------|
| `action` | `str \| HandlerAction` | Action to take: `"ack"`, `"nack"`, `"dlq"`, `"retry"` |

---

### `HandlerAction`

Enum of possible actions a handler can return.

```python
class HandlerAction(str, Enum):
    ACK = "ack"      # Commit offset
    NACK = "nack"    # Retry without backoff
    DLQ = "dlq"      # Route to dead letter queue
    RETRY = "retry"  # Retry with backoff
```

---

### `FailureCategory`

High-level failure category for classifying message processing errors.

```python
if failure.category == kafpy.FailureCategory.Retryable:
    # Transient error, may succeed on retry
```

**Attributes:**

| Name | Description |
|------|-------------|
| `Retryable` | Transient failures that may succeed on retry (e.g., network timeout) |
| `Terminal` | Permanent failures indicating a bad message (e.g., poison message) |
| `NonRetryable` | Failures that should not be retried (e.g., validation error) |

---

### `FailureReason`

A specific failure reason with its category and description.

```python
@dataclass(frozen=True)
class FailureReason:
    category: str       # FailureCategory value
    description: str    # Human-readable description
```

---

## Runtime

### `Consumer`

Python wrapper around the Rust Kafka consumer. Created via `kafpy.Consumer(config)`.

```python
consumer = kafpy.Consumer(config)
```

**Constructor:**

| Name | Type | Description |
|------|------|-------------|
| `config` | [`ConsumerConfig`](#consumerconfig) | Consumer configuration |

**Methods:**

#### `add_handler(topic, handler, *, mode=None, batch_max_size=None, batch_max_wait_ms=None, timeout_ms=None, concurrency=None, middleware=None)`

Register a handler for a topic. Prefer using `@app.handler` decorator instead.

| Name | Type | Default | Description |
|------|------|---------|-------------|
| `topic` | `str` | Required | Kafka topic to subscribe to |
| `handler` | `Callable` | Required | Callable that takes a `KafkaMessage` |
| `mode` | `str \| None` | `None` | Handler mode (`"sync"`, `"batch_sync"`) |
| `batch_max_size` | `int \| None` | `None` | Max messages per batch (batch modes) |
| `batch_max_wait_ms` | `int \| None` | `None` | Max wait time per batch in ms (batch modes) |
| `timeout_ms` | `int \| None` | `None` | Per-handler execution timeout in milliseconds |
| `concurrency` | `int \| None` | `None` | Maximum concurrent executions (None = no limit) |
| `middleware` | `list \| None` | `None` | Middleware instances (e.g., `[Logging(), Metrics()]`) |

#### `start()`

Start the consumer. Blocks until the consumer shuts down.

```python
consumer.start()
```

#### `stop()`

Stop the consumer gracefully. Initiates drain: waits for in-flight messages to complete, then shuts down.

```python
consumer.stop()
```

#### `register_fanout(group_name, sink_topics, handler, *, max_fan_out=None, timeout_ms=None) -> FanOutBuilder`

Register a fan-out group.

```python
builder = consumer.register_fanout(
    group_name="enrichment",
    sink_topics=["topic-a", "topic-b"],
    handler=my_handler,
    max_fan_out=8,
    timeout_ms=5000,
)
registration = builder.max_fan_out(8).register()
```

| Name | Type | Default | Description |
|------|------|---------|-------------|
| `group_name` | `str` | Required | Identifier for this fan-out group |
| `sink_topics` | `list[str]` | Required | List of sink topic names to fan out to |
| `handler` | `Callable` | Required | Python callable invoked for each sink topic |
| `max_fan_out` | `int \| None` | `None` | Maximum concurrent sink branches (default 4, max 64) |
| `timeout_ms` | `int \| None` | `None` | Per-branch execution timeout in milliseconds |

**Returns:** `FanOutBuilder`

#### `register_fanin(handler_key, sources, handler, *, timeout_ms=None) -> FanInRegistration`

Register a fan-in handler: one callback receives messages from multiple topics.

```python
registration = consumer.register_fanin(
    handler_key="aggregator",
    sources=["topic-a", "topic-b", "topic-c"],
    handler=my_handler,
    timeout_ms=5000,
)
```

| Name | Type | Default | Description |
|------|------|---------|-------------|
| `handler_key` | `str` | Required | Unique identifier for this handler |
| `sources` | `list[str]` | Required | List of Kafka topic names to subscribe to |
| `handler` | `Callable` | Required | Python callable invoked for each message |
| `timeout_ms` | `int \| None` | `None` | Per-handler execution timeout in milliseconds |

**Returns:** `FanInRegistration`

#### `status() -> dict[str, Any]`

Return the current runtime snapshot as a dictionary.

```python
status = consumer.status()
# {'timestamp': ..., 'worker_states': {...}, 'queue_depths': {...}, ...}
```

**Returns:** `dict[str, Any]` with keys: `timestamp`, `worker_states`, `queue_depths`, `accumulator_info`, `consumer_lag_summary`

#### Context Manager

`Consumer` supports `async with` for automatic graceful shutdown:

```python
async with consumer:
    consumer.start()
# Automatically stops on exit
```

---

### `KafPy`

Main runtime for consuming Kafka messages. Create with a `Consumer`, register handlers using the decorator or `register_handler()`, then call `run()`.

```python
app = kafpy.KafPy(consumer)

@app.handler(topic="my-topic")
def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
    return kafpy.HandlerResult(action="ack")

app.run()
```

**Constructor:**

| Name | Type | Description |
|------|------|-------------|
| `consumer` | [`Consumer`](#consumer) | Consumer instance |

**Methods:**

#### `handler(topic, *, routing=None, timeout_ms=None, concurrency=None, middleware=None)`

Decorator to register a single-message handler for a topic. **Async functions are not supported.**

```python
@app.handler(topic="my-topic")
def handle(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
    return kafpy.HandlerResult(action="ack")
```

| Name | Type | Default | Description |
|------|------|---------|-------------|
| `topic` | `str` | Required | Kafka topic to handle |
| `routing` | `object \| None` | `None` | Optional routing configuration |
| `timeout_ms` | `int \| None` | `None` | Per-handler execution timeout (overrides `ConsumerConfig.handler_timeout_ms`) |
| `concurrency` | `int \| None` | `None` | Maximum concurrent executions (None = no limit) |
| `middleware` | `list \| None` | `None` | Middleware instances (e.g., `[Logging(), Metrics()]`) |

**Raises:** `TypeError` if the decorated function is async.

#### `batch_handler(topic, *, max_size=100, max_wait_ms=1000, timeout_ms=None)`

Decorator to register a batch handler for a topic. **Async functions are not supported.**

```python
@app.batch_handler(topic="my-topic", max_size=50, max_wait_ms=500)
def handle_batch(messages: list[kafpy.KafkaMessage], ctx) -> kafpy.HandlerResult:
    for msg in messages:
        process(msg)
    return kafpy.HandlerResult(action="ack")
```

| Name | Type | Default | Description |
|------|------|---------|-------------|
| `topic` | `str` | Required | Kafka topic to handle |
| `max_size` | `int` | `100` | Maximum messages per batch |
| `max_wait_ms` | `int` | `1000` | Maximum wait time before dispatching a batch |
| `timeout_ms` | `int \| None` | `None` | Per-handler execution timeout in milliseconds |

**Raises:** `TypeError` if the decorated function is async.

#### `register_handler(topic, handler_fn, *, routing=None, timeout_ms=None, concurrency=None, middleware=None)`

Explicitly register a single-message handler for a topic.

```python
def handle(msg, ctx):
    return HandlerResult(action="ack")
app.register_handler("my-topic", handle)
```

| Name | Type | Default | Description |
|------|------|---------|-------------|
| `topic` | `str` | Required | Kafka topic to handle |
| `handler_fn` | `Callable` | Required | Regular (non-async) callable |
| `routing` | `object \| None` | `None` | Optional routing configuration |
| `timeout_ms` | `int \| None` | `None` | Per-handler execution timeout in milliseconds |
| `concurrency` | `int \| None` | `None` | Maximum concurrent executions |
| `middleware` | `list \| None` | `None` | Middleware instances |

**Raises:** `TypeError` if handler_fn is async.

#### `run()`

Run the consumer until `stop()` is called or a signal is received. Blocks the calling thread.

```python
app.run()
```

#### `start()`

Start consuming messages. Returns control to caller.

```python
app.start()
```

#### `stop()`

Stop the consumer gracefully.

```python
app.stop()
```

---

## Fan-out and Fan-in

### `FanOutBuilder`

Builder for configuring fan-out handler registration.

```python
builder = consumer.register_fanout(
    group_name="enrichment",
    sink_topics=["topic-a", "topic-b"],
    handler=my_handler,
)
registration = builder.max_fan_out(8).register()
```

**Methods:**

#### `max_fan_out(n) -> FanOutBuilder`

Set the maximum fan-out degree.

| Name | Type | Description |
|------|------|-------------|
| `n` | `int` | Maximum concurrent sink branches (capped at 64) |

**Returns:** New `FanOutBuilder` with `max_fan_out` set.

#### `register() -> FanOutRegistration`

Register the fan-out group with the consumer.

**Returns:** `FanOutRegistration`

---

### `FanOutRegistration`

Result of a fan-out registration.

```python
@dataclass(frozen=True)
class FanOutRegistration:
    group_name: str       # The identifier for this fan-out group
    fan_out_id: int      # Unique ID generated at registration
    sink_topics: list[str]  # List of sink topics
```

---

### `FanInRegistration`

Result of a fan-in handler registration.

```python
@dataclass(frozen=True)
class FanInRegistration:
    handler_key: str      # Unique identifier for this handler
    fan_in_id: int        # Unique numeric ID generated at registration
    sources: list[str]    # List of source topics
```

---

## Middleware

### `BaseMiddleware`

Base class for user-defined middleware. Subclass and override `before()`, `after()`, `on_error()`.

```python
class MyMiddleware(kafpy.BaseMiddleware):
    def before(self, ctx):
        print(f"Handling message on {ctx['topic']}")

    def after(self, ctx, result, elapsed_ms):
        print(f"Handler completed in {elapsed_ms}ms with result={result}")

    def on_error(self, ctx, result):
        print(f"Handler error: {result}")
```

**Methods:**

#### `before(ctx: dict) -> None`

Called before the handler is invoked.

| Name | Type | Description |
|------|------|-------------|
| `ctx` | `dict` | ExecutionContext dict with `topic`, `partition`, `offset`, etc. |

#### `after(ctx: dict, result: str, elapsed_ms: float) -> None`

Called after the handler succeeds.

| Name | Type | Description |
|------|------|-------------|
| `ctx` | `dict` | ExecutionContext dict |
| `result` | `str` | Result label (e.g., `"ok"`, `"error"`, `"timeout"`) |
| `elapsed_ms` | `float` | Wall-clock time in milliseconds since `before()` was called |

#### `on_error(ctx: dict, result: str) -> None`

Called when the handler invocation returns an error.

| Name | Type | Description |
|------|------|-------------|
| `ctx` | `dict` | ExecutionContext dict |
| `result` | `str` | Result label (e.g., `"error"`, `"timeout"`) |

---

### `Logging`

Built-in logging middleware. Emits tracing span events on handler start/complete/error.

```python
@app.handler(topic="my-topic", middleware=[kafpy.Logging()])
def handle(msg, ctx):
    return kafpy.HandlerResult(action="ack")
```

---

### `Metrics`

Built-in metrics middleware. Records `kafpy.handler.latency` histogram and `kafpy.message.throughput` counter per handler invocation.

```python
@app.handler(topic="my-topic", middleware=[kafpy.Metrics()])
def handle(msg, ctx):
    return kafpy.HandlerResult(action="ack")
```

---

## Exceptions

All KafPy exceptions inherit from `KafPyError` and carry structured context.

### `KafPyError`

Base exception for all KafPy errors.

```python
@dataclass(frozen=True)
class KafPyError(Exception):
    message: str              # Human-readable error description
    error_code: int | None   # Kafka error code when applicable
    partition: int | None     # Kafka partition number when applicable
    topic: str | None        # Kafka topic name when applicable
```

**Methods:**

- `__str__()` — Returns the message.
- `__repr__()` — Returns a detailed representation with all fields.

---

### `ConsumerError`

Raised for consumer-level errors: Kafka errors, subscription issues, message receive failures, serialization errors.

```python
raise ConsumerError(
    message="NOT_LEADER for partition",
    error_code=6,
    partition=0,
    topic="my-topic",
)
```

Format: `"Consumer error: NOT_LEADER (error 6) on my-topic@partition 0"`

---

### `HandlerError`

Raised for handler processing errors: Python handler exceptions, wrong-type message field access, handler panics caught at the PyO3 boundary.

```python
raise HandlerError(
    message="payload is not valid UTF-8: ...",
    error_code=None,
    partition=0,
    topic="my-topic",
)
```

---

### `ConfigurationError`

Raised for configuration errors: invalid config values, missing required fields, Rust config build failures.

---

## Logging

KafPy uses Python's standard `logging` module. All log messages from the Rust extension are forwarded to the `"kafpy"` logger.

```python
import logging

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s"
)
logging.getLogger("kafpy").setLevel(logging.DEBUG)
```

**Logger name:** `"kafpy"`

---

## External Documentation

- [Kafka Protocol](https://kafka.apache.org/protocol) — Kafka protocol specifications
- [librdkafka](https://github.com/confluentinc/librdkafka) — Core Kafka client library used by KafPy