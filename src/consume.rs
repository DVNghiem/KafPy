use pyo3::prelude::*;
use rdkafka::{
    config::ClientConfig,
    consumer::{Consumer, ConsumerContext, StreamConsumer},
    error::KafkaError,
    ClientContext,
};
use std::sync::Arc;
use tokio::{signal, sync::mpsc};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};

use crate::{
    config::{AppConfig, KafkaConfig},
    kafka_message::KafkaMessage,
    message_processor::{MessageProcessor, MessageRouter, PyProcessor},
};

#[pyclass(name="Consumer")]
pub struct PyConsumer {
    config: AppConfig,
    message_router: Arc<MessageRouter>,
    cancellation_token: CancellationToken,
}

#[pymethods]
impl PyConsumer {
    #[new]
    pub fn new(config: AppConfig) -> PyResult<Self> {
        Ok(Self {
            config,
            message_router: Arc::new(MessageRouter::new()),
            cancellation_token: CancellationToken::new(),
        })
    }

    pub fn add_handler(&mut self, topic: String, callback: Bound<'_, PyAny>) {
        let router = Arc::get_mut(&mut self.message_router)
            .expect("Failed to get mutable reference to message_router");
        router.add_processor(PyProcessor {
            callback: callback.unbind(),
            name: format!("PyProcessor-{}", topic),
            topic,
        });
    }

    pub fn start(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let config = self.config.clone();
        let message_router = Arc::clone(&self.message_router);
        let cancellation_token = self.cancellation_token.clone();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            // Now we are in a Tokio context
            let context = CustomConsumerContext::new();
            let consumer = KafkaManager::create_consumer(&config.kafka, context)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

            let kafka_manager = KafkaManager {
                consumer: Arc::new(consumer),
                config: config.kafka,
            };

            kafka_manager
                .start_consuming(message_router, cancellation_token)
                .await
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?;

            let res: PyResult<Py<PyAny>> = Python::attach(|py| Ok(py.None()));
            res
        })
        .map(|b| b.unbind())
    }

    pub fn stop(&self) {
        self.cancellation_token.cancel();
    }
}

pub struct KafkaManager {
    consumer: Arc<StreamConsumer<CustomConsumerContext>>,
    config: KafkaConfig,
}

impl KafkaManager {
    fn create_consumer(
        config: &KafkaConfig,
        context: CustomConsumerContext,
    ) -> Result<StreamConsumer<CustomConsumerContext>, KafkaError> {
        let mut client_config = ClientConfig::new();

        // Basic configuration
        client_config
            .set("bootstrap.servers", &config.brokers)
            .set("group.id", &config.group_id)
            .set("auto.offset.reset", &config.auto_offset_reset)
            .set("enable.auto.commit", config.enable_auto_commit.to_string())
            .set("session.timeout.ms", config.session_timeout_ms.to_string())
            .set(
                "heartbeat.interval.ms",
                config.heartbeat_interval_ms.to_string(),
            )
            .set(
                "max.poll.interval.ms",
                config.max_poll_interval_ms.to_string(),
            )
            .set("fetch.min.bytes", "1048576")
            .set("enable.partition.eof", "false")
            .set("max.partition.fetch.bytes", "10485760")
            .set("fetch.message.max.bytes", "1048576")
            .set("queued.min.messages", "100000")
            .set("queued.max.messages.kbytes", "65536")
            .set("fetch.wait.max.ms", "500")
            .set("fetch.error.backoff.ms", "500")
            .set("reconnect.backoff.ms", "100")
            .set("retry.backoff.ms", "1000")
            .set("reconnect.backoff.max.ms", "10000");

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

        client_config.create_with_context(context)
    }

    pub async fn start_consuming<P>(
        &self,
        processor: Arc<P>,
        cancellation_token: CancellationToken,
    ) -> Result<(), KafkaError>
    where
        P: MessageProcessor + Send + Sync + 'static,
    {
        info!("Starting Kafka message consumption");

        let consumer = Arc::clone(&self.consumer);

        // Subscribe to topics if not already done
        let topics: Vec<&str> = self.config.topics.iter().map(|s| s.as_str()).collect();
        consumer.subscribe(&topics)?;

        let (tx, mut rx) = mpsc::channel::<KafkaMessage>(1000);

        // Spawn message reader task
        let reader_consumer = Arc::clone(&consumer);
        let reader_token = cancellation_token.clone();
        let reader_tx = tx.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = reader_token.cancelled() => {
                        info!("Kafka message reader received shutdown signal");
                        break;
                    }
                    message_result = reader_consumer.recv() => {
                        match message_result {
                            Ok(message) => {
                                let kafka_msg = KafkaMessage::from_rdkafka_message(&message);
                                if let Err(e) = reader_tx.send(kafka_msg).await {
                                    error!("Failed to send message to processor channel: {}", e);
                                    break;
                                }
                            }
                            Err(e) => {
                                error!("Error receiving Kafka message: {}", e);
                                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                            }
                        }
                    }
                }
            }
        });

        // Process messages
        while let Some(message) = rx.recv().await {
            tokio::select! {
                _ = cancellation_token.cancelled() => {
                    info!("Kafka message processor received shutdown signal");
                    break;
                }
                result = processor.process_message(&message) => {
                    match result {
                        Ok(_) => {
                            // Commit offset manually if auto-commit is disabled
                            if !self.config.enable_auto_commit {
                                if let Err(e) = consumer.commit_consumer_state(rdkafka::consumer::CommitMode::Async) {
                                    error!("Failed to commit offset: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to process message: {}", e);
                            // Implement retry logic or dead letter queue here
                        }
                    }
                }
            }
        }
        self.shutdown_signal().await;
        info!("Kafka message consumption stopped");
        Ok(())
    }

    async fn shutdown_signal(&self) {
        let ctrl_c = async {
            signal::ctrl_c()
                .await
                .expect("failed to install Ctrl+C handler");
        };

        #[cfg(unix)]
        let terminate = async {
            signal::unix::signal(signal::unix::SignalKind::terminate())
                .expect("failed to install signal handler")
                .recv()
                .await;
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            _ = ctrl_c => {
                info!("Received Ctrl+C signal");
                // std::process::exit(0);
            },
            _ = terminate => {
                info!("Received terminate signal");
                // std::process::exit(0);
            },
        }
    }
}

// Custom consumer context for additional functionality
pub struct CustomConsumerContext {
    // Add custom fields here for metrics, logging, etc.
}

impl CustomConsumerContext {
    pub fn new() -> Self {
        Self {}
    }
}

impl ClientContext for CustomConsumerContext {}

impl ConsumerContext for CustomConsumerContext {
    fn pre_rebalance(
        &self,
        _consumer: &rdkafka::consumer::BaseConsumer<Self>,
        rebalance: &rdkafka::consumer::Rebalance,
    ) {
        info!("Pre-rebalance event: {:?}", rebalance);
    }

    fn post_rebalance(
        &self,
        _consumer: &rdkafka::consumer::BaseConsumer<Self>,
        rebalance: &rdkafka::consumer::Rebalance,
    ) {
        info!("Post-rebalance event: {:?}", rebalance);
    }

    fn commit_callback(
        &self,
        result: rdkafka::error::KafkaResult<()>,
        _offsets: &rdkafka::TopicPartitionList,
    ) {
        match result {
            Ok(_) => debug!("Offset commit successful"),
            Err(e) => error!("Offset commit failed: {}", e),
        }
    }
}
