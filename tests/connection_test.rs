use rusty_mqtt::MqttServer;

#[tokio::test]
async fn test_server_run() {
    let _server = MqttServer::new("127.0.0.1:1883");

    // This will bind and listen in background
}
