use pyo3::prelude::*;
use std::env;

use crate::pyconfig::{PyObservabilityConfig, PyRetryPolicy};

#[derive(Debug, Clone)]
#[pyclass]
pub struct ConsumerConfig {
    pub brokers: String,
    pub group_id: String,
    pub topics: Vec<String>,
    pub auto_offset_reset: String,
    pub enable_auto_commit: bool,
    pub session_timeout_ms: u32,
    pub heartbeat_interval_ms: u32,
    pub max_poll_interval_ms: u32,
    pub security_protocol: Option<String>,
    pub sasl_mechanism: Option<String>,
    pub sasl_username: Option<String>,
    pub sasl_password: Option<String>,
    pub fetch_min_bytes: u32,
    pub max_partition_fetch_bytes: u32,
    pub partition_assignment_strategy: String,
    pub retry_backoff_ms: u32,
    pub message_batch_size: usize,
    /// Override the default retry policy. None uses the Rust default (3 attempts, 100ms base).
    pub default_retry_policy: Option<PyRetryPolicy>,
    /// DLQ topic prefix. None defaults to "dlq.".
    pub dlq_topic_prefix: Option<String>,
    /// Drain timeout in seconds during graceful shutdown. None defaults to 30.
    pub drain_timeout_secs: Option<u64>,
    /// Number of worker threads. None defaults to 4.
    pub num_workers: Option<u32>,
    /// Whether to enable auto offset store. None defaults to false.
    pub enable_auto_offset_store: Option<bool>,
    /// Observability configuration (OTLP, tracing, metrics). None uses defaults.
    pub observability_config: Option<PyObservabilityConfig>,
    /// Handler execution timeout in milliseconds. If a handler takes longer than
    /// this, it is cancelled and the message is treated as a failure (routed to
    /// retry or DLQ). Defaults to 300000 (5 minutes) if None, which is lower
    /// than rdkafka's default `max_poll_interval_ms` of 300000ms.
    pub handler_timeout_ms: Option<u64>,
}

#[derive(Debug, Clone)]
#[pyclass]
pub struct ProducerConfig {
    pub brokers: String,
    pub message_timeout_ms: u64,
    pub queue_buffering_max_messages: u32,
    pub queue_buffering_max_kbytes: u32,
    pub batch_num_messages: u32,
    pub compression_type: String,
    pub linger_ms: u32,
    pub request_timeout_ms: u32,
    pub retry_backoff_ms: u32,
    pub retries: u32,
    pub max_in_flight: u32,
    pub enable_idempotence: bool,
    pub acks: String,
    pub security_protocol: Option<String>,
    pub sasl_mechanism: Option<String>,
    pub sasl_username: Option<String>,
    pub sasl_password: Option<String>,
}

