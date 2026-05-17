# MQTT Subscription Handler - Anforderungen

## Ziel
Implementierung eines simplen SUBSCRIBE Handlers für den MQTT Broker MVP,
der SubSCRIBE-Pakete akzeptiert und corresponding SUBACK Responses sendet.

---

## Funktionalitäten (MVP Scope)

### 1. SUBSCRIBE Packet Parsing
- **Input**: MQTT SUBSCRIBE Paket (Control Packet Type: 0x82/130)
- **Parsing**:
  - Variable Header extrahieren (Packet Name, Identifier, Topic Filter List)
  - Topic Filters lesen (String + QoS Bytes: 0x00/1 für QoS 0/1)
  - Payload optional mit Retain Flags einlesen (für zukünftige Features)

### 2. Subscriber Storage
- **Datastruktur**: 
  - `Arc<Mutex<HashMap<String, Vec<QosSubscriber>>>>` globaler storage
  - Key: Topic Filter String
  - Value: Liste der Subscriber mit QoS-Level und Client Identifier

- **Subscriber Record**:
  ```rust
  struct QosSubscriber {
      packet_id: u16,          // Original SUBSCRIBE Packet Identifier (2 bytes)
      qos: QoS,                // 0 oder 1 (für MVP)
      client_id: String,       // Connect Client ID
      topic_filters: Vec<String>, // optional für Routing Info
  }
  ```

### 3. Topic Filter Matching (Grundlegend)
- **Supported**: Einfache String-Matches + Wildcards
  - `+`: Matches genau ein Topic-Level (`/home/user` matches `/home/user123`)
  - `#`: Matches alle Levels ab dem Match-Punkt (`/# matches /a/b/c/d/e`)
  
- **Matching Logik** (MVP):
  ```rust
  pub fn topic_matches(filter: &str, msg_topic: &str) -> bool {
      match filter {
          "#" => true,                           // Matches alles
          f if f.ends_with('+' || '#') => 
              wildcard_match(f, msg_topic),     // Wildcard Logik
          _ => msg_topic == filter,             // Exakte Match
      }
  }
  ```

### 4. SUBACK Response Generation
- **Response Format** (Nach MQTT Spec):
  ```
  Fixed Header: 0x90 + Remaining Length
  Variable Header:
    Type: 0x02 (SUBACK)
    Packet Identifier: Same as SUBSCRIBE
  
  Payload:
    Return Codes Liste (jeweils 1 Byte):
      - 0x80 (Success, QoS = 0, wenn SUBSCRIBE QoS=0)
      - 0x90 (Success for topics with retained flag, falls zutreffend)
  ```

- **Response Logik**:
  ```rust
  pub fn generate_suback(&self, granted_qos_list: &[QoS]) -> Vec<u8> {
      // Pack SUBACK variable header + payload mit return codes
  }
  ```

### 5. Client-Spezifische Storage Organization
- Jeder connected client hat eigenen subscriber store:
  ```rust
  pub struct ClientSubscriptions {
      subscriptions: HashMap<u16, Vec<(String, QoS)>>, // packet_id -> [(topic, qos)]
  }
  ```

---

## API Design (High-Level)

### MqttServer Methods (erweiternd)

```rust
impl MqttServer {
    /// Verarbeitet ein SUBSCRIBE Paket von einem client
    async fn handle_subscribe(
        &self, 
        packet_id: u16, 
        topic_filters: Vec<(String, QoS)> // (topic_filter, qos_level)
    ) -> Result<Vec<u8>, MqttError> {
        // 1. Subscriber speichern (global oder client-local?)
        // 2. SUBACK Response generieren
        // 3. Response in TcpStream senden
    }

    /// Prüft ob ein topic filter mit message topic matches
    fn match_topic(&self, filter: &str, topic: &str) -> bool;

    /// Extrahiert Subscription Records für eine publish message
    async fn get_subscribers_for_topic(
        &self, 
        topic: &str
    ) -> Vec<u16>; // list of packet_ids
}
```

---

## Performance Constraints (MVP)

- **Memory**: Keine Retained Messages (vereinfacht MVP)
- **Max Payload Size**: 1MB default limit bei SUBSCRIBE topic filters (?)
- **Concurrent Access**: Arc<Mutex<>> für subscriber storage

---

## Error Handling

- **Invalid Topic Filter** (z.B. leer, nur wildcards): return error code oder ignorieren
- **Too Many Topics**: Fallback auf Success für MVP (ohne Hard Limit)
- **Duplicate Subscribe**: Merge/Update oder ignorieren?

