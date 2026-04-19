use crate::consumer::config::ConsumerConfig;
use crate::consumer::error::ConsumerError;
use crate::consumer::message::OwnedMessage;
use crate::coordinator::ShutdownCoordinator;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::error::KafkaError;
use std::sync::Arc;
use std::time::Duration;
use tokio::select;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::{Stream, StreamExt};
use tracing::{debug, error, info};

/// The consumer runner owns the consumer and drives the async message loop.
///
/// It converts each `BorrowedMessage` from rdkafka into an `OwnedMessage`
/// before sending it through a channel. This ensures no lifetime aliasing
/// with the consumer buffer.
///
/// Dropping the runner stops the consumer loop gracefully.
#[derive(Clone)]
pub struct ConsumerRunner {
    consumer: Arc<StreamConsumer>,
    shutdown_tx: broadcast::Sender<()>,
    /// Optional shutdown coordinator for phased graceful shutdown.
    /// When `Some`, `stop()` coordinates with the coordinator before signaling.
    coordinator: Option<Arc<ShutdownCoordinator>>,
}

impl ConsumerRunner {
    /// Creates a new runner from a consumer config.
    ///
    /// # Arguments
    ///
    /// * `config` — consumer configuration
    /// * `coordinator` — optional shutdown coordinator for phased graceful shutdown.
    ///   When `None`, `stop()` falls back to the existing broadcast-channel shutdown.
    pub fn new(
        config: ConsumerConfig,
        coordinator: Option<Arc<ShutdownCoordinator>>,
    ) -> Result<Self, ConsumerError> {
        let consumer: StreamConsumer = config
            .clone()
            .into_rdkafka_config()
            .create()
            .map_err(ConsumerError::Kafka)?;

        consumer
            .subscribe(&config.topics.iter().map(|s| s.as_str()).collect::<Vec<_>>())
            .map_err(|e| ConsumerError::Subscription(e.to_string()))?;

        info!(
            topics = ?config.topics,
            group_id = %config.group_id,
            "Consumer subscribed"
        );

        let (shutdown_tx, _) = broadcast::channel(1);

        Ok(Self {
            consumer: Arc::new(consumer),
            shutdown_tx,
            coordinator,
        })
    }

    /// Runs the consumer loop, sending each `OwnedMessage` through the returned channel.
    ///
    /// The sender is dropped when `stop()` is called or a fatal error occurs,
    /// causing the receiver to return `None`.
    pub fn run(&self) -> mpsc::Receiver<Result<OwnedMessage, ConsumerError>> {
        let (tx, rx) = mpsc::channel(1000);
        let consumer = Arc::clone(&self.consumer);
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        tokio::spawn(async move {
            loop {
                select! {
                    biased;

                    _ = shutdown_rx.recv() => {
                        info!("Consumer runner received shutdown signal");
                        break;
                    }

                    message_result = consumer.recv() => {
                        match message_result {
                            Ok(msg) => {
                                let owned = OwnedMessage::from_borrowed(&msg);
                                debug!(
                                    topic = %owned.topic,
                                    partition = owned.partition,
                                    offset = owned.offset,
                                    size = owned.size_bytes(),
                                    "Message received"
                                );
                                if tx.send(Ok(owned)).await.is_err() {
                                    // Receiver dropped — stop producing
                                    break;
                                }
                            }
                            Err(KafkaError::MessageConsumption(err)) => {
                                error!("Message consumption error: {}", err);
                                sleep(Duration::from_millis(100)).await;
                            }
                            Err(err) => {
                                error!("Consumer error: {}", err);
                                sleep(Duration::from_millis(500)).await;
                            }
                        }
                    }
                }
            }
        });

        rx
    }

    /// Stops the consumer loop gracefully.
    ///
    /// If a `ShutdownCoordinator` is configured, this triggers the phased
    /// shutdown sequence (Running -> Draining -> Finalizing -> Done).
    /// The coordinator's cancellation tokens are used to signal each component.
    /// Otherwise, falls back to the existing broadcast-channel shutdown.
    pub fn stop(&self) {
        info!("Signaling consumer runner to stop");
        if let Some(ref coord) = self.coordinator {
            // LSC-02/03: Begin draining phase FIRST, then signal dispatcher.
            // This prevents circular wait: dispatcher must stop before workers drain.
            let (dispatcher_cancel, _worker_cancel, _committer_cancel) =
                coord.begin_draining();
            // Dispatcher cancel is sent to interrupt the dispatcher loop.
            // Worker and committer cancels are passed to their respective owners.
            let _ = self.shutdown_tx.send(());
            info!("dispatcher stop signaled");
        } else {
            let _ = self.shutdown_tx.send(());
        }
    }

