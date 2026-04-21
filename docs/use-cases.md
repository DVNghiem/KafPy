# Real-World Use Cases

## E-Commerce Order Processing

### Scenario
Process orders from Kafka, validate, persist to database, trigger fulfillment.

```python
import kafpy
from dataclasses import dataclass
from typing import Optional
import psycopg2
import json

@dataclass
class Order:
    order_id: str
    customer_id: str
    items: list[dict]
    total: float
    status: str = "pending"

class OrderProcessor:
    def __init__(self, db_conn):
        self.db = db_conn

    def process(self, msg: kafpy.KafkaMessage) -> kafpy.HandlerResult:
        try:
            order_data = json.loads(msg.get_payload_as_string())
            order = Order(**order_data)

            # Validate order
            if not self.validate_order(order):
                return kafpy.HandlerResult(action="nack")

            # Persist to database
            self.save_order(order)

            # Trigger fulfillment
            self.trigger_fulfillment(order)

            return kafpy.HandlerResult(action="ack")

        except json.JSONDecodeError as e:
            # Invalid JSON - send to DLQ for manual review
            print(f"Invalid JSON: {e}")
            return kafpy.HandlerResult(action="dlq")
        except Exception as e:
            # Database errors, etc - retry
            print(f"Processing error: {e}")
            return kafpy.HandlerResult(action="nack")

    def validate_order(self, order: Order) -> bool:
        if order.total <= 0:
            return False
        if not order.items:
            return False
        return True

    def save_order(self, order: Order):
        # Database persistence logic
        pass

    def trigger_fulfillment(self, order: Order):
        # Fulfillment trigger logic
        pass

# Setup
config = kafpy.ConsumerConfig(
    bootstrap_servers="kafka1:9092,kafka2:9092,kafka3:9092",
    group_id="order-processor-v2",
    topics=["orders.created", "orders.updated"],
    auto_offset_reset="earliest",
    enable_auto_commit=False,
    session_timeout_ms=45000,
    max_poll_interval_ms=300000,
)

retry_config = kafpy.RetryConfig(
    max_attempts=5,
    base_delay=1.0,
    max_delay=30.0,
    jitter_factor=0.1,
)

db_conn = psycopg2.connect(os.environ["DATABASE_URL"])
processor = OrderProcessor(db_conn)

consumer = kafpy.Consumer(config)
app = kafpy.KafPy(consumer)

@app.handler(topic="orders.created")
def handle_created(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext):
    return processor.process(msg)

@app.handler(topic="orders.updated")
def handle_updated(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext):
    return processor.process(msg)

app.run()
```

## IoT Data Ingestion

### Scenario
Ingest high-volume sensor data, batch write to time-series database.

```python
import kafpy
from datetime import datetime
import influxdb_client
from influxdb_client.client.write_api import SYNCHRONOUS

class IoTDataIngester:
    def __init__(self):
        self.client = influxdb_client.InfluxDBClient(
            url="http://influxdb:8086",
            token=os.environ["INFLUX_TOKEN"],
            org="iot-project"
        )
        self.write_api = self.client.write_api(write_options=SYNCHRONOUS)

    def ingest(self, messages: list[kafpy.KafkaMessage], ctx: kafpy.HandlerContext):
        """Batch ingestion for high-volume IoT data."""
        points = []
        for msg in messages:
            try:
                payload = json.loads(msg.get_payload_as_string())
                point = influxdb_client.Point("sensors") \
                    .tag("device_id", payload["device_id"]) \
                    .tag("sensor_type", payload["type"]) \
                    .field("value", payload["value"]) \
                    .time(payload["timestamp"] * 1_000_000_000)
                points.append(point)
            except (json.JSONDecodeError, KeyError):
                continue

        if points:
            self.write_api.write(bucket="iot_data", org="iot-project", record=points)

        return kafpy.HandlerResult(action="ack")

# Batch configuration for high throughput
batch_config = kafpy.BatchConfig(
    max_batch_size=500,
    max_batch_timeout_ms=1000,
)

concurrency_config = kafpy.ConcurrencyConfig(
    num_workers=8,
)

config = kafpy.ConsumerConfig(
    bootstrap_servers="kafka:9092",
    group_id="iot-ingester",
    topics=["iot.sensors.temperature", "iot.sensors.humidity", "iot.sensors.pressure"],
    auto_offset_reset="latest",  # Only process new data
)

consumer = kafpy.Consumer(config)
app = kafpy.KafPy(consumer)

ingester = IoTDataIngester()

@app.handler(topic="iot.sensors.*", mode="batch")
def handle_sensors(messages: list[kafpy.KafkaMessage], ctx: kafpy.HandlerContext):
    return ingester.ingest(messages, ctx)

app.run()
```

## Log Aggregation

### Scenario
Aggregate application logs from multiple services.