#[pymethods]
impl ConsumerConfig {
    // Kept for backward compatibility — Python callers using positional args still work.
    // The builder pattern is the preferred API: ConsumerConfigBuilder::new().brokers(...).build()
    #[allow(clippy::too_many_arguments)]
    #[new]
    #[pyo3(signature = (
        brokers,
        group_id,
        topics,
        auto_offset_reset="earliest".to_string(),
        enable_auto_commit=false,
        session_timeout_ms=30000,
        heartbeat_interval_ms=3000,
        max_poll_interval_ms=300000,
        security_protocol = None,
        sasl_mechanism = None,
        sasl_username = None,
        sasl_password = None,
        fetch_min_bytes = 1,
        max_partition_fetch_bytes = 1048576,
        partition_assignment_strategy = "roundrobin".to_string(),
        retry_backoff_ms = 100,
        message_batch_size = 100,
        default_retry_policy = None,
        dlq_topic_prefix = None,
        drain_timeout_secs = None,
        num_workers = None,
        enable_auto_offset_store = None,
        observability_config = None,
        handler_timeout_ms = None,
    ))]
    pub fn new(
        brokers: String,
        group_id: String,
        topics: Vec<String>,
        auto_offset_reset: String,
        enable_auto_commit: bool,
        session_timeout_ms: u32,
        heartbeat_interval_ms: u32,
        max_poll_interval_ms: u32,
        security_protocol: Option<String>,
        sasl_mechanism: Option<String>,
        sasl_username: Option<String>,
        sasl_password: Option<String>,
        fetch_min_bytes: u32,
        max_partition_fetch_bytes: u32,
        partition_assignment_strategy: String,
        retry_backoff_ms: u32,
        message_batch_size: usize,
        default_retry_policy: Option<PyRetryPolicy>,
        dlq_topic_prefix: Option<String>,
        drain_timeout_secs: Option<u64>,
        num_workers: Option<u32>,
        enable_auto_offset_store: Option<bool>,
        observability_config: Option<PyObservabilityConfig>,
        handler_timeout_ms: Option<u64>,
    ) -> Self {
        ConsumerConfig {
            brokers,
            group_id,
            topics,
            auto_offset_reset,
            enable_auto_commit,
            session_timeout_ms,
            heartbeat_interval_ms,
            max_poll_interval_ms,
            security_protocol,
            sasl_mechanism,
            sasl_username,
            sasl_password,
            fetch_min_bytes,
            max_partition_fetch_bytes,
            partition_assignment_strategy,
            retry_backoff_ms,
            message_batch_size,
            default_retry_policy,
            dlq_topic_prefix,
            drain_timeout_secs,
            num_workers,
            enable_auto_offset_store,
            observability_config,
            handler_timeout_ms,
        }
    }

    #[staticmethod]
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
            // Fallback to loading .env from current directory
            dotenvy::dotenv().ok();
        }

        Ok(ConsumerConfig {
            brokers: env::var("KAFKA_BROKERS").unwrap_or_else(|_| "localhost:9092".to_string()),
            group_id: env::var("KAFKA_GROUP_ID")
                .unwrap_or_else(|_| "rust-consumer-group".to_string()),
            topics: env::var("KAFKA_TOPICS")
                .unwrap_or_else(|_| "transactions,events".to_string())
                .split(',')
                .map(|s| s.trim().to_string())
                .collect(),
            auto_offset_reset: env::var("KAFKA_AUTO_OFFSET_RESET")
                .unwrap_or_else(|_| "earliest".to_string()),
            enable_auto_commit: env::var("KAFKA_ENABLE_AUTO_COMMIT")
                .unwrap_or_else(|_| "false".to_string())
                .parse()?,
            session_timeout_ms: env::var("KAFKA_SESSION_TIMEOUT_MS")
                .unwrap_or_else(|_| "30000".to_string())
                .parse()?,
            heartbeat_interval_ms: env::var("KAFKA_HEARTBEAT_INTERVAL_MS")
                .unwrap_or_else(|_| "3000".to_string())
                .parse()?,
            max_poll_interval_ms: env::var("KAFKA_MAX_POLL_INTERVAL_MS")
                .unwrap_or_else(|_| "300000".to_string())
                .parse()?,
            security_protocol: env::var("KAFKA_SECURITY_PROTOCOL").ok(),
            sasl_mechanism: env::var("KAFKA_SASL_MECHANISM").ok(),
            sasl_username: env::var("KAFKA_SASL_USERNAME").ok(),
            sasl_password: env::var("KAFKA_SASL_PASSWORD").ok(),
            fetch_min_bytes: env::var("KAFKA_FETCH_MIN_BYTES")
                .unwrap_or_else(|_| "1".to_string())
                .parse()?,
            max_partition_fetch_bytes: env::var("KAFKA_MAX_PARTITION_FETCH_BYTES")
                .unwrap_or_else(|_| "1048576".to_string())
                .parse()?,
            partition_assignment_strategy: env::var("KAFKA_PARTITION_ASSIGNMENT_STRATEGY")
                .unwrap_or_else(|_| "roundrobin".to_string()),
            retry_backoff_ms: env::var("KAFKA_RETRY_BACKOFF_MS")
                .unwrap_or_else(|_| "100".to_string())
                .parse()?,
            message_batch_size: env::var("KAFKA_MESSAGE_BATCH_SIZE")
                .unwrap_or_else(|_| "100".to_string())
                .parse()?,
            default_retry_policy: None,
            dlq_topic_prefix: env::var("KAFKA_DLQ_TOPIC_PREFIX").ok(),
            drain_timeout_secs: env::var("KAFKA_DRAIN_TIMEOUT_SECS")
                .ok()
                .map(|s| s.parse())
                .transpose()?,
            num_workers: env::var("KAFKA_NUM_WORKERS")
                .ok()
                .map(|s| s.parse())
                .transpose()?,
            enable_auto_offset_store: env::var("KAFKA_ENABLE_AUTO_OFFSET_STORE")
                .ok()
                .map(|s| s.parse())
                .transpose()?,
            observability_config: None,
            handler_timeout_ms: env::var("KAFKA_HANDLER_TIMEOUT_MS")
                .ok()
                .map(|s| s.parse())
                .transpose()?,
        })
    }
}

