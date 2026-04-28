//! Python-facing Kafka message type.
//!
//! Wraps the pure-Rust `OwnedMessage` from `consumer::message` so Python
//! consumers receive a `#[pyclass]` they can inspect and pass around.

use pyo3::prelude::*;

use crate::consumer::message::OwnedMessage;

/// Kafka message visible to Python consumers.
///
/// Constructed from a pure-Rust `OwnedMessage` at the PyO3 boundary.
/// All fields are copied into owned Python objects.
#[pyclass]
#[derive(Debug, Clone)]
pub struct KafkaMessage {
    #[pyo3(get)]
    pub topic: String,
    #[pyo3(get)]
    pub partition: i32,
    #[pyo3(get)]
    pub offset: i64,
    #[pyo3(get)]
    pub key: Option<Vec<u8>>,
    #[pyo3(get)]
    pub payload: Option<Vec<u8>>,
    #[pyo3(get)]
    pub timestamp_millis: Option<i64>,
    pub headers: Vec<(String, Option<Vec<u8>>)>,
}

#[pymethods]
impl KafkaMessage {
    #[new]
    pub fn new(
        topic: String,
        partition: i32,
        offset: i64,
        key: Option<Vec<u8>>,
        payload: Option<Vec<u8>>,
        timestamp_millis: Option<i64>,
        headers: Vec<(String, Option<Vec<u8>>)>,
    ) -> Self {
        Self {
            topic,
            partition,
            offset,
            key,
            payload,
            timestamp_millis,
            headers,
        }
    }

    /// Returns the message headers as a list of (key, value) tuples.
    ///
    /// Each tuple contains the header key as a string and the header value
    /// as bytes (or None if the header has no value).
    pub fn get_headers(&self) -> Vec<(String, Option<Vec<u8>>)> {
        self.headers.clone()
    }

    /// Returns the message timestamp in milliseconds since epoch, if available.
    pub fn get_timestamp_millis(&self) -> Option<i64> {
        self.timestamp_millis
    }

    pub fn __repr__(&self) -> String {
        format!(
            "KafkaMessage(topic={}, partition={}, offset={})",
            self.topic, self.partition, self.offset
        )
    }
}

impl From<OwnedMessage> for KafkaMessage {
    fn from(msg: OwnedMessage) -> Self {
        Self {
            topic: msg.topic,
            partition: msg.partition,
            offset: msg.offset,
            key: msg.key,
            payload: msg.payload,
            timestamp_millis: msg.timestamp.as_millis(),
            headers: msg.headers,
        }
    }
}
