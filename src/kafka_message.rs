use pyo3::prelude::*;
use rdkafka::{message::Headers, Message};

// Kafka message wrapper
#[pyclass]
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct KafkaMessage {
    #[pyo3(get)]
    pub topic: String,
    #[pyo3(get)]
    pub partition: i32,
    #[pyo3(get)]
    pub offset: i64,
    pub key: Option<Vec<u8>>,
    #[pyo3(get)]
    pub payload: Option<String>,
    pub headers: Vec<(String, Option<Vec<u8>>)>,
}

#[pymethods]
impl KafkaMessage {
    #[getter]
    pub fn get_key(&self) -> Option<Vec<u8>> {
        self.key.clone()
    }

    #[getter]
    pub fn get_headers(&self) -> Vec<(String, Option<Vec<u8>>)> {
        self.headers.clone()
    }

    pub fn __repr__(&self) -> String {
        format!(
            "KafkaMessage(topic='{}', partition={}, offset={}, payload={:?})",
            self.topic, self.partition, self.offset, self.payload
        )
    }
}

impl KafkaMessage {
    pub fn from_rdkafka_message(message: &rdkafka::message::BorrowedMessage) -> Self {
        let headers = message
            .headers()
            .map(|h| {
                let mut result = Vec::new();
                for i in 0..h.count() {
                    if let Some(header) = h.try_get(i) {
                        result.push((header.key.to_string(), header.value.map(|v| v.to_vec())));
                    }
                }
                result
            })
            .unwrap_or_default();

        let payload = message
            .payload_view::<str>()
            .and_then(|res| res.ok())
            .map(|s| s.to_string());

        Self {
            topic: message.topic().to_string(),
            partition: message.partition(),
            offset: message.offset(),
            key: message.key().map(|k| k.to_vec()),
            payload,
            headers,
        }
    }
}
