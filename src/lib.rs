use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{timeout, Duration};
use std::error::Error;

#[derive(Debug, PartialEq)]
pub enum MqttPacket {
    Connect,
    Disconnect,
    Unknown,
}

pub struct MqttServer {
    address: String,
}

impl MqttServer {
    pub fn new(addr: &str) -> Self {
        Self {
            address: addr.to_string(),
        }
    }

    pub async fn run(&self) -> Result<(), Box<dyn Error>> {
        let listener = TcpListener::bind(&self.address).await?;
        let actual_addr = listener.local_addr()?;
        println!("MQTT Broker horcht auf: {}", actual_addr);

        loop {
            let (socket, _) = listener.accept().await?;
            tokio::spawn(async move {
                if let Err(e) = Self::handle_connection(socket).await {
                    eprintln!("Fehler in Client-Verbindung: {}", e);
                }
            });
        }
    }

    async fn handle_connection(mut socket: TcpStream) -> Result<(), Box<dyn Error>> {
        let mut buffer = [0u8; 1024];
        let timeout_duration = Duration::from_secs(5);
        let bytes_read = match timeout(timeout_duration, socket.read(&mut buffer)).await {
            Ok(result) => result?,
            Err(_) => {
                println!("😒 Client Timeout");
                return Ok(()); // terminate connection
            }
        };

        if bytes_read == 0 {
            return Ok(());
        }
        let packet_type = Self::parse_packet(&buffer[..bytes_read]);

        if packet_type == MqttPacket::Connect {
            // MQTT CONNACK: [Fixed Header, Length, Flags, Return Code]
            // 0x20 = CONNACK, 0x02 = 2 Bytes folgen, 0x00 = No Flags, 0x00 = Connection Accepted
            let connack = [0x20, 0x02, 0x00, 0x00];
            socket.write_all(&connack).await?;
            println!("👍 Connection succesfull");
        }

        Ok(())
    }

    pub fn parse_packet(buffer: &[u8]) -> MqttPacket {
        if buffer.is_empty() {
            return MqttPacket::Unknown;
        }

        let control_packet_type = buffer[0] >> 4;
        match control_packet_type {
            1 => MqttPacket::Connect,
            14 => MqttPacket::Disconnect,
            _ => MqttPacket::Unknown,
        }
    }
}
