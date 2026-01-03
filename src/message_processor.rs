use async_trait::async_trait;
use pyo3::prelude::*;
use tracing::{error, info, warn};

use super::KafkaMessage;
use crate::errors::KafkaConsumerError;

#[async_trait]
pub trait MessageProcessor: Send + Sync {
    async fn process_message(&self, message: &KafkaMessage) -> Result<(), KafkaConsumerError>;
    fn supported_topic(&self) -> String;
    fn processor_name(&self) -> String;
}

pub struct PyProcessor {
    pub callback: Py<PyAny>,
    pub name: String,
    pub topic: String,
}

#[async_trait]
impl MessageProcessor for PyProcessor {
    async fn process_message(&self, message: &KafkaMessage) -> Result<(), KafkaConsumerError> {
        let message = message.clone();

        Python::attach(|py| {
            let callback = self.callback.bind(py);
            let result = callback.call1((message,));
            match result {
                Ok(coro) => {
                    // Check if it's a coroutine
                    if let Ok(future) = pyo3_async_runtimes::tokio::into_future(coro) {
                        // It's a coroutine, we need to await it
                        tokio::spawn(async move {
                            if let Err(e) = future.await {
                                error!("Error in async Python handler: {:?}", e);
                            }
                        });
                        Ok(())
                    } else {
                        // It's a sync result
                        Ok(())
                    }
                }
                Err(e) => {
                    error!("Error calling Python handler: {:?}", e);
                    Err(KafkaConsumerError::Processing(e.to_string()))
                }
            }
        })
    }

    fn supported_topic(&self) -> String {
        self.topic.clone()
    }

    fn processor_name(&self) -> String {
        self.name.clone()
    }
}

pub struct MessageRouter {
    processors: Vec<Box<dyn MessageProcessor>>,
}

impl MessageRouter {
    pub fn new() -> Self {
        Self {
            processors: Vec::new(),
        }
    }

    pub fn add_processor<P: MessageProcessor + 'static>(&mut self, processor: P) {
        info!(
            "Registering message processor: {} for topic: {:?}",
            processor.processor_name(),
            processor.supported_topic()
        );
        self.processors.push(Box::new(processor));
    }

    pub async fn route_message(&self, message: &KafkaMessage) -> Result<(), KafkaConsumerError> {
        let mut processed = false;

        for processor in &self.processors {
            if processor.supported_topic() == message.topic || processor.supported_topic() == "*" {
                match processor.process_message(message).await {
                    Ok(_) => {
                        processed = true;
                    }
                    Err(e) => {
                        error!(
                            "Failed to process message with {}: {}",
                            processor.processor_name(),
                            e
                        );
                        return Err(e);
                    }
                }
            }
        }
        if !processed {
            warn!("No processor found for topic: {}", message.topic);
        }

        Ok(())
    }
}

#[async_trait]
impl MessageProcessor for MessageRouter {
    async fn process_message(&self, message: &KafkaMessage) -> Result<(), KafkaConsumerError> {
        self.route_message(message).await
    }

    fn supported_topic(&self) -> String {
        "*".to_string()
    }

    fn processor_name(&self) -> String {
        "MessageRouter".to_string()
    }
}
