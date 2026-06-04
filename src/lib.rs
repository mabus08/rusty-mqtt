//! `rusty-mqtt` — MQTT 3.1.1 Broker MVP (QoS 0).
//!
//! Contains the [`MqttServer`] accept loop, the Connection Task with Keep-Alive
//! and fanout logic, as well as helper modules for codec pre-stages, topic routing
//! and the client registry.
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
use tracing::{debug, error, info, warn};

use crate::client_registry::{ClientRegistry, ConnectionCommand};

pub use subscribe_handlers::{SubscribeHandler, TopicRouter};

const CONFIG_FILE_NAME: &str = "rusty-mqtt.toml";
const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 1884;

/// Error loading the broker configuration.
#[derive(Debug)]
pub enum ConfigError {
    /// TOML file could not be parsed.
    ParseError(String),
    /// Configuration values are invalid.
    ValidationError(String),
    /// File-system error while reading the file.
    IoError(std::io::Error),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::ParseError(msg) => write!(f, "Configuration error: {}", msg),
            ConfigError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            ConfigError::IoError(err) => write!(f, "IO error: {}", err),
        }
    }
}

impl std::error::Error for ConfigError {}

/// Broker configuration with host and port.
#[derive(Debug, serde::Deserialize)]
pub struct BrokerConfig {
    /// Bind address of the broker.
    #[serde(default = "default_host")]
    pub host: String,
    /// Port of the broker.
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
    /// Loads configuration from `rusty-mqtt.toml` in the given directory.
    /// If the file is missing, defaults are used.
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

    /// Checks whether the configuration values are valid.
    fn validate(&self) -> Result<(), ConfigError> {
        if self.port == 0 {
            return Err(ConfigError::ValidationError(
                "Port must not be 0".to_string(),
            ));
        }
        Ok(())
    }

    /// Returns the full bind address as a string.
    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// Capacity of the per-connection mpsc channel for frames to be delivered.
const SUBSCRIBER_CHANNEL_CAPACITY: usize = 32;

/// Maximum QoS level supported by the broker.
/// Raised once QoS 1/2 is implemented.
const MAX_SUPPORTED_QOS: u8 = 0;

/// Classification of an incoming MQTT Control Packet (wire-level type).
///
/// Extracted from the Fixed Header byte by `parse_packet` and used in the
/// Connection Task match. Replaced later by the full `MqttPacket` codec.
#[derive(Debug, PartialEq)]
pub enum MqttPacket {
    /// CONNECT (type 1).
    Connect,
    /// SUBSCRIBE (type 8) with Packet Identifier.
    Subscribe(u16),
    /// UNSUBSCRIBE (type 10) with Packet Identifier.
    Unsubscribe(u16),
    /// PUBLISH (type 3) with topic (not yet fully parsed).
    Publish(String),
    /// DISCONNECT (type 14).
    Disconnect,
    /// PINGREQ (type 12).
    PINGREQ,
    /// Unknown or unsupported packet type.
    Unknown,
}

/// MQTT broker server; holds the listener address, TopicRouter and ClientRegistry.
pub struct MqttServer {
    #[allow(unused)]
    address: String,
    topic_router: Arc<Mutex<TopicRouter>>,
    client_registry: Arc<ClientRegistry>,
}

impl MqttServer {
    /// Creates a new `MqttServer` that will listen on `addr` (e.g. `"127.0.0.1:1883"`).
    pub fn new(addr: &str) -> Self {
        Self {
            address: addr.to_string(),
            topic_router: Arc::new(Mutex::new(TopicRouter::new())),
            client_registry: Arc::new(ClientRegistry::new()),
        }
    }

    /// Creates an `MqttServer` from a `BrokerConfig`.
    pub fn from_config(config: BrokerConfig) -> Self {
        Self {
            address: config.address(),
            topic_router: Arc::new(Mutex::new(TopicRouter::new())),
            client_registry: Arc::new(ClientRegistry::new()),
        }
    }