#[pymethods]
impl ProducerConfig {
    // Kept for backward compatibility — Python callers using positional args still work.
    // The builder pattern is the preferred API: ProducerConfigBuilder::new().brokers(...).build()
    #[allow(clippy::too_many_arguments)]
    #[new]
    #[pyo3(signature = (
        brokers,
        message_timeout_ms = 30000,
        queue_buffering_max_messages = 100000,
        queue_buffering_max_kbytes = 1048576,
        batch_num_messages = 10000,
        compression_type = "snappy".to_string(),
        linger_ms = 5,
        request_timeout_ms = 30000,
        retry_backoff_ms = 100,
        retries = 2147483647,
        max_in_flight = 5,
        enable_idempotence = true,
        acks = "all".to_string(),
        security_protocol = None,
        sasl_mechanism = None,
        sasl_username = None,
        sasl_password = None,
    ))]
    pub fn new(
        brokers: String,
        message_timeout_ms: u64,
        queue_buffering_max_messages: u32,
        queue_buffering_max_kbytes: u32,
        batch_num_messages: u32,
        compression_type: String,
        linger_ms: u32,
        request_timeout_ms: u32,
        retry_backoff_ms: u32,
        retries: u32,
        max_in_flight: u32,
        enable_idempotence: bool,
        acks: String,
        security_protocol: Option<String>,
        sasl_mechanism: Option<String>,
        sasl_username: Option<String>,
        sasl_password: Option<String>,
    ) -> Self {
        ProducerConfig {
            brokers,
            message_timeout_ms,
            queue_buffering_max_messages,
            queue_buffering_max_kbytes,
            batch_num_messages,
            compression_type,
            linger_ms,
            request_timeout_ms,
            retry_backoff_ms,
            retries,
            max_in_flight,
            enable_idempotence,
            acks,
            security_protocol,
            sasl_mechanism,
            sasl_username,
            sasl_password,
        }
    }

    #[staticmethod]
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
            // Fallback to loading .env from current directory
            dotenvy::dotenv().ok();
        }

        Ok(ProducerConfig {
            brokers: env::var("KAFKA_BROKERS").unwrap_or_else(|_| "localhost:9092".to_string()),
            message_timeout_ms: env::var("PRODUCER_MESSAGE_TIMEOUT_MS")
                .unwrap_or_else(|_| "30000".to_string())
                .parse()?,
            queue_buffering_max_messages: env::var("PRODUCER_QUEUE_BUFFERING_MAX_MESSAGES")
                .unwrap_or_else(|_| "100000".to_string())
                .parse()?,
            queue_buffering_max_kbytes: env::var("PRODUCER_QUEUE_BUFFERING_MAX_KBYTES")
                .unwrap_or_else(|_| "1048576".to_string())
                .parse()?,
            batch_num_messages: env::var("PRODUCER_BATCH_NUM_MESSAGES")
                .unwrap_or_else(|_| "10000".to_string())
                .parse()?,
            compression_type: env::var("PRODUCER_COMPRESSION_TYPE")
                .unwrap_or_else(|_| "snappy".to_string()),
            linger_ms: env::var("PRODUCER_LINGER_MS")
                .unwrap_or_else(|_| "5".to_string())
                .parse()?,
            request_timeout_ms: env::var("PRODUCER_REQUEST_TIMEOUT_MS")
                .unwrap_or_else(|_| "30000".to_string())
                .parse()?,
            retry_backoff_ms: env::var("PRODUCER_RETRY_BACKOFF_MS")
                .unwrap_or_else(|_| "100".to_string())
                .parse()?,
            retries: env::var("PRODUCER_RETRIES")
                .unwrap_or_else(|_| "2147483647".to_string())
                .parse()?,
            max_in_flight: env::var("PRODUCER_MAX_IN_FLIGHT")
                .unwrap_or_else(|_| "5".to_string())
                .parse()?,
            enable_idempotence: env::var("PRODUCER_ENABLE_IDEMPOTENCE")
                .unwrap_or_else(|_| "true".to_string())
                .parse()?,
            acks: env::var("PRODUCER_ACKS").unwrap_or_else(|_| "all".to_string()),
            security_protocol: env::var("KAFKA_SECURITY_PROTOCOL").ok(),
            sasl_mechanism: env::var("KAFKA_SASL_MECHANISM").ok(),
            sasl_username: env::var("KAFKA_SASL_USERNAME").ok(),
            sasl_password: env::var("KAFKA_SASL_PASSWORD").ok(),
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("missing required field: {0}")]
    MissingField(&'static str),
    #[error("at least one topic must be specified")]
    NoTopics,
}

use std::sync::RwLock;

/// Helper to create a clone of ProducerConfigBuilder for fluent builder pattern.
fn make_producer_builder_clone(b: &ProducerConfigBuilder) -> ProducerConfigBuilder {
    ProducerConfigBuilder {
        brokers: RwLock::new(b.brokers.read().unwrap().clone()),
        message_timeout_ms: b.message_timeout_ms,
        queue_buffering_max_messages: b.queue_buffering_max_messages,
        queue_buffering_max_kbytes: b.queue_buffering_max_kbytes,
        batch_num_messages: b.batch_num_messages,
        compression_type: b.compression_type.clone(),
        linger_ms: b.linger_ms,
        request_timeout_ms: b.request_timeout_ms,
        retry_backoff_ms: b.retry_backoff_ms,
        retries: b.retries,
        max_in_flight: b.max_in_flight,
        enable_idempotence: b.enable_idempotence,
        acks: b.acks.clone(),
        security_protocol: RwLock::new(b.security_protocol.read().unwrap().clone()),
        sasl_mechanism: RwLock::new(b.sasl_mechanism.read().unwrap().clone()),
        sasl_username: RwLock::new(b.sasl_username.read().unwrap().clone()),
        sasl_password: RwLock::new(b.sasl_password.read().unwrap().clone()),
    }
}

#[derive(Debug)]
#[pyclass]
pub struct ProducerConfigBuilder {
    brokers: RwLock<Option<String>>,
    message_timeout_ms: u64,
    queue_buffering_max_messages: u32,
    queue_buffering_max_kbytes: u32,
    batch_num_messages: u32,
    compression_type: String,
    linger_ms: u32,
    request_timeout_ms: u32,
    retry_backoff_ms: u32,
    retries: u32,
    max_in_flight: u32,
    enable_idempotence: bool,
    acks: String,
    security_protocol: RwLock<Option<String>>,
    sasl_mechanism: RwLock<Option<String>>,
    sasl_username: RwLock<Option<String>>,
    sasl_password: RwLock<Option<String>>,
}

impl Clone for ProducerConfigBuilder {
    fn clone(&self) -> Self {
        make_producer_builder_clone(self)
    }
}

#[pymethods]
impl ProducerConfigBuilder {
    #[new]
    #[pyo3(signature = ())]
    pub fn new() -> Self {
        Self {
            brokers: RwLock::new(None),
            message_timeout_ms: 30000,
            queue_buffering_max_messages: 100000,
            queue_buffering_max_kbytes: 1048576,
            batch_num_messages: 10000,
            compression_type: "snappy".to_string(),
            linger_ms: 5,
            request_timeout_ms: 30000,
            retry_backoff_ms: 100,
            retries: 2147483647,
            max_in_flight: 5,
            enable_idempotence: true,
            acks: "all".to_string(),
            security_protocol: RwLock::new(None),
            sasl_mechanism: RwLock::new(None),
            sasl_username: RwLock::new(None),
            sasl_password: RwLock::new(None),
        }
    }

    pub fn brokers(&mut self, brokers: String) -> Self {
        *self.brokers.write().unwrap() = Some(brokers);
        make_producer_builder_clone(self)
    }

    pub fn message_timeout_ms(&mut self, ms: u64) -> Self {
        self.message_timeout_ms = ms;
        make_producer_builder_clone(self)
    }

    pub fn queue_buffering_max_messages(&mut self, n: u32) -> Self {
        self.queue_buffering_max_messages = n;
        make_producer_builder_clone(self)
    }

    pub fn queue_buffering_max_kbytes(&mut self, kbytes: u32) -> Self {
        self.queue_buffering_max_kbytes = kbytes;
        make_producer_builder_clone(self)
    }

    pub fn batch_num_messages(&mut self, n: u32) -> Self {
        self.batch_num_messages = n;
        make_producer_builder_clone(self)
    }

    pub fn compression_type(&mut self, ct: String) -> Self {
        self.compression_type = ct;
        make_producer_builder_clone(self)
    }

    pub fn linger_ms(&mut self, ms: u32) -> Self {
        self.linger_ms = ms;
        make_producer_builder_clone(self)
    }

    pub fn request_timeout_ms(&mut self, ms: u32) -> Self {
        self.request_timeout_ms = ms;
        make_producer_builder_clone(self)
    }

    pub fn retry_backoff_ms(&mut self, ms: u32) -> Self {
        self.retry_backoff_ms = ms;
        make_producer_builder_clone(self)
    }

    pub fn retries(&mut self, n: u32) -> Self {
        self.retries = n;
        make_producer_builder_clone(self)
    }

    pub fn max_in_flight(&mut self, n: u32) -> Self {
        self.max_in_flight = n;
        make_producer_builder_clone(self)
    }

    pub fn enable_idempotence(&mut self, enabled: bool) -> Self {
        self.enable_idempotence = enabled;
        make_producer_builder_clone(self)
    }

    pub fn acks(&mut self, acks: String) -> Self {
        self.acks = acks;
        make_producer_builder_clone(self)
    }

    pub fn security_protocol(&mut self, proto: String) -> Self {
        *self.security_protocol.write().unwrap() = Some(proto);
        make_producer_builder_clone(self)
    }

    pub fn sasl_mechanism(&mut self, mechanism: String, username: String, password: String) -> Self {
        *self.sasl_mechanism.write().unwrap() = Some(mechanism);
        *self.sasl_username.write().unwrap() = Some(username);
        *self.sasl_password.write().unwrap() = Some(password);
        make_producer_builder_clone(self)
    }

    pub fn build(&mut self) -> PyResult<ProducerConfig> {
        let brokers = self.brokers.read().unwrap().clone().ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>("missing required field: brokers")
        })?;
        Ok(ProducerConfig {
            brokers,
            message_timeout_ms: self.message_timeout_ms,
            queue_buffering_max_messages: self.queue_buffering_max_messages,
            queue_buffering_max_kbytes: self.queue_buffering_max_kbytes,
            batch_num_messages: self.batch_num_messages,
            compression_type: self.compression_type.clone(),
            linger_ms: self.linger_ms,
            request_timeout_ms: self.request_timeout_ms,
            retry_backoff_ms: self.retry_backoff_ms,
            retries: self.retries,
            max_in_flight: self.max_in_flight,
            enable_idempotence: self.enable_idempotence,
            acks: self.acks.clone(),
            security_protocol: self.security_protocol.read().unwrap().clone(),
            sasl_mechanism: self.sasl_mechanism.read().unwrap().clone(),
            sasl_username: self.sasl_username.read().unwrap().clone(),
            sasl_password: self.sasl_password.read().unwrap().clone(),
        })
    }
}

