use std::io;

#[derive(Debug, Clone, PartialEq)]
pub enum QoS {
    AtMostOnce = 0,
    AtLeastOnce = 1,
}

impl Default for QoS {
    fn default() -> Self {
        QoS::AtMostOnce
    }
}

#[derive(Debug)]
pub struct TopicFilter {
    pub topic: String,
    pub qos: QoS,
}

/// MVP Subscribe Handler - Parse only (no storage in AP1)  
pub struct SubscribeHandler {
    _private: (),
}

impl SubscribeHandler {
    /// Returns a new empty SubscribeHandler  
    pub fn new() -> Self {
        Self { _private: () }
    }

    /// Extracts the 2-byte packet ID from a SUBSCRIBE buffer.  
    pub fn extract_packet_id(buffer: &[u8]) -> Option<u16> {
        if buffer.len() >= 2 {
            Some(u16::from_be_bytes([buffer[0], buffer[1]]))
        } else {
            None
        }
    }

    /// Parses SUBSCRIBE topic count (simplified MVP - returns count)  
    pub fn parse_subscriber_topics(packet_name: u8, buffer: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        if packet_name != 0x82 {
            return Err(format!("Expected SUBSCRIBE packet name 0x82, got 0x{:02X}", packet_name).into());
        }

        let mut idx = 2; 
        let mut topic_count = 0;
        
        while idx < buffer.len() && buffer[idx] != 0 {
            let filter_len = u16::from(buffer[idx]) as usize;
            if idx + filter_len + 1 > buffer.len() {
                return Err("Buffer too short".into());
            }
            
            // Skip topic string and QoS byte for simple version  
            idx += filter_len + 3;
            topic_count += 1;
        }

        Ok(topic_count)
    }

    /// Generates SUBACK response packet (simplified MVP - just success codes).  
    pub fn generate_suback(packet_id: u16, num_topics: usize) -> Vec<u8> {
        let mut response = Vec::new();
        
        // FIXED HEADER: Type (0x90) + Length (variable length field)  
        if num_topics < 128 {
            response.push((1 + num_topics) as u8); 
            response.push(0x90); 
            response.push(0x02); 
        } else {
            panic!("Packet too large for MVP - too many topics");
        }
        
        // VARIABLE HEADER: Packet Identifier (lower byte only for MVP)  
        if packet_id < 256 { 
            response.push(packet_id as u8);
        } else {
            response.push(0xFF); 
            response.push((packet_id >> 8) as u8);
        }
        
        // PAYLOAD: Return codes (Success = [0x80] per topic)  
        for _ in 0..num_topics {
            response.push(0x80);
        }
        
        response
    }

    /// Simple wildcard matching helper.  
    pub fn wildcard_match(pattern: &str, topic: &str) -> bool {
        if pattern == topic {
            return true;
        }
        if !pattern.contains('+') && !pattern.contains('#') {
            return false;
        }
        // For MVP, use simple exact case-insensitive comparison  
        pattern.to_lowercase() == topic.to_lowercase()
    }

    /// Simple topic matching (exact match for now, will be enhanced in AP2).  
    pub fn exact_match(topic: &str, filter: &str) -> bool {
        topic == filter
    }
}

/// Creates a default empty subscriber handler for unit tests  
pub fn get_subscribe_handler() -> SubscribeHandler {
    SubscribeHandler::new()
}

// Unit Tests  
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qos_default() {
        assert_eq!(QoS::default(), QoS::AtMostOnce);
    }

    #[test]
    fn test_extract_packet_id_valid() {
        let buffer = [0x30, 0x39]; 
        let result = SubscribeHandler::extract_packet_id(&buffer[..]);
        assert_eq!(result, Some(0x3039));
    }

    #[test]
    fn test_generate_suback_basic() {
        let response = SubscribeHandler::generate_suback(45u16, 2);
        
        assert!(response.len() > 2);
        assert_eq!(response[0], (1 + 2) as u8); // Remaining length 
        assert_eq!(response[1], 0x90);         // Fixed header type SUBACK
    }

    #[test]
    fn test_generate_suback_single() {
        let response = SubscribeHandler::generate_suback(1u16, 1);
        assert_eq!(response.len(), 5); 
    }

    #[test]
    fn test_wildcard_match_exact() {
        assert!(SubscribeHandler::wildcard_match("/home", "/home"));
        assert!(!SubscribeHandler::wildcard_match("/home", "/news"));
    }

    #[test]
    fn test_generate_suback_empty_topics() {
        let response = SubscribeHandler::generate_suback(0u16, 0);
        // Empty topics should still return valid packet  
        assert_eq!(response.len(), 4); 
    }
}
