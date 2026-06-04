# MQTT Broker — Implementierungs-Roadmap

Architekturentscheidungen siehe `docs/adr/`. Begriffe siehe `CONTEXT.md`.

---

## Phase 1 — Foundation

- [ ] `thiserror`-Dep hinzufügen, `MqttError`-Enum anlegen
- [ ] `ConfigError` (lib.rs:18) auf `thiserror` umstellen
- [ ] `Box<dyn Error>` und `String`-Errors in Public-API durch `MqttError` ersetzen
- [ ] `tracing` + `tracing-subscriber`-Dep, in `main.rs` initialisieren
- [ ] alle `println!`/`eprintln!` durch `tracing::{info,warn,error,debug}` ersetzen
- [ ] `tokio-util` mit Feature `codec` hinzufügen
- [ ] `MqttCodec`-Skeleton in `src/codec.rs` (Decoder + Encoder Traits, leere Match-Arme)
- [ ] `MqttPacket`-Enum auf strukturierte Varianten umstellen:
  - `Connect { client_id, keep_alive, clean_session }`
  - `ConnAck { session_present, return_code }`
  - `Publish { topic, payload: Bytes, qos: u8 }`
  - `Subscribe { packet_id, filters: Vec<(String, u8)> }`
  - `SubAck { packet_id, granted: Vec<u8> }`
  - `Unsubscribe { packet_id, filters: Vec<String> }`
  - `UnsubAck { packet_id }`
  - `PingReq`, `PingResp`, `Disconnect`
- [ ] CONNECT-Decoder mit voller Validierung:
  - Protocol Name ≠ "MQTT" → `MalformedFrame`, Socket schließen ohne CONNACK
  - Protocol Level ≠ 4 → CONNACK `0x01`, dann schließen
  - Reserved Bit ≠ 0 → `MalformedFrame`
  - Leere Client ID → CONNACK `0x02`
  - Keep-Alive aus Variable Header lesen
- [ ] CONNACK-Encoder (SessionPresent immer 0 für MVP)
- [ ] Varint-Decoder für Remaining Length (bis 4 Bytes)

## Phase 2 — Connection Lifecycle

- [ ] `ConnectionCommand`-Enum (`DeliverFrame(Bytes)`, `Disconnect`)
- [ ] `ClientRegistry`-Struct mit `HashMap<ClientId, mpsc::Sender<ConnectionCommand>>`
- [ ] `const SUBSCRIBER_CHANNEL_CAPACITY: usize = 32;`
- [ ] `MqttServer` um `Arc<ClientRegistry>` erweitern
- [ ] Connection Task auf `Framed`-basierten Loop umbauen
- [ ] `select!` mit drei Armen: `framed.next()`, `rx.recv()`, `keep_alive_sleep`
- [ ] Keep-Alive-Logik:
  - Deadline = 1.5 × `keep_alive` aus CONNECT
  - Bei jedem Frame: `sleep.as_mut().reset(...)`
  - `keep_alive == 0` → keep_alive deaktiviert (TODO: später Safety-Net)
- [ ] Takeover-Protokoll bei Client-ID-Kollision:
  1. `swap_in(client_id, new_tx)` → `Option<old_tx>`
  2. Falls vorhanden: `old_tx.send(Disconnect).await; old_tx.closed().await;`
  3. Erst danach eigene Subscriptions in den Router eintragen
- [ ] Cleanup-Pfad: bei Task-Ende `router.remove_client` + `registry.remove` (einzige Stelle, die diese aufruft)
- [ ] SUBSCRIBE: Codec-basiert parsen, `MAX_SUPPORTED_QOS = 0` cappen, SUBACK encoden
- [ ] `Subscriber.qos` → `Subscriber.granted_qos` umbenennen
- [ ] UNSUBSCRIBE-Handler + `TopicRouter::unsubscribe(client_id, &[filter])` + UNSUBACK
- [ ] PINGREQ → PINGRESP über Encoder
- [ ] DISCONNECT bleibt: bricht Loop, regulärer Cleanup

## Phase 3 — Das eigentliche Feature: PUBLISH-Routing

- [ ] PUBLISH-Decoder:
  - Varint-Length parsen
  - Topic, Payload als `Bytes` extrahieren
  - QoS-Bits aus Fixed Header (QoS > 0 → verwerfen + warnen)
  - Wildcards `+`/`#` im Topic → Frame verwerfen + warnen
  - DUP/RETAIN-Bits ignorieren
- [ ] `Subscriber` um `tx: mpsc::Sender<ConnectionCommand>` erweitern
- [ ] `TopicRouter::subscribe`-Signatur erweitert um `tx`
- [ ] Fanout-Logik im Connection Task: bei PUBLISH inbound → `router.get_subscribers_for_topic` → für jeden Subscriber `tx.try_send(DeliverFrame(bytes))` (drop on full)
- [ ] PUBLISH-Encoder (outbound an Subscriber)
- [ ] Integrationstest end-to-end: zwei Mosquitto-Clients, einer pubt, einer subt mit Wildcard, Payload wird durchgereicht

## Phase 4 — Aufräumen & Definition of Done

- [ ] `SubscribeHandler`-Struct (mod.rs:11) löschen, freie Funktionen in Codec/Router verschieben
- [ ] `parse_packet`, `parse_connect_client_id`, `handle_subscribe_impl`, `extract_packet_id` aus `MqttServer` entfernen (in Codec aufgegangen)
- [ ] `///` Doc-Comments auf allen `pub` Items (AGENT.md §5)
- [ ] `cargo fmt --check`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test` — alle bestehenden 13 Tests + neue Codec/Fanout/Takeover-Tests grün

---

## Bewusst out-of-scope (nicht im MVP)

- Retained Messages
- Last Will / Will-Message
- QoS 1/2 (`MAX_SUPPORTED_QOS` von 0 hochsetzen, PUBACK/PUBREC/PUBREL/PUBCOMP, Inflight-Tracking)
- Persistente Sessions (Clean-Session-Resume)
- Auth (Username/Password aus CONNECT-Flags)
- CLI-Args (`--port`, `--host`, `--verbose`)
- Max-Connections-Limit, Max-Payload-Size, Graceful Shutdown
- Observability (`/stats`, `/broker`-Topic, Throughput-Logging)
- Keep-Alive Safety-Net (Broker-Maximalwert bei `keep_alive=0`)