/// Helper to create a clone of ConsumerConfigBuilder for fluent builder pattern.
fn make_consumer_builder_clone(b: &ConsumerConfigBuilder) -> ConsumerConfigBuilder {
    ConsumerConfigBuilder {
        brokers: RwLock::new(b.brokers.read().unwrap().clone()),
        group_id: RwLock::new(b.group_id.read().unwrap().clone()),
        topics: RwLock::new(b.topics.read().unwrap().clone()),
        auto_offset_reset: b.auto_offset_reset.clone(),
        enable_auto_commit: b.enable_auto_commit,
        session_timeout_ms: b.session_timeout_ms,
        heartbeat_interval_ms: b.heartbeat_interval_ms,
        max_poll_interval_ms: b.max_poll_interval_ms,
        security_protocol: RwLock::new(b.security_protocol.read().unwrap().clone()),
        sasl_mechanism: RwLock::new(b.sasl_mechanism.read().unwrap().clone()),
        sasl_username: RwLock::new(b.sasl_username.read().unwrap().clone()),
        sasl_password: RwLock::new(b.sasl_password.read().unwrap().clone()),
        fetch_min_bytes: b.fetch_min_bytes,
        max_partition_fetch_bytes: b.max_partition_fetch_bytes,
        partition_assignment_strategy: b.partition_assignment_strategy.clone(),
        retry_backoff_ms: b.retry_backoff_ms,
        message_batch_size: b.message_batch_size,
        default_retry_policy: RwLock::new(b.default_retry_policy.read().unwrap().clone()),
        dlq_topic_prefix: RwLock::new(b.dlq_topic_prefix.read().unwrap().clone()),
        drain_timeout_secs: RwLock::new(b.drain_timeout_secs.read().unwrap().clone()),
        num_workers: RwLock::new(b.num_workers.read().unwrap().clone()),
        enable_auto_offset_store: RwLock::new(b.enable_auto_offset_store.read().unwrap().clone()),
        observability_config: RwLock::new(b.observability_config.read().unwrap().clone()),
        handler_timeout_ms: RwLock::new(b.handler_timeout_ms.read().unwrap().clone()),
    }
}

