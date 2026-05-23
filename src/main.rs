use rusty_mqtt::{BrokerConfig, MqttServer};
use std::env;
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config_dir = env::current_dir()?;
    let config_path = config_dir.join("rusty-mqtt.toml");

    if config_path.exists() {
        println!("Konfiguration geladen aus: {}", config_path.display());
    } else {
        println!("Keine Konfigurationsdatei gefunden, verwende Defaults");
    }

    let config = BrokerConfig::load_from(&config_dir)?;
    let mut server = MqttServer::from_config(config);
    server.run().await?;
    Ok(())
}
