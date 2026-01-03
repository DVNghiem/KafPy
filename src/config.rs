use pyo3::prelude::*;
use std::collections::HashMap;
use std::env;
use std::time::Duration;

#[derive(Debug, Clone)]
#[pyclass]
pub struct KafkaConfig {
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

#[pymethods]
impl KafkaConfig {

    #[new]
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
        KafkaConfig {
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
    
}

#[derive(Debug, Clone)]
#[pyclass]
pub struct AppConfig {
    pub kafka: KafkaConfig,
    pub processing_timeout_ms: u64,
    pub graceful_shutdown_timeout_ms: u64,
}

#[pymethods]
impl AppConfig {
    #[new]
    pub fn new(
        kafka: KafkaConfig,
        processing_timeout_ms: u64,
        graceful_shutdown_timeout_ms: u64,
    ) -> Self {
        AppConfig {
            kafka,
            processing_timeout_ms,
            graceful_shutdown_timeout_ms,
        }
    }

    pub fn processing_timeout(&self) -> Duration {
        Duration::from_millis(self.processing_timeout_ms)
    }

    pub fn graceful_shutdown_timeout(&self) -> Duration {
        Duration::from_millis(self.graceful_shutdown_timeout_ms)
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

        let config = AppConfig {
            processing_timeout_ms: env::var("PROCESSING_TIMEOUT_MS")
                .unwrap_or_else(|_| "30000".to_string())
                .parse()?,
            graceful_shutdown_timeout_ms: env::var("GRACEFUL_SHUTDOWN_TIMEOUT_MS")
                .unwrap_or_else(|_| "10000".to_string())
                .parse()?,

            kafka: KafkaConfig {
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
            },
        };

        Ok(config)
    }

    pub fn get_kafka_properties(&self) -> HashMap<String, String> {
        let mut props = HashMap::new();

        props.insert("bootstrap.servers".to_string(), self.kafka.brokers.clone());
        props.insert("group.id".to_string(), self.kafka.group_id.clone());
        props.insert(
            "fetch.min.bytes".to_string(),
            self.kafka.fetch_min_bytes.to_string(),
        );
        props.insert(
            "max.partition.fetch.bytes".to_string(),
            self.kafka.max_partition_fetch_bytes.to_string(),
        );
        props.insert(
            "enable.auto.commit".to_string(),
            self.kafka.enable_auto_commit.to_string(),
        );
        props.insert(
            "auto.offset.reset".to_string(),
            self.kafka.auto_offset_reset.clone(),
        );
        props.insert(
            "session.timeout.ms".to_string(),
            self.kafka.session_timeout_ms.to_string(),
        );
        props.insert(
            "heartbeat.interval.ms".to_string(),
            self.kafka.heartbeat_interval_ms.to_string(),
        );
        props.insert(
            "max.poll.interval.ms".to_string(),
            self.kafka.max_poll_interval_ms.to_string(),
        );
        props.insert(
            "partition.assignment.strategy".to_string(),
            self.kafka.partition_assignment_strategy.clone(),
        );
        props.insert(
            "retry.backoff.ms".to_string(),
            self.kafka.retry_backoff_ms.to_string(),
        );

        // Performance optimizations
        props.insert("socket.keepalive.enable".to_string(), "true".to_string());
        props.insert("socket.max.fails".to_string(), "3".to_string());
        props.insert("reconnect.backoff.max.ms".to_string(), "10000".to_string());

        props
    }
}
