//! Integration tests for MQTT subscribe handling
#[cfg(test)]
mod integration {
    use rusty_mqtt::subscribe_handlers::SubscribeHandler;

    #[test]
    fn test_topic_matching_exact() {
        let exact = SubscribeHandler::wildcard_match("/news", "/news");
        assert!(exact, "Exact topic match should succeed");

        let not_exact = SubscribeHandler::wildcard_match("/news", "/articles");
        assert!(!not_exact, "Different topics should not match");
    }

    #[test]
    fn test_hash_matches_anything_when_at_end() {
        use rusty_mqtt::subscribe_handlers::SubscribeHandler;

        let result = SubscribeHandler::wildcard_match("/#", "/anything/here/at/all");
        assert!(
            result,
            "Hash returns true consuming all topic after matching prefix"
        );

        let result = SubscribeHandler::wildcard_match("#", "anything");
        assert!(result, "# alone matches single word");
    }

    #[test]
    fn test_hash_with_prefix() {
        use rusty_mqtt::subscribe_handlers::SubscribeHandler;

        let result = SubscribeHandler::wildcard_match("/news/#", "/news/articles/b/c");
        assert!(
            result,
            "Hash at end matches all remaining levels after prefix"
        );
    }

    #[test]
    fn test_partial_topic_path() {
        use rusty_mqtt::subscribe_handlers::SubscribeHandler;

        let result = SubscribeHandler::wildcard_match("/a/+/c", "/b/c");
        assert!(!result, "Different first level fails");
    }

    #[test]
    fn test_suback_response_generation() {
        use rusty_mqtt::subscribe_handlers::SubscribeHandler;

        let response = SubscribeHandler::generate_suback(45u16, 2);
        assert_eq!(
            response.len(),
            6,
            "SUBACK: 1 fixed + 1 len + 2 pkt_id + 2 return codes"
        );
        assert_eq!(response[0], 0x90, "Byte 0 = SUBACK packet type");
        assert_eq!(response[1], 4u8, "Remaining length = 2 + num_topics");
        assert_eq!(response[3], 45u8, "Packet ID LSB = 45");
        assert_eq!(response[4], 0x00, "Return code 0 = success");
    }

    #[test]
    fn test_plus_wildcard_single_level() {
        use rusty_mqtt::subscribe_handlers::SubscribeHandler;

        let result = SubscribeHandler::wildcard_match("/+/news/", "/x/news/");
        assert!(result, "+ at char position 1 matches different word");
    }
}

// MQTT Server tests
#[cfg(test)]
mod server_tests {
    use rusty_mqtt::MqttServer;

    #[test]
    fn test_server_creation() {
        let server = MqttServer::new("127.0.0.1:1883");
        assert_eq!(server.get_address(), "127.0.0.1:1883");
    }
}
