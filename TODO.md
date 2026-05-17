# MQTT Broker - Implementierung To-Do Liste

## 🎯 Priorisierung: MVP → Advanced → Polish

---

## 🔴 Kritisch (MVP - Ohne diese Funktion kein MQTT)

### 1. Topic Management
- [ ] **Subscribe Handler**: `SUBACK` response für Subscribe-Messages
  - Topic filter matching (wildcards: `/+/news/#`)
  - Subscriber list pro topic
   
- [ ] **Publish Handler**: `PUBLISH` mit Payload und Packet Handling
  - QoS 0/1/2 Support
  - Message Delivery Tracking
  
- [ ] **Publish zu Topics** (Client-to-Broker)
  - Routing an subscribers
  - Topic-Wildcard-Matching

### 2. MQTT Protocol Essentials  
- [ ] **PingRESPONSE** Handling (Keepalive/Ping timeout cleanup)
- [ ] **Disconnect** packet handling + client cleanup
- [ ] **CONACK** Return Codes (graceful rejection logic)
- [ ] Clean Session Flag enforcement

### 3. Message Lifecycle
- [ ] ~~Retained Messages~~: ✗ verzichtet (vereinfacht MVP)

---

## 🟡 Wichtig (Next Milestone)

### 4. Topic Tree & Filtering
- [ ] Simple Topic Router ("/home", "/news/*")
- [ ] Topic-Wildcard Matching (`+` für einen level, `#` für alle)
- [ ] Subscription Store (topic -> subscribers mapping)

### 5. Connection Pooling
- [ ] Max Connections Limit konfigurieren
- [ ] Graceful Shutdown Logic
- [ ] Arc<Mutex<HashMap>> für subscriber tracking

### 6. Payload Handling
- [ ] Payload Size Limit (~1MB default)
- [ ] Memory-Efficient Buffering (Vec<u8>/bytes)

---

## 🟢 Nice-to-Have (Polish & Ops)

### 7. Broker Metadata
- [ ] `/broker` topic: Health Status API
- [ ] Active Subscriber Count exposement
- [ ] Retained Messages (/retained)

### 8. CLI Konfiguration
- [ ] Port/Address Option (`--port`, `--host`)
- [ ] QoS-Priority Config
- [ ] Max Payload Größe übergeben
- [ ] Verboselogging Flag (`--verbose`)

### 9. Observability
- [ ] Connection Stats (/stats)
- [ ] Topic usage / Subscribent-Zählung
- [ ] Message Durchsatz logging

### 10. Error Handling & Recovery
- [ ] Max retry für PUBLISH (QoS2)
- [ ] Graceful Degradation (Overflow handling)

---

## 🧠 Design Entscheidungen vorab

| Feature | Empfehlung | Begründung |
|--------|-----------|------------|
| **Datastruktur** | `HashMap<Topic, Vec<Subscriber>>` | Einfache Struktur für MVP |
| **Payload Storage** | Heap-Alloc bei publish | MQTT ist message-basiert, nicht streaming |
| **Retained Messages** | Separater HashMap | Performance: O(1) Lookup |
| **Topic Router** | Trie-Baum oder Vec? | Bei >1K Topics: Trie besser |

---

## 📝 Hinweise zur Implementierung

- **MVP**: Nur QoS 0, no retained messages, no last will
- **Ziel Edition**: edition=2021 (nicht 2024) in Cargo.toml prüfen  
- **Tests**: Jeder pub/sub Handler braucht Integrationstest
- **Docs**: `///` Docs für alle pub items hinzufügen