```python
import kafpy
import json
from elasticsearch import Elasticsearch
from datetime import datetime

class LogAggregator:
    def __init__(self):
        self.es = Elasticsearch([os.environ["ELASTICSEARCH_URL"]])
        self.index_prefix = "app-logs"

    def index_logs(self, messages: list[kafpy.KafkaMessage], ctx: kafpy.HandlerContext):
        """Batch index logs to Elasticsearch."""
        bulk_data = []

        for msg in messages:
            try:
                log_entry = {
                    "@timestamp": datetime.utcfromtimestamp(msg.timestamp / 1000).isoformat(),
                    "topic": msg.topic,
                    "partition": msg.partition,
                    "offset": msg.offset,
                    "service": extract_service_name(msg),
                    "level": extract_log_level(msg),
                    "message": msg.get_payload_as_string(),
                    "metadata": {
                        "key": msg.get_key_as_string(),
                        "headers": dict(msg.headers) if msg.headers else {},
                    }
                }
                bulk_data.append({"index": {"_index": f"{self.index_prefix}-{datetime.now():%Y.%m}"}})
                bulk_data.append(log_entry)
            except Exception:
                continue

        if bulk_data:
            self.es.bulk(body=bulk_data, refresh=False)

        return kafpy.HandlerResult(action="ack")

    def extract_service_name(self, msg: kafpy.KafkaMessage) -> str:
        # Extract from topic or headers
        return msg.topic.split(".")[1] if "." in msg.topic else "unknown"

    def extract_log_level(self, msg: kafpy.KafkaMessage) -> str:
        payload = msg.get_payload_as_string() or ""
        for level in ["ERROR", "WARN", "INFO", "DEBUG"]:
            if level in payload:
                return level
        return "INFO"

# Multi-topic subscription
config = kafpy.ConsumerConfig(
    bootstrap_servers="kafka:9092",
    group_id="log-aggregator",
    topics=["logs.app.*", "logs.system.*"],
    auto_offset_reset="earliest",
)

consumer = kafpy.Consumer(config)
app = kafpy.KafPy(consumer)

aggregator = LogAggregator()

@app.handler(topic="logs.*", mode="batch")
def handle_logs(messages: list[kafpy.KafkaMessage], ctx: kafpy.HandlerContext):
    return aggregator.index_logs(messages, ctx)

app.run()
```

## Financial Transaction Processing

### Scenario
Process payment transactions with strict ordering and exactly-once semantics.

```python
import kafpy
import json
import redis
from decimal import Decimal

class TransactionProcessor:
    def __init__(self):
        self.redis = redis.Redis.from_url(os.environ["REDIS_URL"])
        self.db = get_db_connection()

    def process(self, msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
        """Process payment with idempotency check."""
        try:
            tx_data = json.loads(msg.get_payload_as_string())
            tx_id = tx_data["transaction_id"]
            amount = Decimal(tx_data["amount"])
            account = tx_data["account"]

            # Idempotency check via Redis
            if self.redis.exists(f"processed:{tx_id}"):
                print(f"Duplicate transaction: {tx_id}")
                return kafpy.HandlerResult(action="ack")

            # Process within transaction
            with self.db.transaction():
                # Debit source account
                self.debit_account(account, amount)

                # Credit destination
                self.credit_account(tx_data["destination"], amount)

                # Record transaction
                self.record_transaction(tx_id, tx_data)

                # Mark as processed
                self.redis.setex(f"processed:{tx_id}", 86400, "1")

            return kafpy.HandlerResult(action="ack")

        except InsufficientFundsError:
            return kafpy.HandlerResult(action="nack")  # Retry
        except InvalidTransactionError as e:
            print(f"Invalid transaction: {e}")
            return kafpy.HandlerResult(action="dlq")  # DLQ for manual review
        except Exception as e:
            print(f"Processing error: {e}")
            return kafpy.HandlerResult(action="nack")

    def debit_account(self, account: str, amount: Decimal):
        # Debit logic
        pass

    def credit_account(self, account: str, amount: Decimal):
        # Credit logic
        pass

    def record_transaction(self, tx_id: str, data: dict):
        # Recording logic
        pass

# Exactly-once configuration
config = kafpy.ConsumerConfig(
    bootstrap_servers="kafka:9092",
    group_id="payment-processor",
    topics=["transactions.payments"],
    auto_offset_reset="earliest",
    enable_auto_commit=False,  # Manual commit for exactly-once
)

# Retry with exponential backoff
retry_config = kafpy.RetryConfig(
    max_attempts=3,
    base_delay=2.0,
    max_delay=60.0,
    jitter_factor=0.15,
)

consumer = kafpy.Consumer(config)
app = kafpy.KafPy(consumer)

processor = TransactionProcessor()

@app.handler(topic="transactions.payments")
def handle_payment(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext):
    return processor.process(msg, ctx)

app.run()
```

