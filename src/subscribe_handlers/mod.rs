//! Subscribe handler: SUBACK generation and wildcard-matching helpers.
pub mod storage;
pub use storage::{Subscriber, TopicRouter, topic_matches};

/// Internal record of a subscription (Packet ID, QoS, filter).
#[derive(Debug, Clone)]
pub struct SubscriptionRecord {
    /// Packet Identifier of the original SUBSCRIBE packet.
    pub packet_id: u16,
    /// Requested QoS level.
    pub qos: u8,
    /// Topic Filter string.
    pub topic_filter: String,
}

/// Subscribe Handler for parsing SUBSCRIBE packets and generating SUBACK responses
pub struct SubscribeHandler {
    _subscriptions: Option<Vec<SubscriptionRecord>>,
}

impl Default for SubscribeHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl SubscribeHandler {
    /// Creates a new `SubscribeHandler`.
    pub fn new() -> Self {
        let subs = Vec::new();
        Self {
            _subscriptions: Some(subs),
        }
    }

    /// Extracts the 2-byte packet ID from buffer
    pub fn extract_packet_id(buffer: &[u8]) -> Option<u16> {
        if buffer.len() >= 2 {
            Some(u16::from_be_bytes([buffer[0], buffer[1]]))
        } else {
            None
        }
    }

    /// Generates SUBACK response per MQTT v3.1.1 spec.
    /// `granted_qos` contains the actually granted QoS values (1 byte per subscription).
    pub fn generate_suback(packet_id: u16, granted_qos: &[u8]) -> Vec<u8> {
        let mut response = vec![
            0x90,
            (2 + granted_qos.len()) as u8,
            (packet_id >> 8) as u8,
            (packet_id & 0xFF) as u8,
        ];
        response.extend_from_slice(granted_qos);
        response
    }

    /// Topic wildcard matching — delegates to the canonical implementation in storage.
    pub fn wildcard_match(filter: &str, topic: &str) -> bool {
        topic_matches(filter, topic)
    }
}

/// Checks whether `topic` exactly matches `filter` (no wildcard matching).
pub fn exact_match(topic: &str, filter: &str) -> bool {
    topic_matches(topic, filter)
}
