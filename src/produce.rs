use pyo3::prelude::*;
use rdkafka::{
    config::ClientConfig,
    producer::{FutureProducer, FutureRecord, Producer},
    util::Timeout,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

use crate::config::ProducerConfig;

#[pyclass(name = "Producer")]
pub struct PyProducer {
    producer: Arc<RwLock<Option<FutureProducer>>>,
    config: ProducerConfig,
}

#[pymethods]
impl PyProducer {
    #[new]
    pub fn new(config: ProducerConfig) -> PyResult<Self> {
        Ok(Self {
            producer: Arc::new(RwLock::new(None)),
            config,
        })
    }

    /// Initialize the producer (must be called before sending messages)
    pub fn init(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let config = self.config.clone();
        let producer_lock = Arc::clone(&self.producer);

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let producer = Self::create_producer(&config)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

            let mut lock = producer_lock.write().await;
            *lock = Some(producer);

            info!("Kafka producer initialized successfully");

            let res: PyResult<Py<PyAny>> = Python::attach(|py| Ok(py.None()));
            res
        })
        .map(|b| b.unbind())
    }

    /// Send a message to Kafka asynchronously
    pub fn send(
        &self,
        py: Python<'_>,
        topic: String,
        key: Option<Vec<u8>>,
        payload: Option<Vec<u8>>,
        partition: Option<i32>,
        headers: Option<Vec<(String, Vec<u8>)>>,
    ) -> PyResult<Py<PyAny>> {
        let producer_lock = Arc::clone(&self.producer);
        let timeout = self.config.message_timeout_ms;

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let lock = producer_lock.read().await;
            let producer = lock
                .as_ref()
                .ok_or_else(|| {
                    PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                        "Producer not initialized. Call init() first.",
                    )
                })?;

            let mut record = FutureRecord::to(&topic);

            if let Some(k) = key.as_ref() {
                record = record.key(k);
            }

            if let Some(p) = payload.as_ref() {
                record = record.payload(p);
            }

            if let Some(part) = partition {
                record = record.partition(part);
            }

            // Add headers if provided
            if let Some(hdrs) = headers {
                let mut kafka_headers = rdkafka::message::OwnedHeaders::new();
                for (key, value) in hdrs {
                    kafka_headers = kafka_headers.insert(rdkafka::message::Header {
                        key: &key,
                        value: Some(&value),
                    });
                }
                record = record.headers(kafka_headers);
            }

            let delivery_result = producer
                .send(record, Timeout::After(Duration::from_millis(timeout)))
                .await;

            match delivery_result {
                Ok(delivery) => {
                    debug!(
                        "Message delivered to topic '{}', partition: {}, offset: {}",
                        topic, delivery.partition, delivery.offset
                    );
                    let res: PyResult<(i32, i64)> = Ok((delivery.partition, delivery.offset));
                    res
                }
                Err((kafka_err, _)) => {
                    error!("Failed to deliver message: {:?}", kafka_err);
                    Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                        "Failed to deliver message: {:?}",
                        kafka_err
                    )))
                }
            }
        })
        .map(|b| b.unbind())
    }

    /// Send a message synchronously (convenience method)
    pub fn send_sync(
        &self,
        topic: String,
        key: Option<Vec<u8>>,
        payload: Option<Vec<u8>>,
        partition: Option<i32>,
        headers: Option<Vec<(String, Vec<u8>)>>,
    ) -> PyResult<(i32, i64)> {
        let producer_lock = self.producer.clone();
        let timeout = self.config.message_timeout_ms;

        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        rt.block_on(async {
            let lock = producer_lock.read().await;
            let producer = lock.as_ref().ok_or_else(|| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                    "Producer not initialized. Call init() first.",
                )
            })?;

            let mut record = FutureRecord::to(&topic);

            if let Some(k) = key.as_ref() {
                record = record.key(k);
            }

            if let Some(p) = payload.as_ref() {
                record = record.payload(p);
            }

            if let Some(part) = partition {
                record = record.partition(part);
            }

            // Add headers if provided
            if let Some(hdrs) = headers {
                let mut kafka_headers = rdkafka::message::OwnedHeaders::new();
                for (key, value) in hdrs {
                    kafka_headers = kafka_headers.insert(rdkafka::message::Header {
                        key: &key,
                        value: Some(&value),
                    });
                }
                record = record.headers(kafka_headers);
            }

            let delivery_result = producer
                .send(record, Timeout::After(Duration::from_millis(timeout)))
                .await;

            match delivery_result {
                Ok(delivery) => {
                    debug!(
                        "Message delivered to topic '{}', partition: {}, offset: {}",
                        topic, delivery.partition, delivery.offset
                    );
                    Ok((delivery.partition, delivery.offset))
                }
                Err((kafka_err, _)) => {
                    error!("Failed to deliver message: {:?}", kafka_err);
                    Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                        "Failed to deliver message: {:?}",
                        kafka_err
                    )))
                }
            }
        })
    }

    /// Flush all pending messages
    pub fn flush(&self, py: Python<'_>, timeout_ms: Option<u64>) -> PyResult<Py<PyAny>> {
        let producer_lock = Arc::clone(&self.producer);
        let timeout = timeout_ms.unwrap_or(10000);

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let lock = producer_lock.read().await;
            let producer = lock.as_ref().ok_or_else(|| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                    "Producer not initialized. Call init() first.",
                )
            })?;

            producer
                .flush(Timeout::After(Duration::from_millis(timeout)))
                .map_err(|e| {
                    PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                        "Failed to flush producer: {:?}",
                        e
                    ))
                })?;

            info!("Producer flushed successfully");

            let res: PyResult<Py<PyAny>> = Python::attach(|py| Ok(py.None()));
            res
        })
        .map(|b| b.unbind())
    }

    /// Get the number of messages waiting to be sent
    pub fn in_flight_count(&self) -> PyResult<i32> {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

        rt.block_on(async {
            let lock = self.producer.read().await;
            let producer = lock.as_ref().ok_or_else(|| {
                PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                    "Producer not initialized. Call init() first.",
                )
            })?;

            Ok(producer.in_flight_count())
        })
    }
}

