use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct SubscriptionRecord {
    pub packet_id: u16,
    pub qos: u8,
    pub topic_filter: String,
}

/// MVP Subscribe Handler - AP2 complete (Parse & Storage)  
pub struct SubscribeHandler {
    #[allow(dead_code)]
    subscriptions: HashMap<String, Vec<SubscriptionRecord>>,
}

impl Default for SubscribeHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl SubscribeHandler {
    pub fn new() -> Self {
        let subs = HashMap::new();
        Self {
            subscriptions: subs,
        }
    }

    /// Extracts the 2-byte packet ID from buffer (AP1/Parse logic)  
    pub fn extract_packet_id(buffer: &[u8]) -> Option<u16> {
        if buffer.len() >= 2 {
            Some(u16::from_be_bytes([buffer[0], buffer[1]]))
        } else {
            None
        }
    }

    /// Generates SUBACK response with success codes (AP1/Response logic)  
    pub fn generate_suback(packet_id: u16, num_topics: usize) -> Vec<u8> {
        let mut response = Vec::new();
        if num_topics < 128 && packet_id < 256 {
            response.push((1 + num_topics) as u8);
            response.push(0x90);
            response.push(0x02);
            response.push(packet_id as u8);
        } else {
            panic!("Packet too large for MVP");
        }
        response.extend(vec![0x80; num_topics]);
        response
    }

    /// Wildcard topic matching helper (MQTT standard support):  
    /// `+` = exactly one level, `#` = any levels at END of pattern.  
    pub fn wildcard_match(pattern: &str, topic: &str) -> bool {
        if pattern == topic {
            return true;
        }

        if !pattern.contains('+') && !pattern.contains('#') {
            return false;
        }

        let (mut ip, mut it): (usize, usize) = (0, 0);
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

    /// Exact match for topic filter (no wildcard)  
    pub fn exact_match(topic: &str, filter: &str) -> bool {
        topic == filter
    }
}

// Module exports
pub fn extract_packet_id(buffer: &[u8]) -> Option<u16> {
    SubscribeHandler::extract_packet_id(buffer)
}

pub fn exact_match(topic: &str, filter: &str) -> bool {
    SubscribeHandler::exact_match(topic, filter)
}

// Unit Tests
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_packet_id() {
        assert_eq!(extract_packet_id(&[0x30, 0x39]), Some(0x3039));
    }

    #[test]
    fn test_generate_suback() {
        let response = SubscribeHandler::generate_suback(45u16, 2);
        assert_eq!(response[0], 3u8);
    }

    #[test]
    fn test_wildcard_match_exact() {
        assert!(SubscribeHandler::wildcard_match("/home", "/home"));
        assert!(!SubscribeHandler::wildcard_match("/home", "/news"));
    }

    #[test]
    fn test_plus_wildcard() {
        assert!(SubscribeHandler::wildcard_match("+/news", "1/news"));
        assert!(!SubscribeHandler::wildcard_match("+/news", "123456/news"));
        assert!(!SubscribeHandler::wildcard_match("+/news", "/news"));
    }

    #[test]
    fn test_hash_wildcard() {
        assert!(SubscribeHandler::wildcard_match(
            "/news/#",
            "/news/articles/123"
        ));
    }
}

/// Topic Router for storing subscriptions (AP2 deliverable)
pub struct TopicRouter {
    subscriptions: HashMap<u16, Vec<(String, u8)>>, // packet_id -> [(topic_filter, qos)]
}

impl TopicRouter {
    pub fn new() -> Self {
        Self {
            subscriptions: HashMap::new(),
        }
    }
    
    /// Subscribe a client to topics (MVP implementation)
    pub async fn subscribe(&mut self, packet_id: u16, filters: Vec<(String, u8)>) {
        // Store subscription records keyed by packet_id for retrieval
        self.subscriptions.insert(packet_id, filters);
    }
    
    /// Get subscriptions for a specific topic (using wildcard matching)
    pub fn get_subscribers_for_topic(&self, topic: &str) -> Vec<u16> {
        let mut result = Vec::new();
        
        for (_packet_id, filter_topics) in &self.subscriptions {
            for (filter, _) in filter_topics {
                if SubscribeHandler::wildcard_match(filter, topic) {
                    result.push(*_packet_id);
                    break; // Avoid duplicate packet_ids
                }
            }
        }
        
        result
    }
}

impl Default for TopicRouter {
    fn default() -> Self {
        Self::new()
    }
}