#[derive(Debug)]
#[pyclass]
pub struct ConsumerConfigBuilder {
    brokers: RwLock<Option<String>>,
    group_id: RwLock<Option<String>>,
    topics: RwLock<Vec<String>>,
    auto_offset_reset: String,
    enable_auto_commit: bool,
    session_timeout_ms: u32,
    heartbeat_interval_ms: u32,
    max_poll_interval_ms: u32,
    security_protocol: RwLock<Option<String>>,
    sasl_mechanism: RwLock<Option<String>>,
    sasl_username: RwLock<Option<String>>,
    sasl_password: RwLock<Option<String>>,
    fetch_min_bytes: u32,
    max_partition_fetch_bytes: u32,
    partition_assignment_strategy: String,
    retry_backoff_ms: u32,
    message_batch_size: usize,
    default_retry_policy: RwLock<Option<PyRetryPolicy>>,
    dlq_topic_prefix: RwLock<Option<String>>,
    drain_timeout_secs: RwLock<Option<u64>>,
    num_workers: RwLock<Option<u32>>,
    enable_auto_offset_store: RwLock<Option<bool>>,
    observability_config: RwLock<Option<PyObservabilityConfig>>,
    handler_timeout_ms: RwLock<Option<u64>>,
}

impl Clone for ConsumerConfigBuilder {
    fn clone(&self) -> Self {
        make_consumer_builder_clone(self)
    }
}