    /// Binds the TCP listener and starts the accept loop.
    ///
    /// Runs until process exit (no graceful shutdown in MVP).
    pub async fn run(&mut self) -> Result<(), Box<dyn Error>> {
        let listener = TcpListener::bind(&self.address).await?;
        let actual_addr = listener.local_addr()?;
        info!(address = %actual_addr, "MQTT broker listening on");

        loop {
            let (socket, _) = match listener.accept().await {
                Ok(result) => result,
                Err(e) => {
                    error!(error = %e, "Accept failed");
                    continue;
                }
            };

            let router = Arc::clone(&self.topic_router);
            let registry = Arc::clone(&self.client_registry);
            tokio::spawn(async move {
                if let Err(e) = MqttServer::handle_connection(socket, router, registry).await {
                    error!(error = %e, "Error in client connection");
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
        let mut buffer = [0u8; 4096];
        let timeout_duration = Duration::from_secs(300);
        let mut client_id: Option<String> = None;
        let (tx, mut rx) = mpsc::channel::<ConnectionCommand>(SUBSCRIBER_CHANNEL_CAPACITY);
        // Keep-Alive: 0 means disabled; otherwise 1.5 × keep_alive_secs.
        // keep_alive_millis == 0 => no timeout arm active.
        let mut keep_alive_millis: u64 = 0;
        let mut keep_alive_deadline: Option<tokio::time::Instant> = None;
        let far_future = tokio::time::Instant::now() + Duration::from_secs(u32::MAX as u64);

        loop {
            // Deadline for the Keep-Alive arm: if it is in the past
            // or no Keep-Alive is configured, we wait on far_future
            // (effectively disabled).
            let ka_instant = keep_alive_deadline.unwrap_or(far_future);

            tokio::select! {
                _ = tokio::time::sleep_until(ka_instant), if keep_alive_deadline.is_some() => {
                    // Keep-Alive expired — close connection.
                    debug!(client_id = ?client_id, "Keep-Alive timeout");
                    break;
                }
                read_result = timeout(timeout_duration, reader.read(&mut buffer)) => {
                    let bytes_read = match read_result {
                        Ok(Ok(n)) => n,
                        Ok(Err(e)) => return Err(format!("Read error: {}", e).into()),
                        Err(_) => { debug!(client_id = ?client_id, "Client Timeout"); break; }
                    };
                    if bytes_read == 0 {
                        debug!(client_id = ?client_id, "Client disconnected");
                        break;
                    }
                    // Every incoming frame resets the Keep-Alive timer.
                    if keep_alive_millis > 0 {
                        keep_alive_deadline = Some(
                            tokio::time::Instant::now()
                                + Duration::from_millis(keep_alive_millis),
                        );
                    }
                    let mut data = &buffer[..bytes_read];
                    let mut should_break = false;
                    while let Some(len) = frame_length(data) {
                        if len > data.len() {
                            // Partial frame at buffer tail — MVP assumption: read() delivers
                            // complete frames. Will be made more robust with Framed in a later phase.
                            break;
                        }
                        let frame = &data[..len];
                        data = &data[len..];
                        let packet_type = MqttServer::parse_packet(frame);
                        match packet_type {
                        MqttPacket::Connect => {
                            match validate_connect(frame) {
                                ConnectValidation::Ok {
                                    client_id: parsed_client_id,
                                    keep_alive_secs,
                                } => {
                                    info!(client_id = %parsed_client_id, "Client connected");

                                    // Configure Keep-Alive: deadline = 1.5 × keep_alive.
                                    if keep_alive_secs > 0 {
                                        keep_alive_millis =
                                            (keep_alive_secs as u64) * 1500;
                                        keep_alive_deadline = Some(
                                            tokio::time::Instant::now()
                                                + Duration::from_millis(keep_alive_millis),
                                        );
                                    }

                                    // Takeover (ADR-0004): synchronously terminate the old session
                                    // before CONNACK so that its cleanup runs before our subscriptions.
                                    if let Some(old) =
                                        registry.swap_in(&parsed_client_id, tx.clone())
                                    {
                                        let _ = old.send(ConnectionCommand::Disconnect).await;
                                        old.closed().await;
                                    }
                                    client_id = Some(parsed_client_id);

                                    let connack = [0x20, 0x02, 0x00, 0x00];
                                    writer.write_all(&connack).await?;
                                }
                                ConnectValidation::BadProtocolName
                                | ConnectValidation::ReservedBitSet => {
                                    // Malformed: close socket without CONNACK.
                                    should_break = true;
                                    break;
                                }
                                ConnectValidation::BadProtocolLevel => {
                                    let connack = [0x20, 0x02, 0x00, 0x01];
                                    writer.write_all(&connack).await?;
                                    should_break = true;
                                    break;
                                }
                                ConnectValidation::EmptyClientId => {
                                    let connack = [0x20, 0x02, 0x00, 0x02];
                                    writer.write_all(&connack).await?;
                                    should_break = true;
                                    break;
                                }
                            }
                        }
                        MqttPacket::Subscribe(_packet_id) => {
                            let cid = client_id.as_deref().unwrap_or("unknown").to_string();
                            let suback = MqttServer::handle_subscribe_impl(frame, &cid, &router, &tx)?;
                            writer.write_all(&suback).await?;
                            debug!(client_id = %cid, "SUBACK sent");
                        }
                        MqttPacket::Unsubscribe(packet_id) => {
                            let cid = client_id.as_deref().unwrap_or("unknown");
                            let filters = parse_unsubscribe_filters(frame);
                            if let Ok(mut r) = router.lock() {
                                r.unsubscribe(cid, &filters);
                            }
                            // UNSUBACK: fixed header 0xB0, remaining length 2, packet id
                            let unsuback = [
                                0xB0,
                                0x02,
                                (packet_id >> 8) as u8,
                                (packet_id & 0xFF) as u8,
                            ];
                            writer.write_all(&unsuback).await?;
                        }
                        MqttPacket::PINGREQ => {
                            let pingresp = [0xD0, 0x00];
                            writer.write_all(&pingresp).await?;
                        }
                        MqttPacket::Disconnect => {
                            debug!(client_id = ?client_id, "Client sent DISCONNECT");
                            should_break = true;
                            break;
                        }
                        MqttPacket::Publish(_topic) => {
                            if let Some((topic, payload)) = parse_publish(frame) {
                                debug!(client_id = ?client_id, topic = %topic, bytes = payload.len(), "PUBLISH routed");
                                let outbound = encode_publish(&topic, &payload);
                                let bytes = Bytes::from(outbound);
                                // Snapshot the relevant senders, then release the lock.
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
                            } else {
                                warn!(client_id = ?client_id, "PUBLISH dropped (wildcard topic or QoS > 0)");
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
                            // Server-initiated termination (e.g. takeover).
                            break;
                        }
                    }
                }
            }
        }

        // Cleanup path: router + registry. Single source of truth.
        if let Some(ref cid) = client_id {
            if let Ok(mut r) = router.lock() {
                r.remove_client(cid);
            }
            registry.remove_if_owner(cid, &tx);
        }

        Ok(())
    }

    /// Parses a SUBSCRIBE packet, stores subscriptions in the router,
    /// and returns a SUBACK with the correct granted QoS values.
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

            // Cap QoS at the broker maximum (silent downgrade per spec).
            // MAX_SUPPORTED_QOS == 0 => always 0 in MVP.
            #[allow(clippy::unnecessary_min_or_max)]
            let granted = qos.min(MAX_SUPPORTED_QOS);
            topic_filters.push((topic, granted));
        }

        // Store subscriptions in the router and obtain granted QoS values.
        let granted_qos = router
            .lock()
            .map_err(|e| format!("Router lock failed: {}", e))?
            .subscribe(client_id, &topic_filters, tx);

        Ok(SubscribeHandler::generate_suback(packet_id, &granted_qos))
    }

    /// Extracts the Client ID from a CONNECT packet (MQTT 3.1.1).
    ///
    /// CONNECT layout:
    ///   Byte 0:    Fixed Header (0x10)
    ///   Byte 1:    Remaining Length
    ///   Byte 2-8:  Variable Header (Protocol Name "MQTT" + Protocol Level + Connect Flags + Keep Alive)
    ///   Payload:   Client ID (UTF-8 length-prefixed string)
    pub fn parse_connect_client_id(buffer: &[u8]) -> String {
        // Minimum: Fixed Header (2) + Variable Header (10) + Client ID Length (2) = 14
        if buffer.len() < 14 {
            return "unknown".to_string();
        }

        // Remaining Length (simplified: single-byte encoding, sufficient for MVP)
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

    /// Extracts the packet type from the Fixed Header byte of an MQTT frame.
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
            10 => {
                // UNSUBSCRIBE — fixed header byte must be 0xA2
                if buffer.len() > 3 {
                    let packet_id = ((buffer[2] as u16) << 8) | (buffer[3] as u16);
                    MqttPacket::Unsubscribe(packet_id)
                } else {
                    MqttPacket::Unknown
                }
            }
            12 => MqttPacket::PINGREQ,
            14 => MqttPacket::Disconnect,
            _ => MqttPacket::Unknown,
        }
    }

    /// Reads a 2-byte Packet ID from `buffer` at `offset`.
    pub fn extract_packet_id(buffer: &[u8], offset: usize) -> Result<u16, String> {
        if buffer.len() > offset + 1 {
            let byte1 = buffer[offset] & 0x0F;
            let byte2 = buffer[offset + 1];
            Ok((byte1 as u16) << 8 | byte2 as u16)
        } else {
            Err("Buffer too short for Packet ID".into())
        }
    }

    pub fn get_address(&self) -> &str {
        &self.address
    }
}

/// Reads a 2-byte Packet ID from `buffer` at `offset`.
///
/// Delegates to [`MqttServer::extract_packet_id`].
pub fn extract_packet_id(buffer: &[u8], offset: usize) -> Result<u16, String> {
    MqttServer::extract_packet_id(buffer, offset)
}

/// Classification of a CONNECT packet according to MQTT 3.1.1 validation.
///
/// Four observable paths per spec / PRD 0001:
/// - `Ok`              — CONNACK 0x00, session proceeds.
/// - `BadProtocolName` — discard frame as malformed, close socket without CONNACK.
/// - `BadProtocolLevel`— CONNACK with Return Code 0x01, then close.
/// - `ReservedBitSet`  — discard frame as malformed, close socket without CONNACK.
/// - `EmptyClientId`   — CONNACK with Return Code 0x02, then close.
#[derive(Debug)]
pub enum ConnectValidation {
    /// Valid CONNECT; Client ID and Keep-Alive extracted.
    Ok {
        /// Client ID from the payload.
        client_id: String,
        /// Keep-Alive interval in seconds (0 = disabled).
        keep_alive_secs: u16,
    },
    /// Protocol Name ≠ "MQTT".
    BadProtocolName,
    /// Protocol Level ≠ 4 (3.1.1).
    BadProtocolLevel,
    /// Reserved bit (bit 0 of Connect Flags) is set.
    ReservedBitSet,
    /// Payload Client ID string is empty.
    EmptyClientId,
}

/// Validates a CONNECT frame and classifies the result.
///
/// Expects the complete frame including Fixed Header. Multi-byte Varint
/// (up to 4 bytes) is handled.
pub fn validate_connect(buffer: &[u8]) -> ConnectValidation {
    // Minimum length: Fixed Header (>=2) + Variable Header (10) + Client ID Length (2)
    if buffer.is_empty() || (buffer[0] & 0xF0) != 0x10 {
        return ConnectValidation::BadProtocolName;
    }

    // Skip Varint Remaining Length
    let mut offset = 1usize;
    let mut multiplier: usize = 1;
    let mut remaining: usize = 0;
    for _ in 0..4 {
        if offset >= buffer.len() {
            return ConnectValidation::BadProtocolName;
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
        return ConnectValidation::BadProtocolName;
    }

    // Protocol Name: 2 bytes length + bytes
    if offset + 2 > body_end {
        return ConnectValidation::BadProtocolName;
    }
    let name_len = ((buffer[offset] as usize) << 8) | (buffer[offset + 1] as usize);
    offset += 2;
    if offset + name_len > body_end {
        return ConnectValidation::BadProtocolName;
    }
    if &buffer[offset..offset + name_len] != b"MQTT" {
        return ConnectValidation::BadProtocolName;
    }
    offset += name_len;

    // Protocol Level (1 byte)
    if offset >= body_end {
        return ConnectValidation::BadProtocolName;
    }
    let level = buffer[offset];
    offset += 1;
    if level != 0x04 {
        return ConnectValidation::BadProtocolLevel;
    }

    // Connect Flags (1 byte)
    if offset >= body_end {
        return ConnectValidation::BadProtocolName;
    }
    let flags = buffer[offset];
    offset += 1;
    if flags & 0x01 != 0 {
        return ConnectValidation::ReservedBitSet;
    }

    // Read Keep Alive (2 bytes)
    if offset + 2 > body_end {
        return ConnectValidation::BadProtocolName;
    }
    let keep_alive_secs = ((buffer[offset] as u16) << 8) | (buffer[offset + 1] as u16);
    offset += 2;

    // Client ID (length-prefixed UTF-8)
    if offset + 2 > body_end {
        return ConnectValidation::BadProtocolName;
    }
    let cid_len = ((buffer[offset] as usize) << 8) | (buffer[offset + 1] as usize);
    offset += 2;
    if cid_len == 0 {
        return ConnectValidation::EmptyClientId;
    }
    if offset + cid_len > body_end {
        return ConnectValidation::BadProtocolName;
    }
    match std::str::from_utf8(&buffer[offset..offset + cid_len]) {
        Ok(cid) => ConnectValidation::Ok {
            client_id: cid.to_string(),
            keep_alive_secs,
        },
        Err(_) => ConnectValidation::BadProtocolName,
    }
}

/// Determines the total length of a frame (Fixed Header + Varint + Remaining Length).
/// Returns `None` if `buffer` is too short to fully read the Varint.
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

/// Parses an incoming QoS-0 PUBLISH frame and returns `(topic, payload)`.
/// Returns `None` if the frame cannot be parsed (too short, invalid topic
/// length, etc.). Multi-byte Varint is supported up to 4 bytes.
fn parse_publish(buffer: &[u8]) -> Option<(String, Bytes)> {
    if buffer.is_empty() || (buffer[0] & 0xF0) != 0x30 {
        return None;
    }
    // QoS from Fixed Header
    let qos = (buffer[0] >> 1) & 0x03;
    if qos != 0 {
        // MAX_SUPPORTED_QOS = 0 — everything else is dropped in MVP.
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

    // MQTT 3.1.1 §4.7.1.1: Topic Names MUST NOT contain wildcards.
    if topic.contains('+') || topic.contains('#') {
        return None;
    }

    let payload = Bytes::copy_from_slice(&buffer[offset..body_end]);
    Some((topic.to_string(), payload))
}

/// Serialises an outgoing QoS-0 PUBLISH frame without DUP/RETAIN flags.
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

/// Parses the topic filter list from an UNSUBSCRIBE frame.
/// Returns an empty list if the frame is too short or malformed.
fn parse_unsubscribe_filters(buffer: &[u8]) -> Vec<String> {
    // Minimum: fixed header (1) + varint (1) + packet id (2) + min filter (3)
    if buffer.len() < 7 {
        return Vec::new();
    }
    // Skip Varint
    let mut offset = 1usize;
    let mut multiplier: usize = 1;
    let mut remaining: usize = 0;
    for _ in 0..4 {
        if offset >= buffer.len() {
            return Vec::new();
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
    if body_end > buffer.len() || offset + 2 > body_end {
        return Vec::new();
    }
    // Skip Packet ID
    offset += 2;

    let mut filters = Vec::new();
    while offset + 2 <= body_end {
        let flen = ((buffer[offset] as usize) << 8) | (buffer[offset + 1] as usize);
        offset += 2;
        if offset + flen > body_end {
            break;
        }
        if let Ok(f) = std::str::from_utf8(&buffer[offset..offset + flen]) {
            filters.push(f.to_string());
        }
        offset += flen;
    }
    filters
}

/// Writes an MQTT Variable Byte Integer (Remaining Length) to the end of `out`.
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
