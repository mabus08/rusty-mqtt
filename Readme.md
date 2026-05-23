# rusty-mqtt

A small, lightweight MQTT broker written in Rust using Tokio.

## Current Features

### ✅ Implemented

- **MQTT 3.1.1 Protocol Support**
  - CONNECT packet parsing with client ID extraction
  - CONNACK response generation
  - PING/PINGRESP packet handling with configurable keepalive timeouts
  
- **SUBSCRIBE Protocol** (MVP - QoS 0 & 1 only)
  - Parse SUBSCRIBE packets (Control Type 0x82)
  - Topic filter matching with wildcard support:
    - `+` wildcard: matches exactly one topic level (e.g., `/home/+/status` matches `/home/kitchen/status`)
    - `#` wildcard: matches zero or more remaining levels (e.g., `/news/#` matches `/news/tech/ai/latest`)
    - Exact topic matching
  - SUBACK response generation with correct granted QoS per subscription
  - Subscriber storage with replace semantics (duplicate subscription updates QoS)
  - Client cleanup on disconnect/timeout
  
- **Configuration Management**
  - TOML-based configuration file support (`broker.toml`)
  - Configurable host/port (defaults: `127.0.0.1:1883`)
  - Configurable timeouts (keep-alive)

- **Async I/O**
  - Built on Tokio for concurrent client handling
  - Non-blocking TCP connection handling

## Getting Started

### Prerequisites
- Rust 1.70+ (install via [rustup](https://rustup.rs/))
- Cargo (comes with Rust)

### Installation

```bash
git clone https://github.com/yourusername/rusty-mqtt.git
cd rusty-mqtt
```

### Running the Broker

#### Option 1: Default Configuration (localhost:1883)
```bash
cargo run
```

#### Option 2: Custom Configuration File
Create a `broker.toml` in the project root:
```toml
[mqtt]
host = "0.0.0.0"
port = 1883
keep_alive_timeout_secs = 60
```

Then run:
```bash
cargo run
```

#### Option 3: Run as Release Build (Optimized)
```bash
cargo run --release
```

### Testing the Broker

Run all tests:
```bash
cargo test
```

Run with output:
```bash
cargo test -- --nocapture
```

Test coverage includes:
- MQTT packet parsing (CONNECT, SUBSCRIBE)
- Topic wildcard matching (`+`, `#`, exact matches)
- SUBACK response generation
- Subscriber storage and client ID parsing
- Configuration file loading
- Concurrent client connections

## Architecture

### Module Structure

```
src/
├── main.rs                      # Entry point, broker startup
├── lib.rs                       # Core MqttServer, connection handling, CONNECT parsing
├── config.rs                    # TOML configuration parser
└── subscribe_handlers/
    ├── mod.rs                   # SubscribeHandler, SUBACK generation
    └── storage.rs               # TopicRouter, Subscriber storage, wildcard matching
```

### Key Data Structures

**TopicRouter** (`subscribe_handlers/storage.rs`)
- Stores subscriptions: `HashMap<String, Vec<Subscriber>>`
- Key: Topic Filter (e.g., `/home/+/status`)
- Value: List of subscribers (client_id + QoS)
- Implements topic matching and subscriber lookup

**MqttServer** (`lib.rs`)
- Shared state: `Arc<Mutex<TopicRouter>>`
- Handles CONNECT/SUBSCRIBE/PING packets
- Manages client connections with timeouts

## Example: Connecting with MQTT Client

Using `mosquitto_sub` (install: `apt install mosquitto-clients` or `brew install mosquitto`):

```bash
# Terminal 1: Subscribe to a topic
mosquitto_sub -h 127.0.0.1 -p 1883 -t "home/+/temperature"

# Terminal 2: (Future) Publish a message
mosquitto_pub -h 127.0.0.1 -p 1883 -t "home/kitchen/temperature" -m "22.5"
```

## Planned Features (Not Yet Implemented)

- [ ] PUBLISH protocol and message routing to subscribers
- [ ] UNSUBSCRIBE packet handling
- [ ] QoS 2 support (Exactly Once delivery)
- [ ] Retained messages
- [ ] Last Will and Testament (LWT)
- [ ] Client connection metrics / introspection API
- [ ] Persistent message queuing
- [ ] TLS/SSL support

## Performance Notes

- **Concurrency**: Tokio async for handling multiple simultaneous connections
- **Memory**: No retained messages in MVP (minimal memory footprint)
- **Storage**: In-memory only (not persisted across restarts)

## Building & Development

### Build Debug
```bash
cargo build
```

### Build Release (Optimized)
```bash
cargo build --release
```

### Code Quality
```bash
cargo clippy
cargo fmt
```

### Debug Single Test
```bash
cargo test test_name -- --exact --nocapture
```

## License

[Specify your license here]

## Contributing

Issues and pull requests are welcome. For major changes, please open an issue first to discuss what you would like to change.

## Troubleshooting

**Error: Address already in use (port 1883)**
- Change port in `broker.toml` to an available port (e.g., 8883)
- Or kill existing process: `lsof -i :1883 | grep LISTEN | awk '{print $2}' | xargs kill -9`

**Error: Failed to parse TOML config**
- Check syntax in `broker.toml` (valid TOML format required)
- Ensure `[mqtt]` section exists
- Verify `host` and `port` values are strings and numbers respectively

**Client connects but doesn't receive SUBACK**
- Broker is running and accepting connections ✓
- SUBSCRIBE parsing is implemented ✓
- Verify topic filter is valid (no empty strings, valid wildcards)