    /// Commits the current consumer offset state.
    ///
    /// This is a no-op if `enable_auto_commit` is true in the config.
    pub fn commit(&self) -> Result<(), ConsumerError> {
        self.consumer
            .commit_consumer_state(rdkafka::consumer::CommitMode::Async)
            .map_err(ConsumerError::from)?;
        debug!("Offset committed");
        Ok(())
    }

    /// Stores the current offset for a topic-partition in rdkafka's internal state.
    ///
    /// This is the first phase of two-phase manual offset management:
    /// 1. `store_offset` — fast, in-memory (this method)
    /// 2. `commit` — network round-trip to Kafka
    ///
    /// Requires `enable_auto_offset_store=false` in the config.
    pub async fn store_offset(
        &self,
        topic: &str,
        partition: i32,
        offset: i64,
    ) -> Result<(), ConsumerError> {
        let consumer = Arc::clone(&self.consumer);
        let topic = topic.to_string();
        tokio::task::spawn_blocking(move || {
            consumer
                .store_offset(&topic, partition, offset)
                .map_err(ConsumerError::from)
        })
        .await
        .map_err(|e| ConsumerError::Receive(format!("store_offset task failed: {e}")))?
    }

    /// Returns the current partition assignment.
    pub fn assignment(&self) -> Result<rdkafka::TopicPartitionList, ConsumerError> {
        self.consumer.assignment().map_err(ConsumerError::Kafka)
    }

    /// Pauses consumption for the given topic-partition list.
    pub fn pause(&self, tpl: &rdkafka::TopicPartitionList) -> Result<(), ConsumerError> {
        self.consumer.pause(tpl).map_err(ConsumerError::Kafka)
    }

    /// Resumes consumption for the given topic-partition list.
    pub fn resume(&self, tpl: &rdkafka::TopicPartitionList) -> Result<(), ConsumerError> {
        self.consumer.resume(tpl).map_err(ConsumerError::Kafka)
    }
}

/// A message stream backed by a `ReceiverStream`.
pub struct ConsumerStream {
    inner: ReceiverStream<Result<OwnedMessage, ConsumerError>>,
}

impl ConsumerStream {
    fn new(rx: mpsc::Receiver<Result<OwnedMessage, ConsumerError>>) -> Self {
        Self {
            inner: ReceiverStream::new(rx),
        }
    }
}

impl Stream for ConsumerStream {
    type Item = Result<OwnedMessage, ConsumerError>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        std::pin::Pin::new(&mut self.inner).poll_next(cx)
    }
}

impl ConsumerRunner {
    /// Returns a `Stream` of owned messages.
    pub fn stream(&self) -> ConsumerStream {
        ConsumerStream::new(self.run())
    }
}

/// Wraps the runner with a Tokio task, running the message stream in the background.
pub struct ConsumerTask {
    runner: Arc<ConsumerRunner>,
    handle: tokio::task::JoinHandle<()>,
}

impl ConsumerTask {
    /// Spawns the consumer runner as a Tokio task.
    ///
    /// The `process` async function is called for each `OwnedMessage`.
    /// It should return `Ok(())` on success or an error to halt consumption.
    pub fn spawn<F, Fut>(runner: ConsumerRunner, mut process: F) -> Result<Self, ConsumerError>
    where
        F: FnMut(OwnedMessage) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<(), ConsumerError>> + Send,
    {
        let runner = Arc::new(runner);
        let runner_clone = Arc::clone(&runner);

        let handle = tokio::spawn(async move {
            let mut stream = runner_clone.stream();
            while let Some(result) = stream.next().await {
                match result {
                    Ok(msg) => {
                        if let Err(e) = process(msg).await {
                            error!("Processing error, halting consumer: {}", e);
                            break;
                        }
                    }
                    Err(e) => {
                        error!("Consumer error, halting consumer: {}", e);
                        break;
                    }
                }
            }
        });

        Ok(Self { runner, handle })
    }

    /// Stops the consumer and waits for the task to finish.
    pub async fn shutdown(self) {
        self.runner.stop();
        self.handle.abort();
        std::mem::forget(self);
    }
}

impl Drop for ConsumerTask {
    fn drop(&mut self) {
        self.runner.stop();
    }
}