#[pymethods]
impl ConsumerConfigBuilder {
    #[new]
    #[pyo3(signature = ())]
    pub fn new() -> Self {
        Self {
            brokers: RwLock::new(None),
            group_id: RwLock::new(None),
            topics: RwLock::new(Vec::new()),
            auto_offset_reset: "earliest".to_string(),
            enable_auto_commit: false,
            session_timeout_ms: 30000,
            heartbeat_interval_ms: 3000,
            max_poll_interval_ms: 300000,
            security_protocol: RwLock::new(None),
            sasl_mechanism: RwLock::new(None),
            sasl_username: RwLock::new(None),
            sasl_password: RwLock::new(None),
            fetch_min_bytes: 1,
            max_partition_fetch_bytes: 1048576,
            partition_assignment_strategy: "roundrobin".to_string(),
            retry_backoff_ms: 100,
            message_batch_size: 100,
            default_retry_policy: RwLock::new(None),
            dlq_topic_prefix: RwLock::new(None),
            drain_timeout_secs: RwLock::new(None),
            num_workers: RwLock::new(None),
            enable_auto_offset_store: RwLock::new(None),
            observability_config: RwLock::new(None),
            handler_timeout_ms: RwLock::new(None),
        }
    }

    pub fn brokers(&mut self, brokers: String) -> Self {
        *self.brokers.write().unwrap() = Some(brokers);
        make_consumer_builder_clone(self)
    }

