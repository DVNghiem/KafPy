use crate::log::{debug, error, info};
use pyo3::prelude::*;
use rdkafka::{
    config::ClientConfig,
    producer::{FutureProducer, FutureRecord, Producer},
    util::Timeout,
};
use std::sync::Arc;
use std::thread::{JoinHandle, spawn};
use std::time::Duration;
use tokio::sync::mpsc::{channel, Sender};
use tokio::sync::RwLock;
use parking_lot::Mutex;

use crate::config::ProducerConfig;

/// Task sent from PyProducer methods to the background worker thread
enum ProducerTask {
    Init {
        result_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    Send {
        topic: String,
        key: Option<Vec<u8>>,
        payload: Option<Vec<u8>>,
        partition: Option<i32>,
        headers: Option<Vec<(String, Vec<u8>)>>,
        result_tx: tokio::sync::oneshot::Sender<Result<(i32, i64), String>>,
    },
    Flush {
        timeout_ms: u64,
        result_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    InFlightCount {
        result_tx: tokio::sync::oneshot::Sender<Result<i32, String>>,
    },
}

#[pyclass(name = "Producer")]
pub struct PyProducer {
    producer: Arc<RwLock<Option<FutureProducer>>>,
    config: ProducerConfig,
    task_tx: Arc<Mutex<Option<Sender<ProducerTask>>>>,
    worker_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
}

#[pymethods]
impl PyProducer {
    #[new]
    pub fn new(config: ProducerConfig) -> PyResult<Self> {
        Ok(Self {
            producer: Arc::new(RwLock::new(None)),
            config,
            task_tx: Arc::new(Mutex::new(None)),
            worker_handle: Arc::new(Mutex::new(None)),
        })
    }

    /// Initialize the producer (must be called before sending messages)
    pub fn init(&self) -> PyResult<()> {
        let config = self.config.clone();
        let producer_lock = Arc::clone(&self.producer);

        // Create channel for sending tasks to the worker thread
        let (task_tx, mut task_rx) = channel::<ProducerTask>(100);

        // Store the task sender before spawning the thread
        {
            let mut tx_guard = self.task_tx.lock();
            *tx_guard = Some(task_tx.clone());
        }

        // Spawn the background worker thread
        let handle = spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    error!("Failed to create Tokio runtime for producer: {}", e);
                    return;
                }
            };
            rt.block_on(async {
                loop {
                    // Use select! to listen for shutdown or tasks
                    tokio::select! {
                        Some(task) = task_rx.recv() => {
                            match task {
                                ProducerTask::Init { result_tx } => {
                                    let result = Self::create_producer(&config)
                                        .map_err(|e| e.to_string());
                                    match result {
                                        Ok(producer) => {
                                            let mut lock = producer_lock.write().await;
                                            *lock = Some(producer);
                                            let _ = result_tx.send(Ok(()));
                                            info!("Kafka producer initialized successfully");
                                        }
                                        Err(e) => {
                                            let _ = result_tx.send(Err(e));
                                        }
                                    }
                                }
                                ProducerTask::Send { topic, key, payload, partition, headers, result_tx } => {
                                    let result = Self::do_send(&producer_lock, &topic, key, payload, partition, headers).await;
                                    let _ = result_tx.send(result);
                                }
                                ProducerTask::Flush { timeout_ms, result_tx } => {
                                    let result = Self::do_flush(&producer_lock, timeout_ms).await;
                                    let _ = result_tx.send(result);
                                }
                                ProducerTask::InFlightCount { result_tx } => {
                                    let result = Self::do_in_flight_count(&producer_lock).await;
                                    let _ = result_tx.send(result);
                                }
                            }
                        }
                        else => {
                            // Channel closed, exit worker
                            break;
                        }
                    }
                }
            });
        });

        // Store the worker handle
        {
            let mut handle_guard = self.worker_handle.lock();
            *handle_guard = Some(handle);
        }

        // Send init task and wait for result
        let (result_tx, result_rx) = tokio::sync::oneshot::channel();
        let task = ProducerTask::Init { result_tx };

        // Use blocking send to send the task
        {
            let tx = self.task_tx.lock();
            if let Some(tx) = tx.as_ref() {
                tx.blocking_send(task)
                    .map_err(|_| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Worker channel closed"))?;
            }
        }

        // Wait for the init result using blocking recv
        match result_rx.blocking_recv() {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e)),
            Err(_) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Worker thread died during producer init")),
        }
    }

    /// Send a message to Kafka asynchronously
    pub fn send(
        &self,
        topic: String,
        key: Option<Vec<u8>>,
        payload: Option<Vec<u8>>,
        partition: Option<i32>,
        headers: Option<Vec<(String, Vec<u8>)>>,
    ) -> PyResult<(i32, i64)> {
        let (result_tx, result_rx) = tokio::sync::oneshot::channel();
        let task = ProducerTask::Send {
            topic: topic.clone(),
            key,
            payload,
            partition,
            headers,
            result_tx,
        };

        // Send task to worker using blocking send
        {
            let tx = self.task_tx.lock();
            if tx.is_none() {
                return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                    "Producer not initialized",
                ));
            }
            tx.as_ref().unwrap()
                .blocking_send(task)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        }

        // Wait for result using blocking recv (no runtime needed)
        match result_rx.blocking_recv() {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(e)) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e)),
            Err(_) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Worker thread died unexpectedly")),
        }
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
        // send_sync is identical to send in this implementation
        // since the worker thread handles all operations
        self.send(topic, key, payload, partition, headers)
    }

    /// Flush all pending messages
    pub fn flush(&self, timeout_ms: Option<u64>) -> PyResult<()> {
        let timeout = timeout_ms.unwrap_or(10000);

        let (result_tx, result_rx) = tokio::sync::oneshot::channel();
        let task = ProducerTask::Flush {
            timeout_ms: timeout,
            result_tx,
        };

        // Send task to worker using blocking send
        {
            let tx = self.task_tx.lock();
            if tx.is_none() {
                return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                    "Producer not initialized",
                ));
            }
            tx.as_ref().unwrap()
                .blocking_send(task)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        }

        // Wait for result using blocking recv (no runtime needed)
        match result_rx.blocking_recv() {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e)),
            Err(_) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Worker thread died unexpectedly")),
        }
    }

    /// Get the number of messages waiting to be sent
    pub fn in_flight_count(&self) -> PyResult<i32> {
        let (result_tx, result_rx) = tokio::sync::oneshot::channel();
        let task = ProducerTask::InFlightCount { result_tx };

        // Send task to worker using blocking send
        {
            let tx = self.task_tx.lock();
            if tx.is_none() {
                return Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                    "Producer not initialized",
                ));
            }
            tx.as_ref().unwrap()
                .blocking_send(task)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;
        }

        // Wait for result using blocking recv (no runtime needed)
        match result_rx.blocking_recv() {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(e)) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e)),
            Err(_) => Err(PyErr::new::<pyo3::exceptions::PyRuntimeError, _>("Worker thread died unexpectedly")),
        }
    }
}

