use rusty_mqtt::{BrokerConfig, MqttServer};
use std::env;
use std::error::Error;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("rusty_mqtt=info".parse()?))
        .init();

    let config_dir = env::current_dir()?;
    let config_path = config_dir.join("rusty-mqtt.toml");

    if config_path.exists() {
        info!(path = %config_path.display(), "Configuration loaded");
    } else {
        info!("No configuration file found, using defaults");
    }

    let config = BrokerConfig::load_from(&config_dir)?;
    let mut server = MqttServer::from_config(config);
    server.run().await?;
    Ok(())
}
