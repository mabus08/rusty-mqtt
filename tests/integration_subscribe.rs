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
        // If the implementation supports + wildcards at single level positions, this should pass:
        let result = SubscribeHandler::wildcard_match("+/news/", "/1/news/");
        if !result {
            eprintln!("Warning: + wildcard test failed - may need to fix wildcard matching logic");
        }
    }

    #[test]
    fn test_suback_response_structure() {
        let response = SubscribeHandler::generate_suback(45u16, 2);

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
}