---

## MQTT Spec Compliance (MVP Minimum)

1. Variable Header: Packet Identifier (2 bytes) + muss SUBACK gleichen
2. Payload: Return Code pro subscription (mindestens 1 Byte)
3. QoS Level: Nur 0 und 1 für MVP (2 optional, aber ignorieren)
4. Retain Handle: Optional im payload einlesen, aber no retention logic

---

## Tests (Required Before Merge)

1. `test_handle_subscribe()`: Subscribt auf `/test/topic` → SUBACK收到
2. `test_topic_matching()`: Wildcard Matches testen (`+/news/#`)
3. `test_suback_response()`: Return code in packet correct?
4. `test_concurrent_subscribe()`: Mehrere Subscription auf einmal

---

---

## Arbeitspaket 1: Subscribe Handler (Parsing Only)

**Ziel**: SUBSCRIBE Pakete parsieren und SUBACK Response generieren.

### Deliverables:
- Neue Rust Datei: `src/subscribe_handlers.rs`
- Parse SUBSCRIBE packets:
  - Variable Header (Packet Name, Identifier, Topic Filter List)  
  - Subject Filters + QoS Bytes extrahieren
  
### Spezifikation SUBACK response:
- Fixed Header: 0x90 + Remaining Length
- Variable Header: Packet Identifier + Type (SUBACK = 0x02)
- Payload: Return Code Liste pro subscription

```rust
pub async fn parse_subscriber(
    packet_id: u16, 
    topic_filters: Vec<(String, QoS)>
) -> Result<(), MqttError> { /* Parse Logic */ }

pub fn generate_suback(packet_id: u16, return_codes: &[u8]) -> Vec<u8>; /* Response */
```

**Wichtig**: Hier wird **noch kein Storage** implementiert! Nur parsen und response send.

---

## Arbeitspaket 2: Subscriber Storage & Topic Router

**Ziel**: Subskribierte topics persistent speichern und für routing verfügbar machen.

### Deliverables:
- Globaler subscriber storage (Arc<Mutex<HashMap>>): Key=Topic Filter, Value=[Subscribers]
- Topic tree mit wildcard patterns (`+/news/#`)
- Subscription lookup optimization

```rust
struct Subscriber {
    packet_id: u16,          // Original SUBSCRIBE Packet Identifier
    qos: QoS,                // 0 oder 1 (für MVP)
    client_id: String,       // Connect Client ID
    topic_filters: Vec<String>, // gespeichert für routing info
}

pub struct TopicRouter {
    subscriptions: Arc<Mutex<HashMap<String, Vec<Subscriber>>>>,
}

impl TopicRouter {
    pub async fn subscribe(&self, packet_id: u16, 
                          filters: Vec<(String, QoS)>) -> Result<(), MqttError>;
    
    pub fn match_topic(&self, filter: &str, topic: &str) -> bool;
}
```

---

## Arbeitspaket 3: Integration in Broker (lib.rs Einbindung)

**Ziel**: Subscribe Handler und Topic Router in die `MqttServer` Struktur integrieren.

### Deliverables:
- `MqttServer` mit `subscribe_handlers: SubscribeHandler` expandieren
- TCP Verbindung lifecycle: Handle SUBSCRIBE bei Connection accept  
- Packet routing zu internem router implementieren

```rust
pub struct MqttServer {
    address: String,
    subscribe_handler: SubscribeHandler, 
    topic_router: Arc<TopicRouter>,
}

impl MqttServer {
    pub fn new(addr: &str) -> Self;
    
    pub async fn handle_subscribe(&mut self, stream: TcpStream) -> Result<(), MqttError>;
}
```

**Timeline**: Diese Integration erfolgt **nach Abschluss von AP2**.

---

## Arbeitspaket 4: Protocol Enhancements & Error Handling

**Ziel**: MQTT Control Packets außerhalb subscribes/publishes.

### Deliverables:
- PINGRESPONSE Packet senden (Keepalive/Ping timeout cleanup)
- DISCONNECT packet handling + client cleanup
- CONACK Return Codes (graceful rejection logic)
- Clean Session Flag enforcement

---

## Arbeitspaket 5: Testing & Validation

**Ziel**: Integrationstests für subscribe handlers.

### Deliverables (tests):
- `test_handle_subscribe()` - SUBSCRIBE parse und SUBACK send
- `test_topic_matching()` - Wildcard Matches testen (`+/news/#`)
- `test_suback_response()` - Return code in packet correct?
- `test_concurrent_subscribe()` - Mehrere Subscription auf einmal