    pub fn group_id(&mut self, group_id: String) -> Self {
        *self.group_id.write().unwrap() = Some(group_id);
        make_consumer_builder_clone(self)
    }

    pub fn topics(&mut self, topics: Vec<String>) -> Self {
        *self.topics.write().unwrap() = topics;
        make_consumer_builder_clone(self)
    }

    pub fn add_topic(&mut self, topic: String) -> Self {
        self.topics.write().unwrap().push(topic);
        make_consumer_builder_clone(self)
    }

    pub fn auto_offset_reset(&mut self, reset: String) -> Self {
        self.auto_offset_reset = reset;
        make_consumer_builder_clone(self)
    }

    pub fn enable_auto_commit(&mut self, enabled: bool) -> Self {
        self.enable_auto_commit = enabled;
        make_consumer_builder_clone(self)
    }

    pub fn session_timeout_ms(&mut self, ms: u32) -> Self {
        self.session_timeout_ms = ms;
        make_consumer_builder_clone(self)
    }

    pub fn heartbeat_interval_ms(&mut self, ms: u32) -> Self {
        self.heartbeat_interval_ms = ms;
        make_consumer_builder_clone(self)
    }

    pub fn max_poll_interval_ms(&mut self, ms: u32) -> Self {
        self.max_poll_interval_ms = ms;
        make_consumer_builder_clone(self)
    }

    pub fn security_protocol(&mut self, proto: String) -> Self {
        *self.security_protocol.write().unwrap() = Some(proto);
        make_consumer_builder_clone(self)
    }

    pub fn sasl_mechanism(&mut self, mechanism: String, username: String, password: String) -> Self {
        *self.sasl_mechanism.write().unwrap() = Some(mechanism);
        *self.sasl_username.write().unwrap() = Some(username);
        *self.sasl_password.write().unwrap() = Some(password);
        make_consumer_builder_clone(self)
    }

    pub fn fetch_min_bytes(&mut self, bytes: u32) -> Self {
        self.fetch_min_bytes = bytes;
        make_consumer_builder_clone(self)
    }

    pub fn max_partition_fetch_bytes(&mut self, bytes: u32) -> Self {
        self.max_partition_fetch_bytes = bytes;
        make_consumer_builder_clone(self)
    }

