//! Subscribe-Handler: SUBACK-Generierung und Wildcard-Matching-Hilfsfunktionen.
pub mod storage;
pub use storage::{Subscriber, TopicRouter, topic_matches};

/// Interne Aufzeichnung einer Subscription (Paket-ID, QoS, Filter).
#[derive(Debug, Clone)]
pub struct SubscriptionRecord {
    /// Packet Identifier des urspruenglichen SUBSCRIBE-Pakets.
    pub packet_id: u16,
    /// Gewuenschter QoS-Level.
    pub qos: u8,
    /// Topic Filter String.
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
    /// Erstellt einen neuen `SubscribeHandler`.
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
    /// `granted_qos` enthaelt die tatsaechlich gewährten QoS-Werte (je 1 Byte pro Subscription).
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

    /// Topic wildcard matching -- delegiert an die kanonische Implementierung in storage.
    pub fn wildcard_match(filter: &str, topic: &str) -> bool {
        topic_matches(filter, topic)
    }
}

/// Prueft ob `topic` exakt mit `filter` uebereinstimmt (kein Wildcard-Matching).
pub fn exact_match(topic: &str, filter: &str) -> bool {
    topic_matches(topic, filter)
}