impl PyProducer {
    fn create_producer(config: &ProducerConfig) -> Result<FutureProducer, rdkafka::error::KafkaError> {
        let mut client_config = ClientConfig::new();

        // Basic configuration
        client_config
            .set("bootstrap.servers", &config.brokers)
            .set("message.timeout.ms", config.message_timeout_ms.to_string())
            .set("queue.buffering.max.messages", config.queue_buffering_max_messages.to_string())
            .set("queue.buffering.max.kbytes", config.queue_buffering_max_kbytes.to_string())
            .set("batch.num.messages", config.batch_num_messages.to_string())
            .set("compression.type", &config.compression_type)
            .set("linger.ms", config.linger_ms.to_string())
            .set("request.timeout.ms", config.request_timeout_ms.to_string())
            .set("retry.backoff.ms", config.retry_backoff_ms.to_string())
            .set("retries", config.retries.to_string())
            .set("max.in.flight.requests.per.connection", config.max_in_flight.to_string())
            .set("enable.idempotence", config.enable_idempotence.to_string())
            .set("acks", &config.acks);

        // Security configuration
        if let Some(security_protocol) = &config.security_protocol {
            client_config.set("security.protocol", security_protocol);
        }

        if let Some(sasl_mechanism) = &config.sasl_mechanism {
            client_config.set("sasl.mechanism", sasl_mechanism);
        }

        if let Some(sasl_username) = &config.sasl_username {
            client_config.set("sasl.username", sasl_username);
        }

        if let Some(sasl_password) = &config.sasl_password {
            client_config.set("sasl.password", sasl_password);
        }

        client_config.create()
    }
}