## Event Sourcing / CQRS

### Scenario
Handle domain events for event-sourced aggregates.

```python
import kafpy
import json
from abc import ABC, abstractmethod
from typing import TypeVar, Generic

T = TypeVar('T')

class EventHandler(ABC, Generic[T]):
    @abstractmethod
    def handle(self, event: T) -> None:
        pass

class OrderAggregate(EventHandler[dict]):
    def __init__(self):
        self.snapshots = {}

    def handle(self, event: dict) -> None:
        event_type = event["type"]

        if event_type == "OrderCreated":
            self.handle_created(event)
        elif event_type == "OrderPaid":
            self.handle_paid(event)
        elif event_type == "OrderShipped":
            self.handle_shipped(event)

    def handle_created(self, event: dict):
        order_id = event["order_id"]
        self.snapshots[order_id] = {
            "id": order_id,
            "status": "created",
            "items": event["items"],
            "total": event["total"],
        }

    def handle_paid(self, event: dict):
        order = self.snapshots.get(event["order_id"])
        if order:
            order["status"] = "paid"

    def handle_shipped(self, event: dict):
        order = self.snapshots.get(event["order_id"])
        if order:
            order["status"] = "shipped"

class EventConsumer:
    def __init__(self, aggregate: OrderAggregate):
        self.aggregate = aggregate

    def process(self, msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext) -> kafpy.HandlerResult:
        try:
            event = json.loads(msg.get_payload_as_string())
            self.aggregate.handle(event)
            return kafpy.HandlerResult(action="ack")
        except Exception as e:
            print(f"Event processing error: {e}")
            return kafpy.HandlerResult(action="nack")

# Event sourcing configuration
config = kafpy.ConsumerConfig(
    bootstrap_servers="kafka:9092",
    group_id="order-aggregate",
    topics=[
        "domain.orders.created",
        "domain.orders.paid",
        "domain.orders.shipped",
        "domain.orders.delivered",
    ],
    auto_offset_reset="earliest",
)

consumer = kafpy.Consumer(config)
app = kafpy.KafPy(consumer)

event_consumer = EventConsumer(OrderAggregate())

# Route events to the same handler
@app.handler(topic="domain.orders.*")
def handle_order_event(msg: kafpy.KafkaMessage, ctx: kafpy.HandlerContext):
    return event_consumer.process(msg, ctx)

app.run()
```

## ML Feature Engineering

### Scenario
Real-time feature computation for ML inference.

```python
import kafpy
import json
import numpy as np
from scipy import signal

class FeatureComputer:
    def __init__(self):
        self.model = load_feature_model()

    def compute_features(self, messages: list[kafpy.KafkaMessage], ctx: kafpy.HandlerContext):
        """Compute features in batch for efficiency."""
        for msg in messages:
            try:
                data = json.loads(msg.get_payload_as_string())
                features = self.extract_features(data)

                # Publish features for inference
                self.publish_features(features, msg.topic)

            except Exception as e:
                print(f"Feature computation error: {e}")
                continue

        return kafpy.HandlerResult(action="ack")

    def extract_features(self, data: dict) -> dict:
        """Extract ML features from raw sensor data."""
        raw = np.array(data["values"])

        features = {
            "mean": np.mean(raw),
            "std": np.std(raw),
            "min": np.min(raw),
            "max": np.max(raw),
            "rms": np.sqrt(np.mean(raw**2)),
            "peak_to_peak": np.ptp(raw),
        }

        # Frequency domain features
        fft = np.fft.fft(raw)
        freq = np.fft.fftfreq(len(raw))
        power = np.abs(fft)**2

        features["dominant_frequency"] = freq[np.argmax(power[1:]) + 1]
        features["spectral_centroid"] = np.sum(freq * power) / np.sum(power)

        # Filtered features
        filtered = signal.medfilt(raw, kernel_size=3)
        features["median_filtered_mean"] = np.mean(filtered)

        return features

    def publish_features(self, features: dict, original_topic: str):
        # Publish to feature store
        pass

config = kafpy.ConsumerConfig(
    bootstrap_servers="kafka:9092",
    group_id="ml-features",
    topics=["sensor.raw.data"],
    auto_offset_reset="latest",
)

batch_config = kafpy.BatchConfig(
    max_batch_size=200,
    max_batch_timeout_ms=500,
)

consumer = kafpy.Consumer(config)
app = kafpy.KafPy(consumer)

computer = FeatureComputer()

@app.handler(topic="sensor.raw.data", mode="batch")
def handle_raw_data(messages: list[kafpy.KafkaMessage], ctx: kafpy.HandlerContext):
    return computer.compute_features(messages, ctx)

app.run()
```