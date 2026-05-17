use std::io;

#[derive(Debug, Clone, PartialEq)]
pub enum QoS {
    AtMostOnce = 0,
    AtLeastOnce = 1,
}

impl Default for QoS {
    fn default() -> Self {
        QoS::AtMostOnce
    }
}

#[derive(Debug)]
pub struct TopicFilter {
    pub topic: String,
    pub qos: QoS,
}

#[derive(Debug)]
pub struct SubscribePacket {
    pub packet_name: u8,
    pub remaining_length: u16,
    pub packet_identifier: u16,
    pub topic_filters: Vec<TopicFilter>,
}

#[derive(Debug)]
pub enum SubcribeParseError {
    #[error("Invalid packet length: {0}")]
    InvalidLength(u16),

    #[error("Expected SUBSCRIBE packet name 0x82, got 0x{:02X}", .0)]
    UnexpectedPacketType(u8),

    #[error("Empty topic filter")]
    EmptyTopicFilter,
}

pub type SubscribeParseResult<T> = Result<T, SubcribeParseError>;

/// Parsen eines SUBSCRIBE Paket aus dem Buffer.
/// 
/// # Returns
/// - `packet_identifier`: 2 Byte aus Variable Header extrahiert  
/// - `topic_filters`: Vec<(String, QoS)>
pub async fn parse_subscribe_packet(
    packet_name: u8,
    buffer: &[u8],
) -> SubscribeParseResult<(u16, Vec<TopicFilter>)> {
    if packet_name != 0x82 {
        return Err(SubcribeParseError::UnexpectedPacketType(packet_name));
    }

    // Variable Header Parsing (mindestens 2 Byte Packet Identifier + Topic Filters)  
    let mut idx = 1;

    // Packet Identifier (2 Bytes bei SUBSCRIBE)  
    if buffer.len() < idx + 2 {
        return Err(SubcribeParseError::InvalidLength(0));
    }

    let packet_identifier = u16::from_be_bytes([buffer[idx], buffer[idx + 1]]);
    idx += 3; // Packet Identifier (2 bytes) + Reserved Byte (1 byte, always 0x00)

    // Topic Filters Parsing  
    let mut topic_filters: Vec<TopicFilter> = Vec::new();

    while idx < buffer.len() {
        // Topic Filter string length
        if buffer[idx] == 0 {
            break;
        }

        let filter_len = u16::from(buffer[idx]);
        idx += 2;

        if idx + filter_len as usize > buffer.len() {
            return Err(SubcribeParseError::InvalidLength(filter_len));
        }

        // Topic Filter String  
        let topic = parse_string(buffer, idx)?;
        idx += filter_len as usize;

        // QoS Byte (1 byte)  
        if idx >= buffer.len() {
            break;
        }

        let qos_raw = buffer[idx];

        match qos_raw {
            0 => topic_filters.push(TopicFilter {
                topic,
                qos: QoS::AtMostOnce,
            }),
            1 => topic_filters.push(TopicFilter {
                topic,
                qos: QoS::AtLeastOnce,
            }),
            _ => {
                // Für MVP ignorieren wir unbekannte QoS Werte  
                let qos = QoS::AtMostOnce;
                topic_filters.push(TopicFilter {
                    topic,
                    qos,
                });
            }
        }
    }

    Ok((packet_identifier, topic_filters))
}

/// Parst einen String aus dem MQTT Buffer mit der Länge vorangehend.  
pub fn parse_string(buffer: &[u8], start_idx: usize) -> io::Result<String> {
    if buffer.len() < start_idx + 3 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Buffer too small"));
    }

    let len = u16::from_be_bytes([buffer[start_idx], buffer[start_idx + 1]]) as usize;
    let end = start_idx + 3 + len;

    if end > buffer.len() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid topic length"));
    }

    let bytes = &buffer[start_idx + 3..end];
    String::from_utf8_lossy(bytes).to_string()
}

/// Extrahiert das Packet Identifier aus dem ersten Byte des Pakets.  
pub fn extract_packet_id(buffer: &[u8]) -> Option<u16> {
    if buffer.len() >= 3 {
        Some(u16::from_be_bytes([buffer[1], buffer[2]]))
    } else {
        None
    }
}

/// Generiert ein SUBACK Response Packet.  
/// 
/// # Format:  
/// - Fixed Header: 0x90 + Remaining Length  
/// - Variable Header: Packet Identifier (2 Bytes) + Type (1 Byte = SUBACK = 0x02)  
/// - Payload: Return Codes Liste (1 Byte pro Topic Filter)  
pub fn generate_suback(packet_identifier: u16, return_codes: &[QoS]) -> Vec<u8> {
    // Fixed Header  
    let mut fixed_header = [0u8; 5];
    fixed_header[0] = 0x90; // SUBACK Fixed Header Type

    // Remaining Length (Variable Header + Payload)  
    let remaining_length = (2 /* Packet Identifier */) as u16
        + 1 /* Type Byte */
        + return_codes.len() as u16;

    let mut len_bytes = Vec::new();
    encode_remaining_length(&mut len_bytes, remaining_length);

    fixed_header[1] = len_bytes[0];
    if !len_bytes.is_empty() && len_bytes.len() > 1 {
        fixed_header[2..].extend_from_slice(&len_bytes[1..]);
    } else if len_bytes.is_empty() {
        fixed_header.remove(1);
    }

    // Variable Header: Packet Identifier + SUBACK type  
    let mut header_payload = vec![0u8; 3];
    header_payload[0] = ((packet_identifier & 0xFF00) >> 8) as u8;
    header_payload[1] = (packet_identifier & 0xFF) as u8;
    fixed_header[4 + len_bytes.len()..].copy_from_slice(&header_payload[..]);

    // Payload: Return Codes Liste  
    let mut response = Vec::new();
    response.extend_from_slice(&fixed_header[..]);

    for return_code in return_codes {
        match return_code {
            QoS::AtMostOnce => response.push(0x80), // Success
            _ => response.push(0x90), // Success for AtLeastOnce (retained msg, falls zutreffend)
        }
    }

    response
}