impl PyProducer {
    fn create_producer(
        config: &ProducerConfig,
    ) -> Result<FutureProducer, rdkafka::error::KafkaError> {
        let mut client_config = ClientConfig::new();

        // Basic configuration
        client_config
            .set("bootstrap.servers", &config.brokers)
            .set("message.timeout.ms", config.message_timeout_ms.to_string())
            .set(
                "queue.buffering.max.messages",
                config.queue_buffering_max_messages.to_string(),
            )
            .set(
                "queue.buffering.max.kbytes",
                config.queue_buffering_max_kbytes.to_string(),
            )
            .set("batch.num.messages", config.batch_num_messages.to_string())
            .set("compression.type", &config.compression_type)
            .set("linger.ms", config.linger_ms.to_string())
            .set("request.timeout.ms", config.request_timeout_ms.to_string())
            .set("retry.backoff.ms", config.retry_backoff_ms.to_string())
            .set("retries", config.retries.to_string())
            .set(
                "max.in.flight.requests.per.connection",
                config.max_in_flight.to_string(),
            )
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

    async fn do_send(
        producer_lock: &Arc<RwLock<Option<FutureProducer>>>,
        topic: &str,
        key: Option<Vec<u8>>,
        payload: Option<Vec<u8>>,
        partition: Option<i32>,
        headers: Option<Vec<(String, Vec<u8>)>>,
    ) -> Result<(i32, i64), String> {
        let timeout_ms = 30000; // Default timeout

        let lock = producer_lock.read().await;
        let producer = lock.as_ref().ok_or_else(|| "Producer not initialized".to_string())?;

        let mut record = FutureRecord::to(topic);

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
            .send(record, Timeout::After(Duration::from_millis(timeout_ms)))
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
                Err(format!("Failed to deliver message: {:?}", kafka_err))
            }
        }
    }

    async fn do_flush(
        producer_lock: &Arc<RwLock<Option<FutureProducer>>>,
        timeout_ms: u64,
    ) -> Result<(), String> {
        let lock = producer_lock.read().await;
        let producer = lock.as_ref().ok_or_else(|| "Producer not initialized".to_string())?;

        producer
            .flush(Timeout::After(Duration::from_millis(timeout_ms)))
            .map_err(|e| format!("Failed to flush producer: {:?}", e))?;

        info!("Producer flushed successfully");
        Ok(())
    }

    async fn do_in_flight_count(
        producer_lock: &Arc<RwLock<Option<FutureProducer>>>,
    ) -> Result<i32, String> {
        let lock = producer_lock.read().await;
        let producer = lock.as_ref().ok_or_else(|| "Producer not initialized".to_string())?;

        Ok(producer.in_flight_count())
    }
}

impl Drop for PyProducer {
    fn drop(&mut self) {
        // Drop the task sender to close the channel and signal the worker to exit
        {
            let mut tx = self.task_tx.lock();
            *tx = None;
        }

        // Join the worker thread
        {
            let mut handle = self.worker_handle.lock();
            if let Some(h) = handle.take() {
                let _ = h.join();
            }
        }
    }
}
