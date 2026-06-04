# MQTT Broker — Implementation Roadmap

Architecture decisions: see `docs/adr/`. Terminology: see `CONTEXT.md`.

---

## Phase 1 — Foundation

- [ ] Add `thiserror` dependency, create `MqttError` enum
- [ ] Migrate `ConfigError` (lib.rs:18) to `thiserror`
- [ ] Replace `Box<dyn Error>` and `String` errors in the public API with `MqttError`
- [ ] Add `tracing` + `tracing-subscriber` dependencies, initialise in `main.rs`
- [ ] Replace all `println!`/`eprintln!` with `tracing::{info,warn,error,debug}`
- [ ] Add `tokio-util` with feature `codec`
- [ ] `MqttCodec` skeleton in `src/codec.rs` (Decoder + Encoder traits, empty match arms)
- [ ] Migrate `MqttPacket` enum to structured variants:
  - `Connect { client_id, keep_alive, clean_session }`
  - `ConnAck { session_present, return_code }`
  - `Publish { topic, payload: Bytes, qos: u8 }`
  - `Subscribe { packet_id, filters: Vec<(String, u8)> }`
  - `SubAck { packet_id, granted: Vec<u8> }`
  - `Unsubscribe { packet_id, filters: Vec<String> }`
  - `UnsubAck { packet_id }`
  - `PingReq`, `PingResp`, `Disconnect`
- [ ] CONNECT decoder with full validation:
  - Protocol Name ≠ "MQTT" → `MalformedFrame`, close socket without CONNACK
  - Protocol Level ≠ 4 → CONNACK `0x01`, then close
  - Reserved Bit ≠ 0 → `MalformedFrame`
  - Empty Client ID → CONNACK `0x02`
  - Read Keep-Alive from Variable Header
- [ ] CONNACK encoder (SessionPresent always 0 for MVP)
- [ ] Varint decoder for Remaining Length (up to 4 bytes)

## Phase 2 — Connection Lifecycle

- [ ] `ConnectionCommand` enum (`DeliverFrame(Bytes)`, `Disconnect`)
- [ ] `ClientRegistry` struct with `HashMap<ClientId, mpsc::Sender<ConnectionCommand>>`
- [ ] `const SUBSCRIBER_CHANNEL_CAPACITY: usize = 32;`
- [ ] Extend `MqttServer` with `Arc<ClientRegistry>`
- [ ] Migrate Connection Task to `Framed`-based loop
- [ ] `select!` with three arms: `framed.next()`, `rx.recv()`, `keep_alive_sleep`
- [ ] Keep-Alive logic:
  - Deadline = 1.5 × `keep_alive` from CONNECT
  - On every frame: `sleep.as_mut().reset(...)`
  - `keep_alive == 0` → keep-alive disabled (TODO: safety-net later)
- [ ] Takeover protocol on Client ID collision:
  1. `swap_in(client_id, new_tx)` → `Option<old_tx>`
  2. If present: `old_tx.send(Disconnect).await; old_tx.closed().await;`
  3. Only then register own subscriptions in the router
- [ ] Cleanup path: on task exit call `router.remove_client` + `registry.remove` (single call site)
- [ ] SUBSCRIBE: parse via codec, cap at `MAX_SUPPORTED_QOS = 0`, encode SUBACK
- [ ] Rename `Subscriber.qos` → `Subscriber.granted_qos`
- [ ] UNSUBSCRIBE handler + `TopicRouter::unsubscribe(client_id, &[filter])` + UNSUBACK
- [ ] PINGREQ → PINGRESP via encoder
- [ ] DISCONNECT remains: breaks loop, regular cleanup

## Phase 3 — The Core Feature: PUBLISH Routing

- [ ] PUBLISH decoder:
  - Parse Varint length
  - Extract topic and payload as `Bytes`
  - QoS bits from Fixed Header (QoS > 0 → discard + warn)
  - Wildcards `+`/`#` in topic → discard frame + warn
  - Ignore DUP/RETAIN bits
- [ ] Extend `Subscriber` with `tx: mpsc::Sender<ConnectionCommand>`
- [ ] Extend `TopicRouter::subscribe` signature to include `tx`
- [ ] Fanout logic in Connection Task: on inbound PUBLISH → `router.get_subscribers_for_topic` → for each subscriber `tx.try_send(DeliverFrame(bytes))` (drop on full)
- [ ] PUBLISH encoder (outbound to subscribers)
- [ ] End-to-end integration test: two Mosquitto clients, one publishes, one subscribes with wildcard, payload is forwarded

## Phase 4 — Cleanup & Definition of Done

- [ ] Delete `SubscribeHandler` struct (mod.rs:11), move free functions into Codec/Router
- [ ] Remove `parse_packet`, `parse_connect_client_id`, `handle_subscribe_impl`, `extract_packet_id` from `MqttServer` (absorbed into Codec)
- [ ] `///` doc-comments on all `pub` items (AGENT.md §5)
- [ ] `cargo fmt --check`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test` — all existing 13 tests + new Codec/Fanout/Takeover tests green

---

## Deliberately out of scope (not in MVP)

- Retained Messages
- Last Will / Will-Message
- QoS 1/2 (raise `MAX_SUPPORTED_QOS` from 0, PUBACK/PUBREC/PUBREL/PUBCOMP, inflight tracking)
- Persistent Sessions (Clean-Session resume)
- Auth (Username/Password from CONNECT flags)
- CLI args (`--port`, `--host`, `--verbose`)
- Max-connections limit, max-payload size, graceful shutdown
- Observability (`/stats`, `/broker` topic, throughput logging)
- Keep-Alive safety-net (broker maximum value when `keep_alive=0`)
