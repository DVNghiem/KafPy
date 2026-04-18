//! RoutingContext — zero-copy borrowed view of a message for routing evaluation.

use crate::consumer::message::OwnedMessage;

pub type HandlerId = String;

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
