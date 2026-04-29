//! DLQ produce — fire-and-forget producer for dead-letter messages.
//!
//! Uses a bounded tokio channel (~100) to decouple the worker loop from
//! the slow-path broker produce. When the channel is full, messages are
//! dropped (don't block the main consumer pipeline).

use rdkafka::producer::{FutureProducer, FutureRecord};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, error, warn};

use super::metadata::DlqMetadata;
use crate::consumer::config::ConsumerConfig;
use crate::observability::metrics::{DlqMetrics, SharedPrometheusSink};

/// Bounded channel capacity — if full, DLQ messages are dropped.
const DLQ_CHANNEL_CAPACITY: usize = 100;

struct DLQMessage {
    topic: String,
    original_topic: String,
    partition: i32,
    payload: Vec<u8>,
    key: Option<Vec<u8>>,
    headers: Vec<(String, Vec<u8>)>,
}

/// SharedDLQ producer with bounded fire-and-forget channel.
///
/// Decouples the worker loop from DLQ produce latency using a bounded
/// mpsc channel. The worker loop sends to the channel without awaiting;
/// a background task drains the channel and produces to Kafka.
pub struct SharedDlqProducer {
    send_tx: mpsc::Sender<DLQMessage>,
    /// Stored only for cloning to background task — field itself is never read.
    #[allow(dead_code)]
    prometheus_sink: SharedPrometheusSink,
}

impl SharedDlqProducer {
    /// Creates a new SharedDlqProducer from ConsumerConfig.
    ///
    /// Creates a dedicated FutureProducer for DLQ messages using the same
    /// broker configuration as the main consumer.
    pub fn new(
        config: &ConsumerConfig,
        prometheus_sink: SharedPrometheusSink,
    ) -> Result<Self, rdkafka::error::KafkaError> {
        let producer = Self::create_producer(config)?;
        let producer = Arc::new(producer);
        let (send_tx, mut send_rx) = mpsc::channel::<DLQMessage>(DLQ_CHANNEL_CAPACITY);
        let producer_clone = Arc::clone(&producer);
        let sink_clone = prometheus_sink.clone();

        // Background task drains the channel and produces to Kafka
        tokio::spawn(async move {
            while let Some(msg) = send_rx.recv().await {
                let producer = Arc::clone(&producer_clone);
                Self::do_produce(producer, msg, sink_clone.clone()).await;
            }
        });

        Ok(Self {
            send_tx,
            prometheus_sink,
        })
    }

    fn create_producer(
        config: &ConsumerConfig,
    ) -> Result<FutureProducer, rdkafka::error::KafkaError> {
        let mut cfg = rdkafka::config::ClientConfig::new();
        cfg.set("bootstrap.servers", &config.brokers)
            .set("message.timeout.ms", "5000")
            .set("queue.buffering.max.messages", "100000")
            .set("queue.buffering.max.kbytes", "65536")
            .set("request.timeout.ms", "5000")
            .set("retry.backoff.ms", "100")
            .set("retries", "2");

        if let Some(ref proto) = config.security_protocol {
            cfg.set("security.protocol", proto);
        }
        if let Some(ref mechanism) = config.sasl_mechanism {
            cfg.set("sasl.mechanism", mechanism);
        }
        if let Some(ref username) = config.sasl_username {
            cfg.set("sasl.username", username);
        }
        if let Some(ref password) = config.sasl_password {
            cfg.set("sasl.password", password);
        }

        cfg.create()
    }

    async fn do_produce(
        producer: Arc<FutureProducer>,
        msg: DLQMessage,
        prometheus_sink: SharedPrometheusSink,
    ) {
        let mut record = FutureRecord::to(&msg.topic)
            .payload(&msg.payload)
            .partition(msg.partition)
            .headers(Self::build_headers(&msg.headers));

        if let Some(ref key) = msg.key {
            record = record.key(key.as_slice());
        }

        let original_topic = msg.original_topic.clone();
        match producer
            .send(
                record,
                rdkafka::util::Timeout::After(Duration::from_secs(5)),
            )
            .await
        {
            Ok(delivery) => {
                debug!(
                    "DLQ message delivered to '{}' partition {} offset {}",
                    msg.topic, delivery.partition, delivery.offset
                );
                // Record DLQ metric after successful delivery
                DlqMetrics::record_dlq_message(&prometheus_sink, &msg.topic, &original_topic);
            }
            Err((err, _)) => {
                error!(
                    error = %err,
                    topic = %msg.topic,
                    partition = msg.partition,
                    "Failed to deliver DLQ message"
                );
            }
        }
    }

    fn build_headers(headers: &[(String, Vec<u8>)]) -> rdkafka::message::OwnedHeaders {
        let mut kafka_headers = rdkafka::message::OwnedHeaders::new();
        for (key, value) in headers {
            kafka_headers = kafka_headers.insert(rdkafka::message::Header {
                key: key.as_str(),
                value: Some(value),
            });
        }
        kafka_headers
    }

    /// Produces a message to DLQ asynchronously — fire-and-forget.
    ///
    /// Does NOT block the caller. If the bounded channel is full, the
    /// message is dropped and a warning is logged.
    pub fn produce_async(
        &self,
        topic: String,
        partition: i32,
        payload: Vec<u8>,
        key: Option<Vec<u8>>,
        metadata: &DlqMetadata,
    ) {
        let headers = Self::metadata_to_headers(metadata);

        let msg = DLQMessage {
            topic,
            original_topic: metadata.original_topic.clone(),
            partition,
            payload,
            key,
            headers,
        };

        // Try to send without blocking — if channel is full, drop the message
        if self.send_tx.try_send(msg).is_err() {
            warn!(
                topic = %metadata.original_topic,
                partition = metadata.original_partition,
                offset = metadata.original_offset,
                "DLQ channel full — dropping message"
            );
        }
    }

    /// Converts DlqMetadata into Kafka headers.
    pub fn metadata_to_headers(metadata: &DlqMetadata) -> Vec<(String, Vec<u8>)> {
        vec![
            (
                "dlq.original_topic".to_string(),
                metadata.original_topic.as_bytes().to_vec(),
            ),
            (
                "dlq.partition".to_string(),
                metadata.original_partition.to_string().as_bytes().to_vec(),
            ),
            (
                "dlq.offset".to_string(),
                metadata.original_offset.to_string().as_bytes().to_vec(),
            ),
            (
                "dlq.reason".to_string(),
                metadata.failure_reason.as_bytes().to_vec(),
            ),
            (
                "dlq.attempts".to_string(),
                metadata.attempt_count.to_string().as_bytes().to_vec(),
            ),
            (
                "dlq.first_failure".to_string(),
                metadata
                    .first_failure_timestamp
                    .to_rfc3339()
                    .as_bytes()
                    .to_vec(),
            ),
            (
                "dlq.last_failure".to_string(),
                metadata
                    .last_failure_timestamp
                    .to_rfc3339()
                    .as_bytes()
                    .to_vec(),
            ),
        ]
    }
}
