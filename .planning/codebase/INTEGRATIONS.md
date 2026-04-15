# External Integrations

**Analysis Date:** 2026-04-15

## APIs & External Services

### Apache Kafka

**Integration via rdkafka v0.38** - Rust bindings for librdkafka

- **Purpose:** Message consumption and production for Apache Kafka
- **Protocol Implementation:** rdkafka handles all Kafka protocol communication
- **Supported Brokers:** Apache Kafka 0.10+, Confluent Platform, Amazon MSK, Redpanda

**Consumer Implementation (`src/consume.rs`):**
```rust
use rdkafka::{
    config::ClientConfig,
    consumer::{Consumer, ConsumerContext, StreamConsumer},
};

let mut client_config = ClientConfig::new();
client_config
    .set("bootstrap.servers", &config.brokers)
    .set("group.id", &config.group_id)
    .set("auto.offset.reset", &config.auto_offset_reset)
    // ... security and performance settings
    .create_with_context(context)
```

**Producer Implementation (`src/produce.rs`):**
```rust
use rdkafka::{
    config::ClientConfig,
    producer::{FutureProducer, FutureRecord, Producer},
};

let mut client_config = ClientConfig::new();
client_config
    .set("bootstrap.servers", &config.brokers)
    .set("message.timeout.ms", config.message_timeout_ms.to_string())
    // ... batching, compression, acks settings
    .create()
```

**Key rdkafka Features Used:**
- `StreamConsumer`: Async message streaming with tokio
- `FutureProducer`: Non-blocking message sending with delivery callbacks
- `ClientConfig`: Fluent configuration API
- `ConsumerContext` / `ClientContext`: Custom context for callbacks
- Headers support for message metadata
- SASL/SSL security protocols

## Environment Configuration

**Integration via dotenvy v0.15.7** - Environment variable loading

- **Purpose:** Load configuration from `.env` files
- **Implementation:** `config.rs` `ConsumerConfig::from_env()` and `ProducerConfig::from_env()`
- **Behavior:** Loads `.env` from current directory or specified path before reading variables

**Implementation Pattern (`src/config.rs`):**
```rust
pub fn from_env() -> PyResult<Self> {
    // Load from .env file if it exists
    if let Ok(current_dir) = env::current_dir() {
        let env_path = current_dir.join(".env");
        if env_path.exists() {
            dotenvy::from_path(&env_path).map_err(|e| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string())
            })?;
        }
    } else {
        dotenvy::dotenv().ok();
    }

    // Read configuration values from environment
    Ok(ConsumerConfig {
        brokers: env::var("KAFKA_BROKERS").unwrap_or_else(|_| "localhost:9092".to_string()),
        // ... other fields
    })
}
```

### Consumer Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `KAFKA_BROKERS` | `localhost:9092` | Comma-separated broker addresses |
| `KAFKA_GROUP_ID` | `rust-consumer-group` | Consumer group identifier |
| `KAFKA_TOPICS` | `transactions,events` | Comma-separated topic list |
| `KAFKA_AUTO_OFFSET_RESET` | `earliest` | Offset reset policy |
| `KAFKA_ENABLE_AUTO_COMMIT` | `false` | Auto offset commit |
| `KAFKA_SESSION_TIMEOUT_MS` | `30000` | Session timeout in ms |
| `KAFKA_HEARTBEAT_INTERVAL_MS` | `3000` | Heartbeat interval in ms |
| `KAFKA_MAX_POLL_INTERVAL_MS` | `300000` | Max poll interval in ms |
| `KAFKA_SECURITY_PROTOCOL` | None | Security protocol (PLAINTEXT, SSL, SASL_PLAINTEXT, SASL_SSL) |
| `KAFKA_SASL_MECHANISM` | None | SASL mechanism (PLAIN, SCRAM-SHA-256, SCRAM-SHA-512) |
| `KAFKA_SASL_USERNAME` | None | SASL username |
| `KAFKA_SASL_PASSWORD` | None | SASL password |
| `KAFKA_FETCH_MIN_BYTES` | `1` | Min bytes per fetch |
| `KAFKA_MAX_PARTITION_FETCH_BYTES` | `1048576` | Max bytes per partition |
| `KAFKA_PARTITION_ASSIGNMENT_STRATEGY` | `roundrobin` | Partition assignment |
| `KAFKA_RETRY_BACKOFF_MS` | `100` | Retry backoff in ms |
| `KAFKA_MESSAGE_BATCH_SIZE` | `100` | Message batch size |

### Producer Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `KAFKA_BROKERS` | `localhost:9092` | Comma-separated broker addresses |
| `PRODUCER_MESSAGE_TIMEOUT_MS` | `30000` | Message timeout in ms |
| `PRODUCER_QUEUE_BUFFERING_MAX_MESSAGES` | `100000` | Max buffered messages |
| `PRODUCER_QUEUE_BUFFERING_MAX_KBYTES` | `1048576` | Max buffered KB |
| `PRODUCER_BATCH_NUM_MESSAGES` | `10000` | Batch size for batching |
| `PRODUCER_COMPRESSION_TYPE` | `snappy` | Compression type |
| `PRODUCER_LINGER_MS` | `5` | Linger time in ms |
| `PRODUCER_REQUEST_TIMEOUT_MS` | `30000` | Request timeout in ms |
| `PRODUCER_RETRY_BACKOFF_MS` | `100` | Retry backoff in ms |
| `PRODUCER_RETRIES` | `2147483647` | Max retries |
| `PRODUCER_MAX_IN_FLIGHT` | `5` | Max in-flight requests |
| `PRODUCER_ENABLE_IDEMPOTENCE` | `true` | Idempotent producer |
| `PRODUCER_ACKS` | `all` | Acknowledgment level |
| `KAFKA_SECURITY_PROTOCOL` | None | Security protocol |
| `KAFKA_SASL_MECHANISM` | None | SASL mechanism |
| `KAFKA_SASL_USERNAME` | None | SASL username |
| `KAFKA_SASL_PASSWORD` | None | SASL password |

