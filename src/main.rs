use rusty_mqtt::MqttServer;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Hier nutzen wir die Library
    let server = MqttServer::new("127.0.0.1:1883");
    server.run().await?;
    Ok(())
}
