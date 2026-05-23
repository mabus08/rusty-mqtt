pub mod storage;
pub use storage::TopicRouter;

#[derive(Debug, Clone)]
pub struct SubscriptionRecord {
    pub packet_id: u16,
    pub qos: u8,
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

    /// Generates SUBACK response per MQTT v3.1.1 spec
    pub fn generate_suback(packet_id: u16, num_topics: usize) -> Vec<u8> {
        let mut response = vec![
            0x90,
            (2 + num_topics) as u8,
            (packet_id >> 8) as u8,
            (packet_id & 0xFF) as u8,
        ];
        response.extend(vec![0x00; num_topics]);
        response
    }

    /// Topic wildcard matching with + and # support  
    pub fn wildcard_match(pattern: &str, topic: &str) -> bool {
        if pattern == topic {
            return true;
        }

        let mut ip = 0;
        let mut it = 0;
        let p_bytes = pattern.as_bytes();
        let t_bytes = topic.as_bytes();

        loop {
            if ip >= p_bytes.len() && it >= t_bytes.len() {
                return true;
            }
            if ip >= p_bytes.len() || it >= t_bytes.len() {
                return false;
            }

            match *p_bytes.get(ip).unwrap_or(&0) {
                b'+' => {
                    ip += 1;
                    it += 1;
                }
                b'#' => return true,
                _ => {
                    if p_bytes[ip] != t_bytes[it] {
                        return false;
                    }
                    ip += 1;
                    it += 1;
                }
            }
        }
    }
}

pub fn exact_match(topic: &str, filter: &str) -> bool {
    SubscribeHandler::wildcard_match(topic, filter)
}
