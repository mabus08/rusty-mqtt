# Implemented Features — rusty-mqtt

## Project Goal
- MQTT broker as a TCP server using `tokio` (network communication: TCP)
- Goal: receive and process MQTT Connect/Disconnect packets
- Edition 2024, Rust stable

## Architecture & Implementation

### Core Library (`src/lib.rs`)
- **Packet types**: enum `MqttPacket` with three variants
  - `Connect`: MQTT Connect (type 1)
  - `Disconnect`: MQTT Disconnect (type 14)
  - `Unknown`: all other packets

- **Server struct**: `MqttServer` with `address` String field
  - Builder pattern via `new(addr: &str)`

- **Lifecycle**:
  - `run()`: accepts TCP connections, spawns an async task per client
  - `handle_connection()`: processes a single connection

- **Connection logic**:
  - Buffer read with 5-second timeout (if empty → connection accepted)
  - 0 bytes read → close without error
  - Packet parsing → identify Control Packet Type

- **Response logic**:
  - On Connect → CONNACK sent ([0x20, 0x02, 0x00, 0x00])
    - Flags: None (0x00)
    - Return Code: Success (0x00)
  - `parse_packet()`: extracts Control Packet Type from MSB

### Entry Point (`src/main.rs`)
- Tokio main with `async fn main()`
- Instantiation of `MqttServer` for `127.0.0.1:1883` (standard MQTT port)
- Blocking execution via `run().await?`

## Tests (`tests/connection_test.rs`)
- `test_external_connection()`: integration test
  - Server spawned on test port 1885
  - Client connection via `TcpStream`
  - SEND: Disconnect packet ([0x10, 0x02, 0x00, 0x00])
  - Expectation: response packet with CONNACK type (0x20)

## Dependencies (`Cargo.toml`)
- `tokio = "1"` with "full" feature set
- `bytes = "1.5"` for network packet handling
  [agent]