/// Parse ein SUBSCRIBE Paket aus dem MQTT Binary Buffer.
/// 
/// # Arguments:
/// - `buffer`: Der komplette Puffer mit Fixed Header + Variable Header + Payload
/// 
/// # Returns
/// - Ok((packet_id, topic_filters)) falls erfolgreich geparsed
/// - Err mit spezifischem Fehlerfall  
pub fn subscribe_from_buffer(buffer: &[u8]) -> SubscribeParseResult<(u16, Vec<TopicFilter>)> {
    if buffer.is_empty() || buffer.len() < 2 {
        return Err(SubcribeParseError::InvalidLength(0));
    }

    let packet_type = buffer[0] >> 4;
    let remaining_length = (buffer[0] & 0x0F) as u16;

    match packet_type {
        5 => { // CONNACK - ignoriert  
            Ok((0u16, Vec::new()))
        }
        82 => { // SUBSCRIBE (Packet Name: 0x82)
            parse_subscribe_packet(0x82, buffer)
        }
        _ => {
            Err(SubcribeParseError::UnexpectedPacketType(buffer[0]))
        }
    }
}

/// Kodiert den MQTT Remaining Length in variabler Länge  
pub fn encode_remaining_length(buf: &mut Vec<u8>, mut length: u16) {
    let mut len_bytes = Vec::new();

    if length == 0 {
        len_bytes.push(0);
    } else if length < 128 {
        len_bytes.push(length as u8);
    } else if length < 16384 {
        len_bytes.push(0x80 | ((length >> 8) as u8));
        len_bytes.push((length & 0xFF) as u8);
    } else {
        // Für MVP reicht dieser Fall nicht - wir erwarten kleine Nachrichten
        panic!("Packet zu groß für MVP");
    }

    buf.extend_from_slice(&len_bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_suback_basic() {
        let packet_id: u16 = 0x3039; // 12345
        let codes = vec![QoS::AtMostOnce, QoS::AtMostOnce]; // Success für beide

        let response = generate_suback(packet_id, &codes);

        assert_eq!(response[0], 0x90);         // PkType = SUBACK  
        assert_eq!(response.len(), 7);         // Fixed Header (5) + Packet ID (2)

        // Extract packet identifier from response
        let resp_pid_u16: u16 = ((response[2] as u16) << 8) | (response[3] as u16);
        assert_eq!(resp_pid_u16, packet_id);   // Packet ID korrekt

        // Return Codes in payload
        assert_eq!(response[5..], b"\x80\x80");  // Both successful  
    }

    #[test]
    fn test_generate_suback_single() {
        let packet_id: u16 = 1;
        let codes = vec![QoS::AtLeastOnce];

        let response = generate_suback(packet_id, &codes);

        assert_eq!(response[0], 0x90);         // Fixed Header  
        assert!(!response.is_empty());
    }

    #[test]
    fn test_parse_string() {
        let topic = b"/home/user123";
        let len_u8 = topic.len() as u8;
        
        // Format: [Len Hi][Len Lo][0x01][topic...] für Testzwecke  
        // Real MQTT uses [LenHi][LenLo][StringChars...] for SUBSCRIBE payload
        
        // Einfacher String Test: 
        let simple_len = 5u16;
        let buffer = b"foo";       // 3 chars
        let result = parse_string(buffer, 0);
        
        assert_eq!(result.unwrap(), "foo");    // Works for basic string
    }

    #[test]
    fn test_extract_packet_id() {
        let header_bytes = [0x82, 0x30, 0x39]; // SUBSCRIBE + PID=12345
        
        let result = extract_packet_id(&header_bytes[1..]);
        assert_eq!(result, Some(0x3039));      // Packet ID extracted correct
    }

    #[test]
    fn test_encodes_remaining_length() {
        let mut buf = Vec::new();
        encode_remaining_length(&mut buf, 6);
        assert_eq!(buf, vec![0x06]);           // Length fits in 1 byte

        let mut buf = Vec::new();
        encode_remaining_length(&mut buf, 200);
        assert_eq!(buf.len(), 2);               // Length needs 2 bytes
    }

    #[test]
    fn test_qos_default() {
        assert_eq!(QoS::default(), QoS::AtMostOnce);  // Default is QoS=0
    }


}
}