    pub fn partition_assignment_strategy(&mut self, strategy: String) -> Self {
        self.partition_assignment_strategy = strategy;
        make_consumer_builder_clone(self)
    }

    pub fn retry_backoff_ms(&mut self, ms: u32) -> Self {
        self.retry_backoff_ms = ms;
        make_consumer_builder_clone(self)
    }

    pub fn message_batch_size(&mut self, size: usize) -> Self {
        self.message_batch_size = size;
        make_consumer_builder_clone(self)
    }

    pub fn default_retry_policy(&mut self, policy: PyRetryPolicy) -> Self {
        *self.default_retry_policy.write().unwrap() = Some(policy);
        make_consumer_builder_clone(self)
    }

    pub fn dlq_topic_prefix(&mut self, prefix: String) -> Self {
        *self.dlq_topic_prefix.write().unwrap() = Some(prefix);
        make_consumer_builder_clone(self)
    }

    pub fn drain_timeout_secs(&mut self, secs: u64) -> Self {
        *self.drain_timeout_secs.write().unwrap() = Some(secs);
        make_consumer_builder_clone(self)
    }

    pub fn num_workers(&mut self, n: u32) -> Self {
        *self.num_workers.write().unwrap() = Some(n);
        make_consumer_builder_clone(self)
    }

    pub fn enable_auto_offset_store(&mut self, enabled: bool) -> Self {
        *self.enable_auto_offset_store.write().unwrap() = Some(enabled);
        make_consumer_builder_clone(self)
    }

    pub fn observability_config(&mut self, config: PyObservabilityConfig) -> Self {
        *self.observability_config.write().unwrap() = Some(config);
        make_consumer_builder_clone(self)
    }

    pub fn handler_timeout_ms(&mut self, ms: u64) -> Self {
        *self.handler_timeout_ms.write().unwrap() = Some(ms);
        make_consumer_builder_clone(self)
    }

    pub fn build(&mut self) -> PyResult<ConsumerConfig> {
        let brokers = self.brokers.read().unwrap().clone().ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>("missing required field: brokers")
        })?;
        let group_id = self.group_id.read().unwrap().clone().ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyValueError, _>("missing required field: group_id")
        })?;
        let topics = self.topics.read().unwrap().clone();
        if topics.is_empty() {
            return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                "at least one topic must be specified",
            ));
        }
        Ok(ConsumerConfig {
            brokers,
            group_id,
            topics,
            auto_offset_reset: self.auto_offset_reset.clone(),
            enable_auto_commit: self.enable_auto_commit,
            session_timeout_ms: self.session_timeout_ms,
            heartbeat_interval_ms: self.heartbeat_interval_ms,
            max_poll_interval_ms: self.max_poll_interval_ms,
            security_protocol: self.security_protocol.read().unwrap().clone(),
            sasl_mechanism: self.sasl_mechanism.read().unwrap().clone(),
            sasl_username: self.sasl_username.read().unwrap().clone(),
            sasl_password: self.sasl_password.read().unwrap().clone(),
            fetch_min_bytes: self.fetch_min_bytes,
            max_partition_fetch_bytes: self.max_partition_fetch_bytes,
            partition_assignment_strategy: self.partition_assignment_strategy.clone(),
            retry_backoff_ms: self.retry_backoff_ms,
            message_batch_size: self.message_batch_size,
            default_retry_policy: self.default_retry_policy.read().unwrap().clone(),
            dlq_topic_prefix: self.dlq_topic_prefix.read().unwrap().clone(),
            drain_timeout_secs: self.drain_timeout_secs.read().unwrap().clone(),
            num_workers: self.num_workers.read().unwrap().clone(),
            enable_auto_offset_store: self.enable_auto_offset_store.read().unwrap().clone(),
            observability_config: self.observability_config.read().unwrap().clone(),
            handler_timeout_ms: self.handler_timeout_ms.read().unwrap().clone(),
        })
    }
}
