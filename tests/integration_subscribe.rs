//! Integration tests for MQTT subscribe handling - Verify the implemented behavior
#[cfg(test)]
mod subscribe_integration {
    use rusty_mqtt::subscribe_handlers::SubscribeHandler;

    #[test]
    fn test_hash_wildcard() {
        let result = SubscribeHandler::wildcard_match("/news/#", "/news/articles/123");
        assert!(result, "Hash wildcard should match nested topic levels");
    }

    #[test]
    fn test_plus_wildcard_basic() {
        // + matches exactly one topic level
        let result = SubscribeHandler::wildcard_match("+/news", "sport/news");
        assert!(result, "+ should match one full topic level");
    }

    #[test]
    fn test_suback_response_structure() {
        let response = SubscribeHandler::generate_suback(45u16, &[0x00, 0x00]);

        assert!(!response.is_empty(), "SUBACK response should not be empty");
        assert_eq!(response[0], 0x90, "Byte 0 should be SUBACK packet type");
        assert_eq!(
            response[1], 4u8,
            "Remaining length should be 2 + num_topics (4 for 2 topics)"
        );
        assert_eq!(response[2], 0x00, "Packet ID MSB should be 0x00");
        assert_eq!(response[3], 45u8, "Packet ID LSB should match");
        assert_eq!(response[4], 0x00, "Return code 1 should be success (QoS 0)");
        assert_eq!(response[5], 0x00, "Return code 2 should be success (QoS 0)");
    }

    #[test]
    fn test_exact_match_differents() {
        let not_same = SubscribeHandler::wildcard_match("/home", "/about");
        assert!(!not_same, "Different topics should not match");
    }

    #[test]
    fn test_suback_reflects_granted_qos() {
        // QoS 1 subscription should be reflected in SUBACK
        let response = SubscribeHandler::generate_suback(100u16, &[0x01]);
        assert_eq!(response[4], 0x01, "Return code should reflect QoS 1");
    }

    #[test]
    fn test_connect_client_id_parsing() {
        use rusty_mqtt::MqttServer;

        // Build a minimal CONNECT packet with client ID "test-client"
        let client_id = b"test-client";
        let mut packet: Vec<u8> = Vec::new();
        // Fixed Header
        packet.push(0x10); // CONNECT
        packet.push(0x00); // Remaining length placeholder (will fix)
        // Variable Header
        packet.extend_from_slice(&[0x00, 0x04]); // Protocol Name Length
        packet.extend_from_slice(b"MQTT"); // Protocol Name
        packet.push(0x04); // Protocol Level (3.1.1)
        packet.push(0x02); // Connect Flags (Clean Session)
        packet.extend_from_slice(&[0x00, 0x3C]); // Keep Alive (60s)
        // Payload: Client ID
        packet.push(0x00); // Client ID Length MSB
        packet.push(client_id.len() as u8); // Client ID Length LSB
        packet.extend_from_slice(client_id);

        // Fix remaining length
        packet[1] = (packet.len() - 2) as u8;

        let parsed = MqttServer::parse_connect_client_id(&packet);
        assert_eq!(parsed, "test-client");
    }
}
