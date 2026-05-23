use std::error::Error;
use std::fmt;
use std::path::Path;
use std::sync::{Arc, Mutex};
pub mod subscribe_handlers;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{Duration, timeout};

pub use subscribe_handlers::{SubscribeHandler, TopicRouter};

const CONFIG_FILE_NAME: &str = "rusty-mqtt.toml";
const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 1884;

/// Fehler beim Laden der Broker-Konfiguration.
#[derive(Debug)]
pub enum ConfigError {
    /// TOML-Datei konnte nicht geparst werden.
    ParseError(String),
    /// Konfigurationswerte sind ungueltig.
    ValidationError(String),
    /// Dateisystemfehler beim Lesen der Datei.
    IoError(std::io::Error),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::ParseError(msg) => write!(f, "Konfigurationsfehler: {}", msg),
            ConfigError::ValidationError(msg) => write!(f, "Validierungsfehler: {}", msg),
            ConfigError::IoError(err) => write!(f, "IO-Fehler: {}", err),
        }
    }
}

impl std::error::Error for ConfigError {}

/// Broker-Konfiguration mit Host und Port.
#[derive(Debug, serde::Deserialize)]
pub struct BrokerConfig {
    /// Bind-Adresse des Brokers.
    #[serde(default = "default_host")]
    pub host: String,
    /// Port des Brokers.
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_host() -> String {
    DEFAULT_HOST.to_string()
}

fn default_port() -> u16 {
    DEFAULT_PORT
}

impl Default for BrokerConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
        }
    }
}

impl BrokerConfig {
    /// Laedt Konfiguration aus `rusty-mqtt.toml` im angegebenen Verzeichnis.
    /// Fehlt die Datei, werden Defaults verwendet.
    pub fn load_from(dir: &Path) -> Result<Self, ConfigError> {
        let config_path = dir.join(CONFIG_FILE_NAME);

        if !config_path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&config_path).map_err(ConfigError::IoError)?;

        let config: BrokerConfig =
            toml::from_str(&content).map_err(|e| ConfigError::ParseError(e.to_string()))?;

        config.validate()?;

        Ok(config)
    }

    /// Prueft ob die Konfigurationswerte gueltig sind.
    fn validate(&self) -> Result<(), ConfigError> {
        if self.port == 0 {
            return Err(ConfigError::ValidationError(
                "Port darf nicht 0 sein".to_string(),
            ));
        }
        Ok(())
    }

    /// Erzeugt die vollstaendige Bind-Adresse als String.
    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

#[derive(Debug, PartialEq)]
pub enum MqttPacket {
    Connect,
    Subscribe(u16),
    Unsubscribe(u16),
    Publish(String),
    Disconnect,
    PINGREQ,
    Unknown,
}

pub struct MqttServer {
    #[allow(unused)]
    address: String,
    topic_router: Arc<Mutex<TopicRouter>>,
}

impl MqttServer {
    pub fn new(addr: &str) -> Self {
        Self {
            address: addr.to_string(),
            topic_router: Arc::new(Mutex::new(TopicRouter::new())),
        }
    }

    /// Erstellt einen MqttServer aus einer BrokerConfig.
    pub fn from_config(config: BrokerConfig) -> Self {
        Self {
            address: config.address(),
            topic_router: Arc::new(Mutex::new(TopicRouter::new())),
        }
    }

    pub async fn run(&mut self) -> Result<(), Box<dyn Error>> {
        let listener = TcpListener::bind(&self.address).await?;
        let actual_addr = listener.local_addr()?;
        println!("MQTT Broker horcht auf: {}", actual_addr);

        loop {
            let (socket, _) = match listener.accept().await {
                Ok(result) => result,
                Err(e) => {
                    eprintln!("Akzeptieren fehlgeschlagen: {}", e);
                    continue;
                }
            };

            let router = Arc::clone(&self.topic_router);
            tokio::spawn(async move {
                if let Err(e) = MqttServer::handle_connection(socket, router).await {
                    eprintln!("Fehler in Client-Verbindung: {}", e);
                }
            });
        }
    }

    async fn handle_connection(
        mut socket: TcpStream,
        router: Arc<Mutex<TopicRouter>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut buffer = [0u8; 1024];
        let timeout_duration = Duration::from_secs(300);
        let mut client_id: Option<String> = None;

        loop {
            let bytes_read = match timeout(timeout_duration, socket.read(&mut buffer)).await {
                Ok(result) => result.map_err(|e| format!("Read error: {}", e))?,
                Err(_) => {
                    println!("Client Timeout");
                    break;
                }
            };

            if bytes_read == 0 {
                println!("Client disconnected");
                break;
            }

            let packet_type = MqttServer::parse_packet(&buffer[..bytes_read]);

            match packet_type {
                MqttPacket::Connect => {
                    let parsed_client_id = MqttServer::parse_connect_client_id(&buffer[..bytes_read]);
                    println!("Connection from client: {:?}", parsed_client_id);
                    client_id = Some(parsed_client_id);

                    let connack = [0x20, 0x02, 0x00, 0x00];
                    socket.write_all(&connack).await?;
                }
                MqttPacket::Subscribe(_packet_id) => {
                    let cid = client_id.as_deref().unwrap_or("unknown");
                    let suback = MqttServer::handle_subscribe_impl(
                        &buffer[..bytes_read],
                        cid,
                        &router,
                    )?;
                    socket.write_all(&suback).await?;
                    println!("SUBACK sent for client: {}", cid);
                }
                MqttPacket::PINGREQ => {
                    let pingresp = [0xD0, 0x00];
                    socket.write_all(&pingresp).await?;
                }
                MqttPacket::Disconnect => {
                    println!("Client sent DISCONNECT");
                    break;
                }
                MqttPacket::Publish(_topic) => {
                    // TODO: Publish handling with topic routing via TopicRouter
                }
                _ => {}
            }
        }

        // Client-Subscriptions aufraeumen bei Disconnect/Timeout
        if let Some(ref cid) = client_id {
            if let Ok(mut r) = router.lock() {
                r.remove_client(cid);
            }
        }

        Ok(())
    }

