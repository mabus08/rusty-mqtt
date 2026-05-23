#[cfg(test)]
mod config_tests {
    use rusty_mqtt::{BrokerConfig, ConfigError};
    use std::env;

    #[test]
    fn test_defaults_when_no_config_file_exists() {
        // Arbeitsverzeichnis auf ein temporaeres Verzeichnis setzen,
        // in dem keine rusty-mqtt.toml liegt
        let tmp = env::temp_dir().join("rusty_mqtt_test_no_config");
        std::fs::create_dir_all(&tmp).unwrap();

        let config = BrokerConfig::load_from(&tmp).unwrap();

        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 1884);
    }

    #[test]
    fn test_toml_file_overrides_defaults() {
        let tmp = env::temp_dir().join("rusty_mqtt_test_override");
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(
            tmp.join("rusty-mqtt.toml"),
            "host = \"0.0.0.0\"\nport = 9999\n",
        )
        .unwrap();

        let config = BrokerConfig::load_from(&tmp).unwrap();

        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 9999);
    }

    #[test]
    fn test_invalid_toml_returns_parse_error() {
        let tmp = env::temp_dir().join("rusty_mqtt_test_invalid_toml");
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(
            tmp.join("rusty-mqtt.toml"),
            "das ist %%% kein gueltiges TOML {{{\n",
        )
        .unwrap();

        let result = BrokerConfig::load_from(&tmp);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ConfigError::ParseError(_)));
    }

    #[test]
    fn test_port_zero_returns_validation_error() {
        let tmp = env::temp_dir().join("rusty_mqtt_test_port_zero");
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("rusty-mqtt.toml"), "port = 0\n").unwrap();

        let result = BrokerConfig::load_from(&tmp);

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ConfigError::ValidationError(_)
        ));
    }

    #[test]
    fn test_port_above_65535_returns_parse_error() {
        let tmp = env::temp_dir().join("rusty_mqtt_test_port_high");
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("rusty-mqtt.toml"), "port = 70000\n").unwrap();

        let result = BrokerConfig::load_from(&tmp);

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_server_uses_config_address() {
        use rusty_mqtt::MqttServer;
        use tokio::net::TcpStream;

        let config = BrokerConfig {
            host: "127.0.0.1".to_string(),
            port: 18840,
        };

        let mut server = MqttServer::from_config(config);

        // Server im Hintergrund starten
        let handle = tokio::spawn(async move {
            let _ = server.run().await;
        });

        // Kurz warten bis der Server bereit ist
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Verbindung auf dem konfigurierten Port muss funktionieren
        let result = TcpStream::connect("127.0.0.1:18840").await;
        assert!(
            result.is_ok(),
            "Server sollte auf konfiguriertem Port horchen"
        );

        handle.abort();
    }
}