## Python/Async Integration

**Integration via pyo3-async-runtimes v0.27.0** - Python async runtime bridging

- **Purpose:** Enable async Rust functions to be called from Python asyncio
- **Runtime:** tokio-based runtime (`tokio-runtime` feature)
- **Key Function:** `pyo3_async_runtimes::tokio::future_into_py()` - converts Rust futures to Python coroutines

**Usage in Consumer (`src/consume.rs`):**
```rust
pub fn start(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
    pyo3_async_runtimes::tokio::future_into_py(py, async move {
        // Async tokio code here
        let context = CustomConsumerContext::new();
        let consumer = KafkaManager::create_consumer(&config, context)?;
        // ...
    })
    .map(|b| b.unbind())
}
```

**Usage in Producer (`src/produce.rs`):**
```rust
pub fn send(&self, py: Python<'_>, /* ... */) -> PyResult<Py<PyAny>> {
    pyo3_async_runtimes::tokio::future_into_py(py, async move {
        // Async send logic
        producer.send(record, Timeout::After(Duration::from_millis(timeout))).await
    })
    .map(|b| b.unbind())
}
```

**Python Usage Pattern:**
```python
import asyncio
from kafpy import Consumer

async def main():
    await consumer.start()  # Python async call into Rust

asyncio.run(main())
```

## Logging & Observability

**Integration via tracing v0.1.44 and tracing-subscriber v0.3.22**

- **Purpose:** Structured logging for Rust async code
- **Filter:** Environment-based via `RUST_LOG` env var or `info,consumer=debug` default
- **Format:** Line numbers, thread IDs, file names enabled

**Configuration (`src/logging.rs`):**
```rust
tracing_subscriber::fmt()
    .with_env_filter(
        tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "info,consumer=debug".into()),
    )
    .with_line_number(true)
    .with_thread_ids(true)
    .with_file(true)
    .init();
```

**Log Levels:**
- `error!` - Failed operations (message processing errors, offset commit failures)
- `info!` - Lifecycle events (startup, shutdown, rebalancing)
- `debug!` - Detailed operations (message delivery confirmations)

## Security Features

**SASL Authentication Support (rdkafka):**
- `security.protocol`: PLAINTEXT, SSL, SASL_PLAINTEXT, SASL_SSL
- `sasl.mechanism`: PLAIN, SCRAM-SHA-256, SCRAM-SHA-512
- `sasl.username` / `sasl.password`: Credentials

**SSL/TLS Support:**
- Native rdkafka SSL support for encrypted connections
- Configured via `security_protocol` parameter

## CI/CD & Deployment

**Build System: maturin v1.x**

- **Purpose:** Build and publish Python wheels with Rust extensions
- **CI Provider:** GitHub Actions via PyO3/maturin-action
- **Artifacts:** Platform-specific wheels for Linux (manylinux, musllinux), Windows, macOS

**GitHub Workflows (`.github/workflows/`):**

1. **release.yml** - Release pipeline
   - Builds wheels for: Linux (x86_64, x86, aarch64), Windows (x64, x86), macOS (x86_64, aarch64)
   - Builds musllinux wheels
   - Generates sdist (source distribution)
   - Publishes to PyPI on tag push
   - Artifact attestation via GitHub Actions

2. **preview-deployment.yml** - Preview/test pipeline
   - Builds wheels on main branch pushes
   - Used for testing before releases
   - Windows and macOS builds currently commented out

**Build Commands:**
```bash
maturin develop --release    # Development build and install
maturin build --release       # Build wheels
pip install target/wheels/*.whl  # Install built wheel
```

## Internal Python-Rust Communication

**PyO3 FFI Layer:**

- **Module Name:** `_kafpy` (Rust crate)
- **Public Package:** `kafpy` (Python wrapper)
- **Exported Classes:**
  - `Consumer` -> `PyConsumer`
  - `Producer` -> `PyProducer`
  - `ConsumerConfig` -> `config::ConsumerConfig`
  - `ProducerConfig` -> `config::ProducerConfig`
  - `KafkaMessage` -> `kafka_message::KafkaMessage`

**PyO3 Bindings (`src/lib.rs`):**
```rust
#[pymodule]
fn _kafpy(m: &Bound<'_, PyModule>) -> PyResult<()> {
    Logger::init();
    m.add_class::<KafkaMessage>()?;
    m.add_class::<PyConsumer>()?;
    m.add_class::<PyProducer>()?;
    m.add_class::<config::ConsumerConfig>()?;
    m.add_class::<config::ProducerConfig>()?;
    Ok(())
}
```

**Python Re-export (`kafpy/__init__.py`):**
```python
from ._kafpy import (
    ConsumerConfig,
    ProducerConfig,
    KafkaMessage,
    Consumer,
    Producer,
)
```

---

*Integration audit: 2026-04-15*
