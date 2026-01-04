use pyo3::prelude::*;
use std::env;
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
        })
    }
}

#[pymethods]
impl ProducerConfig {
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
