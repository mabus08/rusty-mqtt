use std::error::Error;
use std::fmt;
use std::path::Path;
use std::sync::{Arc, Mutex};
pub mod client_registry;
pub mod subscribe_handlers;
use bytes::Bytes;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::time::{Duration, timeout};

use crate::client_registry::{ClientRegistry, ConnectionCommand};

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

/// Capacity des per-Connection mpsc-Channels fuer auszuliefernde Frames.
const SUBSCRIBER_CHANNEL_CAPACITY: usize = 32;

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
    client_registry: Arc<ClientRegistry>,
}

impl MqttServer {
    pub fn new(addr: &str) -> Self {
        Self {
            address: addr.to_string(),
            topic_router: Arc::new(Mutex::new(TopicRouter::new())),
            client_registry: Arc::new(ClientRegistry::new()),
        }
    }

    /// Erstellt einen MqttServer aus einer BrokerConfig.
    pub fn from_config(config: BrokerConfig) -> Self {
        Self {
            address: config.address(),
            topic_router: Arc::new(Mutex::new(TopicRouter::new())),
            client_registry: Arc::new(ClientRegistry::new()),
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
            let registry = Arc::clone(&self.client_registry);
            tokio::spawn(async move {
                if let Err(e) = MqttServer::handle_connection(socket, router, registry).await {
                    eprintln!("Fehler in Client-Verbindung: {}", e);
                }
            });
        }
    }

    async fn handle_connection(
        socket: TcpStream,
        router: Arc<Mutex<TopicRouter>>,
        registry: Arc<ClientRegistry>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (mut reader, mut writer) = socket.into_split();
        let mut buffer = [0u8; 1024];
        let timeout_duration = Duration::from_secs(300);
        let mut client_id: Option<String> = None;
        let (tx, mut rx) = mpsc::channel::<ConnectionCommand>(SUBSCRIBER_CHANNEL_CAPACITY);

        loop {
            tokio::select! {
                read_result = timeout(timeout_duration, reader.read(&mut buffer)) => {
                    let bytes_read = match read_result {
                        Ok(Ok(n)) => n,
                        Ok(Err(e)) => return Err(format!("Read error: {}", e).into()),
                        Err(_) => { println!("Client Timeout"); break; }
                    };
                    if bytes_read == 0 {
                        println!("Client disconnected");
                        break;
                    }
                    let mut data = &buffer[..bytes_read];
                    let mut should_break = false;
                    while let Some(len) = frame_length(data) {
                        if len > data.len() {
                            // partial frame at buffer tail — MVP-Annahme: read() liefert
                            // ganze Frames. Wird mit Framed in spaeterer Phase robuster.
                            break;
                        }
                        let frame = &data[..len];
                        data = &data[len..];
                        let packet_type = MqttServer::parse_packet(frame);
                        match packet_type {
                        MqttPacket::Connect => {
                            let parsed_client_id = MqttServer::parse_connect_client_id(frame);
                            println!("Connection from client: {:?}", parsed_client_id);

                            // Takeover (ADR-0004): vor CONNACK alte Session synchron
                            // beenden, damit deren Cleanup vor unseren Subscriptions
                            // laeuft.
                            if let Some(old) = registry.swap_in(&parsed_client_id, tx.clone()) {
                                let _ = old.send(ConnectionCommand::Disconnect).await;
                                old.closed().await;
                            }
                            client_id = Some(parsed_client_id);

                            let connack = [0x20, 0x02, 0x00, 0x00];
                            writer.write_all(&connack).await?;
                        }
                        MqttPacket::Subscribe(_packet_id) => {
                            let cid = client_id.as_deref().unwrap_or("unknown").to_string();
                            let suback = MqttServer::handle_subscribe_impl(frame, &cid, &router, &tx)?;
                            writer.write_all(&suback).await?;
                            println!("SUBACK sent for client: {}", cid);
                        }
                        MqttPacket::PINGREQ => {
                            let pingresp = [0xD0, 0x00];
                            writer.write_all(&pingresp).await?;
                        }
                        MqttPacket::Disconnect => {
                            println!("Client sent DISCONNECT");
                            should_break = true;
                            break;
                        }
                        MqttPacket::Publish(_topic) => {
                            if let Some((topic, payload)) = parse_publish(frame) {
                                let outbound = encode_publish(&topic, &payload);
                                let bytes = Bytes::from(outbound);
                                // Snapshot der zustaendigen Sender, dann Lock freigeben.
                                let senders: Vec<mpsc::Sender<ConnectionCommand>> = match router.lock() {
                                    Ok(r) => r
                                        .get_subscribers_for_topic(&topic)
                                        .into_iter()
                                        .map(|s| s.tx.clone())
                                        .collect(),
                                    Err(_) => Vec::new(),
                                };
                                for s in senders {
                                    let _ = s.try_send(ConnectionCommand::DeliverFrame(bytes.clone())); // drop-on-full
                                }
                            }
                        }
                        _ => {}
                        }
                    }
                    if should_break { break; }
                }
                Some(cmd) = rx.recv() => {
                    match cmd {
                        ConnectionCommand::DeliverFrame(frame) => {
                            writer.write_all(&frame).await?;
                        }
                        ConnectionCommand::Disconnect => {
                            // Vom Broker angeordnetes Beenden (z.B. Takeover).
                            break;
                        }
                    }
                }
            }
        }

        // Cleanup-Pfad: Router + Registry. Single source of truth.
        if let Some(ref cid) = client_id {
            if let Ok(mut r) = router.lock() {
                r.remove_client(cid);
            }
            registry.remove_if_owner(cid, &tx);
        }

        Ok(())
    }

