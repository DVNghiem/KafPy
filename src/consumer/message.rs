use rdkafka::message::{BorrowedMessage, Headers};
use rdkafka::{Message, Timestamp};
use std::time::{SystemTime, UNIX_EPOCH};

/// Owned message envelope.
///
/// Produced by the consumer loop — no lifetime tied to the consumer buffer.
/// Contains all metadata needed for routing, processing, and offset tracking.
#[derive(Debug, Clone)]
pub struct OwnedMessage {
    pub topic: String,
    pub partition: i32,
    pub offset: i64,
    pub key: Option<Vec<u8>>,
    pub payload: Option<Vec<u8>>,
    pub timestamp: MessageTimestamp,
    pub headers: Vec<(String, Option<Vec<u8>>)>,
}

/// Timestamp representation for Kafka messages.
#[derive(Debug, Clone, Copy, Default)]
pub enum MessageTimestamp {
    /// Message creation time set by the producer.
    CreateTime(i64),
    /// Message log append time set by the broker.
    LogAppendTime(i64),
    /// Timestamp was not set.
    #[default]
    NotAvailable,
}

impl MessageTimestamp {
    pub fn from_kafka(ts: Timestamp) -> Self {
        match ts {
            Timestamp::CreateTime(ms) => Self::CreateTime(ms),
            Timestamp::LogAppendTime(ms) => Self::LogAppendTime(ms),
            Timestamp::NotAvailable => Self::NotAvailable,
        }
    }

    /// Returns milliseconds since epoch, if available.
    pub fn as_millis(&self) -> Option<i64> {
        match self {
            Self::CreateTime(ms) | Self::LogAppendTime(ms) => Some(*ms),
            Self::NotAvailable => None,
        }
    }

    /// Returns a `SystemTime` if the timestamp is available.
    pub fn as_system_time(&self) -> Option<SystemTime> {
        self.as_millis().map(|ms| UNIX_EPOCH + std::time::Duration::from_millis(ms as u64))
    }
}

impl OwnedMessage {
    /// Converts a `BorrowedMessage` from rdkafka into an owned envelope.
    ///
    /// This is the single conversion point at the consumer loop boundary.
    /// Once converted, the message is fully owned and the consumer buffer
    /// is free to advance.
    pub fn from_borrowed(msg: &BorrowedMessage) -> Self {
        let headers = msg
            .headers()
            .map(|h| {
                let mut result = Vec::with_capacity(h.count());
                for i in 0..h.count() {
                    if let Some(header) = h.try_get(i) {
                        result.push((
                            header.key.to_string(),
                            header.value.map(|v| v.to_vec()),
                        ));
                    }
                }
                result
            })
            .unwrap_or_default();

        Self {
            topic: msg.topic().to_string(),
            partition: msg.partition(),
            offset: msg.offset(),
            key: msg.key().map(|k| k.to_vec()),
            payload: msg.payload().map(|p| p.to_vec()),
            timestamp: MessageTimestamp::from_kafka(msg.timestamp()),
            headers,
        }
    }

    /// Returns the payload as a UTF-8 string, if valid.
    pub fn payload_str(&self) -> Option<&str> {
        self.payload
            .as_deref()
            .and_then(|b| std::str::from_utf8(b).ok())
    }

    /// Returns the key as a UTF-8 string, if valid.
    pub fn key_str(&self) -> Option<&str> {
        self.key.as_deref().and_then(|k| std::str::from_utf8(k).ok())
    }

    /// Returns the message size in bytes (payload + key), for metrics.
    pub fn size_bytes(&self) -> usize {
        self.payload.as_ref().map(|p| p.len()).unwrap_or(0)
            + self.key.as_ref().map(|k| k.len()).unwrap_or(0)
    }
}

/// A reference to a message key and payload for routing decisions.
#[derive(Debug, Clone)]
pub struct MessageRef<'a> {
    pub topic: &'a str,
    pub partition: i32,
    pub offset: i64,
    pub key: Option<&'a [u8]>,
}

impl OwnedMessage {
    /// Creates a non-owning reference suitable for routing lookups.
    /// Does not copy data.
    pub fn as_ref(&self) -> MessageRef<'_> {
        MessageRef {
            topic: &self.topic,
            partition: self.partition,
            offset: self.offset,
            key: self.key.as_deref(),
        }
    }
}
