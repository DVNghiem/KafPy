//! RoutingContext — zero-copy borrowed view of a message for routing evaluation.

use crate::consumer::message::OwnedMessage;

/// Opaque handler identifier.
///
/// Distinct from [`RoutingContext::topic`] — HandlerId is the internal key used
/// to route messages to registered handlers; topic is the Kafka topic name.
/// While handlers are currently registered by topic name, these are conceptually
/// separate: a handler might be registered with an alias, not a topic.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HandlerId(String);

impl HandlerId {
    /// Creates a new HandlerId from a String (or &str via Into).
    #[inline]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Returns the underlying string slice.
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns true if the underlying string is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Display for HandlerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for HandlerId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for HandlerId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for HandlerId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<HandlerId> for String {
    fn from(id: HandlerId) -> Self {
        id.0
    }
}

impl std::borrow::Borrow<str> for HandlerId {
    fn borrow(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RoutingContext<'a> {
    pub topic: &'a str,
    pub partition: i32,
    pub offset: i64,
    pub key: Option<&'a [u8]>,
    pub payload: Option<&'a [u8]>,
    pub headers: &'a [(String, Option<Vec<u8>>)],
}

impl<'a> RoutingContext<'a> {
    pub fn from_message(msg: &'a OwnedMessage) -> Self {
        Self {
            topic: &msg.topic,
            partition: msg.partition,
            offset: msg.offset,
            key: msg.key.as_deref(),
            payload: msg.payload.as_deref(),
            headers: &msg.headers,
        }
    }

    pub fn payload_str(&self) -> Option<&'a str> {
        self.payload.and_then(|b| std::str::from_utf8(b).ok())
    }

    pub fn key_str(&self) -> Option<&'a str> {
        self.key.and_then(|k| std::str::from_utf8(k).ok())
    }

    pub fn headers_iter(&self) -> impl Iterator<Item = (&str, Option<&[u8]>)> + 'a {
        self.headers.iter().map(|(k, v)| (k.as_str(), v.as_deref()))
    }

    pub fn has_header(&self, key: &str) -> bool {
        self.headers.iter().any(|(k, _)| k == key)
    }

    pub fn get_header(&self, key: &str) -> Option<&[u8]> {
        self.headers
            .iter()
            .find(|(k, _)| k == key)
            .and_then(|(_, v)| v.as_deref())
    }
}