    /// Parst ein SUBSCRIBE-Paket, speichert Subscriptions im Router,
    /// und gibt ein SUBACK mit den korrekten granted QoS-Werten zurueck.
    pub fn handle_subscribe_impl(
        buffer: &[u8],
        client_id: &str,
        router: &Arc<Mutex<TopicRouter>>,
    ) -> Result<Vec<u8>, String> {
        if buffer.len() < 4 {
            return Err("Buffer too short for SUBSCRIBE".to_string());
        }

        let packet_id = ((buffer[2] as u16) << 8) | (buffer[3] as u16);

        let mut topic_filters: Vec<(String, u8)> = Vec::new();
        let mut offset = 4usize;

        while offset + 2 < buffer.len() {
            let topic_len = ((buffer[offset] as usize) << 8) | (buffer[offset + 1] as usize);
            offset += 2;

            if offset + topic_len >= buffer.len() {
                break;
            }

            let topic = String::from_utf8_lossy(&buffer[offset..offset + topic_len]).to_string();
            offset += topic_len;

            if offset >= buffer.len() {
                break;
            }

            let qos = buffer[offset];
            offset += 1;

            topic_filters.push((topic, qos));
        }

        // Subscriptions im Router speichern und granted QoS erhalten
        let granted_qos = router
            .lock()
            .map_err(|e| format!("Router lock failed: {}", e))?
            .subscribe(client_id, &topic_filters);

        Ok(SubscribeHandler::generate_suback(packet_id, &granted_qos))
    }

    /// Extrahiert die Client ID aus einem CONNECT-Paket (MQTT 3.1.1).
    ///
    /// CONNECT Layout:
    ///   Byte 0:    Fixed Header (0x10)
    ///   Byte 1:    Remaining Length
    ///   Byte 2-8:  Variable Header (Protocol Name "MQTT" + Protocol Level + Connect Flags + Keep Alive)
    ///   Payload:   Client ID (UTF-8 length-prefixed String)
    pub fn parse_connect_client_id(buffer: &[u8]) -> String {
        // Minimum: Fixed Header (2) + Variable Header (10) + Client ID Length (2) = 14
        if buffer.len() < 14 {
            return "unknown".to_string();
        }

        // Remaining Length (simplified: single-byte encoding, ausreichend fuer MVP)
        let remaining_start = 2usize;

        // Variable Header: 7 bytes Protocol Name ("MQTT") + Level + Flags + Keep Alive
        // Protocol Name Length (2 bytes) + "MQTT" (4 bytes) + Protocol Level (1) + Connect Flags (1) + Keep Alive (2) = 10 bytes
        let client_id_offset = remaining_start + 10;

        if buffer.len() < client_id_offset + 2 {
            return "unknown".to_string();
        }

        let client_id_len =
            ((buffer[client_id_offset] as usize) << 8) | (buffer[client_id_offset + 1] as usize);

        let client_id_start = client_id_offset + 2;

        if buffer.len() < client_id_start + client_id_len {
            return "unknown".to_string();
        }

        String::from_utf8_lossy(&buffer[client_id_start..client_id_start + client_id_len])
            .to_string()
    }

    pub fn parse_packet(buffer: &[u8]) -> MqttPacket {
        if buffer.is_empty() {
            return MqttPacket::Unknown;
        }

        let control_packet_type = buffer[0] >> 4;
        match control_packet_type {
            1 => MqttPacket::Connect,
            3 => MqttPacket::Publish(String::new()),
            8 => {
                if buffer.len() > 3 {
                    let packet_id = ((buffer[2] as u16) << 8) | (buffer[3] as u16);
                    MqttPacket::Subscribe(packet_id)
                } else {
                    MqttPacket::Unknown
                }
            }
            12 => MqttPacket::PINGREQ,
            14 => MqttPacket::Disconnect,
            _ => MqttPacket::Unknown,
        }
    }

    pub fn extract_packet_id(buffer: &[u8], offset: usize) -> Result<u16, String> {
        if buffer.len() > offset + 1 {
            let byte1 = buffer[offset] & 0x0F;
            let byte2 = buffer[offset + 1];
            Ok((byte1 as u16) << 8 | byte2 as u16)
        } else {
            Err("Buffer zu kurz fuer Packet ID".into())
        }
    }

    pub fn get_address(&self) -> &str {
        &self.address
    }
}

pub fn extract_packet_id(buffer: &[u8], offset: usize) -> Result<u16, String> {
    MqttServer::extract_packet_id(buffer, offset)
}