    /// Parst ein SUBSCRIBE-Paket, speichert Subscriptions im Router,
    /// und gibt ein SUBACK mit den korrekten granted QoS-Werten zurueck.
    pub fn handle_subscribe_impl(
        buffer: &[u8],
        client_id: &str,
        router: &Arc<Mutex<TopicRouter>>,
        tx: &mpsc::Sender<ConnectionCommand>,
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
            .subscribe(client_id, &topic_filters, tx);

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

/// Bestimmt die Gesamtlaenge eines Frames (Fixed Header + Varint + Remaining Length).
/// Liefert `None` wenn `buffer` zu kurz ist, um die Varint vollstaendig zu lesen.
fn frame_length(buffer: &[u8]) -> Option<usize> {
    if buffer.is_empty() {
        return None;
    }
    let mut offset = 1usize;
    let mut multiplier: usize = 1;
    let mut remaining: usize = 0;
    for _ in 0..4 {
        if offset >= buffer.len() {
            return None;
        }
        let b = buffer[offset];
        offset += 1;
        remaining += (b & 0x7F) as usize * multiplier;
        if b & 0x80 == 0 {
            return Some(offset + remaining);
        }
        multiplier *= 128;
    }
    None
}

/// Parst ein eingehendes QoS-0-PUBLISH-Frame und liefert `(topic, payload)`.
/// Gibt `None` zurueck wenn das Frame nicht parsierbar ist (zu kurz, ungueltige
/// Topic-Laenge etc.). Multi-Byte-Varint wird bis 4 Bytes unterstuetzt.
fn parse_publish(buffer: &[u8]) -> Option<(String, Bytes)> {
    if buffer.is_empty() || (buffer[0] & 0xF0) != 0x30 {
        return None;
    }
    // QoS aus Fixed Header
    let qos = (buffer[0] >> 1) & 0x03;
    if qos != 0 {
        // MAX_SUPPORTED_QOS = 0 — alles andere wird im MVP verworfen.
        return None;
    }

    // Varint Remaining Length
    let mut offset = 1usize;
    let mut multiplier: usize = 1;
    let mut remaining: usize = 0;
    for _ in 0..4 {
        if offset >= buffer.len() {
            return None;
        }
        let b = buffer[offset];
        offset += 1;
        remaining += (b & 0x7F) as usize * multiplier;
        if b & 0x80 == 0 {
            break;
        }
        multiplier *= 128;
    }

    let body_end = offset + remaining;
    if body_end > buffer.len() {
        return None;
    }
    if offset + 2 > body_end {
        return None;
    }
    let topic_len = ((buffer[offset] as usize) << 8) | (buffer[offset + 1] as usize);
    offset += 2;
    if offset + topic_len > body_end {
        return None;
    }
    let topic = std::str::from_utf8(&buffer[offset..offset + topic_len]).ok()?;
    offset += topic_len;

    let payload = Bytes::copy_from_slice(&buffer[offset..body_end]);
    Some((topic.to_string(), payload))
}

/// Serialisiert ein ausgehendes QoS-0-PUBLISH-Frame ohne DUP/RETAIN-Flags.
fn encode_publish(topic: &str, payload: &[u8]) -> Vec<u8> {
    let topic_bytes = topic.as_bytes();
    let remaining_len = 2 + topic_bytes.len() + payload.len();
    let mut out = Vec::with_capacity(2 + remaining_len);
    out.push(0x30);
    encode_remaining_length(remaining_len, &mut out);
    out.extend_from_slice(&(topic_bytes.len() as u16).to_be_bytes());
    out.extend_from_slice(topic_bytes);
    out.extend_from_slice(payload);
    out
}

/// Schreibt eine MQTT Variable Byte Integer (Remaining Length) ans Ende von `out`.
fn encode_remaining_length(mut value: usize, out: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value > 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}
