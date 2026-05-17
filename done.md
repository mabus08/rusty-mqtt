# Implementierte Features rusty-mqtt

## Projektziel
- MQTT Broker als TCP Server über `tokio` (Netzwerkkommunikation: TCP)
- Ziel: MQTT Connect/Disconnect Pakete empfangen und verarbeiten
- Edition 2024, Rust stable

## Architektur & Implementation

### Core Library (`src/lib.rs`)
- **Paketentypen**: enum `MqttPacket` mit drei Varianten
  - `Connect`: MQTT Connect (Typ 1)
  - `Disconnect`: MQTT Disconnect (Typ 14)
  - `Unknown`: Alle anderen Pakete
  
- **Server-Struktur**: `MqttServer` mit `address` String-Feld
  - Builder-Pattern über `new(addr: &str)`
  
- **Lifecycle**:
  - `run()`: akzeptiert TCP-Verbindungen, spawnet pro Client async Task
  - `handle_connection()`: verarbeitet einzelne Verbindung
  
- **Konnektorlogik**:
  - Puffer-Lesung mit 5 Sekunden Timeout (falls leer → Connection accepted)
  - 0 Bytes read → discontinue ohne Fehler
  - Paket-Parsing → Control Packet Type identifizieren
  
- **Response Logik**:
  - Bei Connect → CONNACK gesendet ([0x20, 0x02, 0x00, 0x00])
    - Flags: None (0x00)
    - Return Code: Success (0x00)
  - `parse_packet()`: Extrahiert Control Packet Type aus MSB

### Entry Point (`src/main.rs`)
- Tokio-main mit async fn main()
- Instanziierung von `MqttServer` für `127.0.0.1:1883` (Standard MQTT Port)
- Blockierende Ausführung via `run().await?`

## Tests (`tests/connection_test.rs`)
- `test_external_connection()`: Integrationstest
  - Server wird auf Testport 1885 spawnet
  - Client-Verbindung über TcpStream
  - SEND: Disconnect-Paket ([0x10, 0x02, 0x00, 0x00])
  - Expectation: Antwortpaket mit CONNACK Typ (0x20)

## Dependencies (`Cargo.toml`)
- `tokio = "1"` mit "full" Feature Set
- `bytes = "1.5"` für Netzwerkpaket-Handling
  [agent]

