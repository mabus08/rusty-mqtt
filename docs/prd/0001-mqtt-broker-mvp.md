# PRD 0001 — MQTT 3.1.1 Broker MVP: End-to-End Pub/Sub

**Status:** ready-for-agent
**Glossary:** [`CONTEXT.md`](../../CONTEXT.md)
**Architecture:** [`docs/adr/0002`](../adr/0002-mpsc-fanout-fuer-publish-zustellung.md), [`0003`](../adr/0003-framed-codec-und-typisierte-fehler.md), [`0004`](../adr/0004-clientregistry-und-takeover-protokoll.md)
**Roadmap:** [`TODO.md`](../../TODO.md)

## Problem Statement

As an MQTT client developer I can currently connect to the broker, send SUBSCRIBE packets and perform PING roundtrips — but I **cannot exchange messages between two clients**. A PUBLISH is silently swallowed by the broker and no subscriber ever receives anything. This makes the broker useless for its actual purpose.

In addition there are several silent spec violations that lead to hard-to-diagnose failures in real setups: the Keep-Alive value from CONNECT is ignored (dead clients hang as connected for minutes), two parallel CONNECTs with the same Client ID are both accepted (instead of terminating the old connection), and SUBACK granted QoS values do not reflect what the broker actually delivers.

## Solution

The broker is extended to a fully functional MQTT 3.1.1 MVP for QoS 0. A PUBLISH from one client is delivered to all subscribers whose topic filters (including `+`/`#` wildcards) match the topic. Keep-Alive from CONNECT is respected, Client ID collisions result in a spec-compliant takeover, and SUBACK honestly reflects what the broker can deliver.

Concretely, at the end the user can run a complete pub/sub session with two `mosquitto` clients: a `mosquitto_sub -t 'home/+/temp'` receives every `mosquitto_pub -t 'home/kitchen/temp' -m '21.5'` message, idle clients are cleaned up after their Keep-Alive interval expires, and a second connect with the same `--id` cleanly kills the first session.

## User Stories

1. As an IoT developer I want to send a PUBLISH with a concrete topic to the broker so that all subscribers with a matching topic filter receive the message.
2. As an IoT developer I want to specify a topic filter with a `+` wildcard in SUBSCRIBE so that I can leave exactly one topic level free (e.g. `home/+/temp`).
3. As an IoT developer I want to specify a topic filter with a terminal `#` in SUBSCRIBE so that I can subscribe to any number of subtopics at once (e.g. `sensors/#`).
4. As an IoT developer I want to send payloads of arbitrary size (within the Varint limit) so that JSON sensor data of a few KB is forwarded cleanly.
5. As an IoT developer I want my subscriber to receive binary payloads byte-for-byte so that I have no encoding losses.
6. As an IoT developer I want a PUBLISH with wildcards in the topic to be rejected by the broker so that the spec guarantees for topic names are preserved.
7. As an IoT developer I want my PUBLISH with QoS > 0 to be silently dropped for now (with a log warning) so that the broker remains stable until QoS 1/2 is supported.
8. As an IoT developer I want to be able to send a SUBSCRIBE with requested QoS 1 or 2 and receive a SUBACK with granted QoS 0 so that my client library can downgrade in a spec-compliant way.
9. As an IoT developer I want multiple subscribers to be subscribed to the same topic simultaneously and all receive the message so that fan-out works.
10. As an IoT developer I want to be able to send an UNSUBSCRIBE and afterwards receive no more PUBLISH messages for those filters so that my client can unsubscribe from topics selectively.
11. As an IoT developer I want my UNSUBSCRIBE to be acknowledged with an UNSUBACK carrying the same Packet ID so that my client library can match the acknowledgement.
12. As an IoT developer I want my client to transmit a Keep-Alive value in CONNECT and for the broker to consider me dead only after 1.5× that interval so that I can determine the heartbeat frequency myself.
13. As an IoT developer I want a PINGREQ within the Keep-Alive interval to reset the read timer at the broker so that my long-lived subscriber stays online.
14. As an IoT developer I want any control packet (not just PINGREQ) to reset the Keep-Alive timer so that active publishers do not need to send additional pings.
15. As an IoT developer I want to be able to set `keep_alive=0` so that my client can work without a heartbeat obligation.
16. As an IoT developer I want a second CONNECT with the same Client ID to cleanly terminate the old connection before sending CONNACK on the new connection so that there are never two active sessions with the same ID.
17. As an IoT developer I want the subscriptions of the old session to be fully cleaned up before the new session registers its own so that no cleanup race accidentally deletes my new subscriptions.
18. As an IoT developer I want my CONNECT with a wrong Protocol Name to be rejected by the broker without CONNACK so that my client quickly recognises it is not speaking MQTT 3.1.1.
19. As an IoT developer I want a CONNECT with Protocol Level ≠ 4 to receive a CONNACK with Return Code 0x01 (Unacceptable Protocol Version) and for the connection to be closed afterwards so that my client has a clear diagnostic.
20. As an IoT developer I want a CONNECT with an empty Client ID to be rejected with CONNACK Return Code 0x02 (Identifier Rejected) so that anonymous connects do not silently succeed.
21. As an IoT developer I want a CONNECT with the Reserved Bit set to be discarded so that spec violations are visible early.
22. As an IoT developer I want my DISCONNECT to be treated by the broker as a regular conclusion (no error log) so that my test suites have clean outputs.
23. As an IoT developer I want both the ClientRegistry and all TopicRouter entries for my client to be removed after a DISCONNECT or read timeout so that I do not inherit ghost state after reconnecting.
24. As an IoT developer I want my Clean-Session flag in CONNECT to be read and SessionPresent in CONNACK to be consistently set to 0 so that my client knows no session is being resumed.
25. As a broker operator I want all connection lifecycle events (Connect, Subscribe, Publish, Disconnect, Kick) to be logged in structured form via `tracing` so that I can trace problems.
26. As a broker operator I want `println!` to be used nowhere in library code so that logging is uniformly configurable.
27. As a broker operator I want slow subscribers to lose their own frames rather than blocking the publisher so that a single broken client cannot bring down the broker.
28. As a broker operator I want to be able to configure the host/port the broker binds to (via TOML, as already decided in ADR-0001) so that I can distinguish deployment environments.
29. As a Rust developer on the project I want public API errors to be returned in a typed form via `MqttError` so that calling code can pattern-match on error cases.
30. As a Rust developer on the project I want the codec to be testable in isolation from Tokio so that wire-format bugs are found in second-level tests rather than in end-to-end setups.
31. As a Rust developer on the project I want the TCP read loop to no longer assume one packet per `read()` call so that fragmented or bundled frames are handled in a spec-compliant way.
32. As a Rust developer on the project I want all `pub` items to be documented (`///`) so that the generated rustdoc documentation is complete.
33. As a Rust developer on the project I want `cargo clippy --all-targets -- -D warnings` to pass without warnings so that the Definition of Done from AGENT.md is satisfied.

## Implementation Decisions

### Modules

- **`codec`** (new, deep) — `MqttCodec` implements `tokio_util::codec::{Decoder, Encoder}`. Single place where wire-format knowledge lives (Fixed Header bits, Varint lengths, UTF-8 prefixes, Connect validation). `Decoder::Item = MqttPacket`. Pure logic on `BytesMut`, no Tokio socket dependency.
- **`topic_router`** (existing, modified) — Subscriber type is extended with `tx: mpsc::Sender<ConnectionCommand>`, field `qos` renamed to `granted_qos`, new method `unsubscribe(client_id, &[topic_filter])` supplements `remove_client` (which continues to remove all subscriptions). Wildcard matching stays as-is.
- **`client_registry`** (new, deep) — `HashMap<ClientId, mpsc::Sender<ConnectionCommand>>` behind `Mutex`. Methods: `swap_in(client_id, tx) → Option<old_tx>` (atomic insert-or-replace, returns the displaced sender), `remove(client_id)`. Encapsulates the race-critical takeover policy.
- **`error`** (new) — `MqttError` enum via `thiserror`. Incrementally replaces all `Box<dyn Error>` and `String` errors in the public API. Variants grow organically (`Io`, `MalformedFrame`, `UnsupportedQos`, `PublishTopicHasWildcards`, …).
- **`connection_task`** (existing as private fn, cleanly modularised) — orchestrates the per-client loop: `Framed`-based `select!` over three arms (incoming frames, outgoing `ConnectionCommand`s from the mpsc, Keep-Alive sleep). Inherently Tokio-coupled, therefore an integration-test subject.
- **`server`** (existing, simplified) — `MqttServer` accept loop, holds `Arc<Mutex<TopicRouter>>` and `Arc<ClientRegistry>`, passes both to `connection_task` per connection.
- **`config`** (existing, unchanged) — `BrokerConfig` from TOML, already tested.

### Data Types

- **`MqttPacket`** — a single enum for inbound and outbound (cf. grilling decision g1), structured variants with named fields. PUBLISH variant: `{ topic: String, payload: Bytes, qos: u8 }`. Encoder has `unreachable!()` arms for inbound-only variants — documents the asymmetry without forcing two separate enums.
- **`ConnectionCommand`** — enum with two variants: `DeliverFrame(Bytes)` (pre-rendered PUBLISH frames from the router fanout) and `Disconnect` (server-initiated termination on Client ID collision).
- **`Subscriber`** — `{ client_id: String, granted_qos: u8, tx: mpsc::Sender<ConnectionCommand> }`.

### Constants

- `MAX_SUPPORTED_QOS: u8 = 0` — physical code capability, not configuration. Raised when QoS 1/2 is implemented.
- `SUBSCRIBER_CHANNEL_CAPACITY: usize = 32` — per connection. On full: `try_send` → drop, no backpressure on the publisher.

### Behaviour Decisions

- **PUBLISH routing**: fanout via `Bytes::clone()` (ref-count copy, cheap). The Connection Task decoding the incoming PUBLISH is responsible for serialising the outbound PUBLISH frame and `try_send`-ing to each matched subscriber. Drop-on-full is QoS-0-compliant ("at most once").
- **PUBLISH validation**: wildcards (`+`/`#`) in the topic name are dropped + logged (spec §4.7.1.1: topic names must not contain wildcards). QoS > 0 is dropped + logged. DUP bit is ignored. RETAIN bit is ignored (Retained Messages out of scope).
- **CONNECT validation**: Protocol Name ≠ `"MQTT"` → discard frame as malformed, close socket without CONNACK. Protocol Level ≠ 4 → send CONNACK with Return Code 0x01, then close socket. Reserved Bit ≠ 0 → malformed. Client ID empty → CONNACK 0x02, then close.
- **CONNACK format**: SessionPresent always 0 (no session persistence in MVP). Return Code 0x00 on success.
- **SUBACK granted QoS**: `granted = min(requested, MAX_SUPPORTED_QOS)` per topic filter (silent downgrade). Failure code 0x80 is not used in MVP.
- **Keep-Alive**: read deadline = 1.5 × `keep_alive` from CONNECT. `keep_alive == 0` → no read timeout. Implemented as a third `select!` arm with `tokio::time::Sleep`, whose deadline is reset on every received frame via `as_mut().reset(...)` (clean arm symmetry).
- **Client ID takeover**: the **new** Connection Task is responsible for the kick — synchronously, **before** it registers itself in the TopicRouter:
  1. `let old = registry.swap_in(client_id, new_tx)`
  2. If `old.is_some()`: `old.send(ConnectionCommand::Disconnect).await; old.closed().await;`
  3. Send CONNACK
  4. Register own subscriptions in the router (happens only on SUBSCRIBE anyway)

  The `closed().await` guarantees that the old task has completed its `router.remove_client` cleanup before the new task proceeds — the takeover race is deterministically eliminated.
- **Cleanup path**: exactly one place — the end of the `connection_task` function — calls `router.remove_client` + `registry.remove`. Applies uniformly for DISCONNECT, read timeout, TCP error, and takeover kick.
- **Clean-Session flag**: read, logged, otherwise ignored. SessionPresent in CONNACK stays 0 — spec-compliant since the broker persists no sessions.
- **Stream framing**: structurally solved by `Framed` — the "one read() = one packet" bug of the current stack-buffer loop disappears without an explicit fix.

### Dependencies (new)

- `tokio-util = { version = "0.7", features = ["codec"] }`
- `thiserror = "1"`
- `tracing = "0.1"`
- `tracing-subscriber = { version = "0.3", features = ["env-filter"] }`

`bytes` is already available indirectly (Cargo.lock) and is declared explicitly as a direct dependency.

### Architecture References

- ADR-0002 — mpsc fanout, bounded, drop-on-full
- ADR-0003 — Framed codec, `thiserror`/`MqttError`
- ADR-0004 — ClientRegistry, takeover protocol with `closed().await`

## Testing Decisions

### Test Philosophy

Tests describe **observable behaviour**, not implementation details. Specifically:

- Codec tests check byte layouts (input → output), not internal helper functions.
- Router tests check what `get_subscribers_for_topic` returns — not how the HashMap is structured internally.
- Registry tests check the semantics of `swap_in` (what comes back, what is stored afterwards) — not whether a Mutex is used.
- Integration tests run real TCP sessions and check that bytes flow between two sockets — not which Tokio tasks are spawned.

### Modules to Test

- **`codec`** (highest test density):
  - Decoder happy path per packet type: CONNECT, PUBLISH (with/without payload, with/without multi-byte Varint), SUBSCRIBE (with multiple filters), UNSUBSCRIBE, PINGREQ, DISCONNECT.
  - Decoder malformed cases: buffer too short, Varint overflow (5th byte), wrong Protocol Name, Protocol Level ≠ 4, Reserved Bit set, empty Client ID, wildcards in PUBLISH topic, QoS > 2.
  - Decoder incomplete cases: half frame → `Ok(None)` (Decoder asks for more bytes).
  - Encoder happy path per outbound packet type: CONNACK (with various Return Codes), PUBLISH, SUBACK, UNSUBACK, PINGRESP.
  - Round-trip tests: `encode(p) then decode == p` for PUBLISH (the only bidirectional packet with non-trivial structure).
- **`topic_router`** (existing 9 tests retained, supplemented by):
  - `unsubscribe` removes a single filter, leaves others intact.
  - `unsubscribe` on a non-existent filter is a no-op (no error).
  - `subscribe` with channel tx: `get_subscribers_for_topic` returns the correct tx; `try_send` via that tx reaches a mock receiver.
- **`client_registry`**:
  - `swap_in` on empty slot → `None`.
  - `swap_in` with existing entry → old sender as `Some`, new sender stored afterwards.
  - `remove` deletes; afterwards `swap_in` → `None`.
- **`connection_task`** (end-to-end integration tests in `tests/`):
  - Two TCP clients: one subscribes to `home/+/temp`, the other publishes to `home/kitchen/temp` with payload — subscriber receives the bytes.
  - Multiple subscribers on the same topic — all receive.
  - Subscriber subscribes with `#` — receives messages at any topic depth.
  - Client ID collision: second CONNECT with same ID → first client receives TCP close, second client receives CONNACK 0x00.
  - Keep-Alive timeout: client with `keep_alive=1` and no traffic for > 1.5s → broker closes the connection.
  - DISCONNECT: regular conclusion, no error log, subscriptions cleaned up.

### Prior Art in the Repo

- `tests/integration_subscribe.rs` — byte-level tests of SUBSCRIBE/SUBACK frames, template for codec encoder tests.
- `tests/integration_tests.rs` — wildcard matching tests, supplemented by router tests in the new style.
- `tests/connection_test.rs` — real TCP roundtrips with `TcpStream`, template for end-to-end tests.
- `src/subscribe_handlers/storage.rs` module tests (`#[cfg(test)] mod tests`) — inline unit tests as convention, adopted for `codec`, `client_registry`, `error`.

## Out of Scope

- **Retained Messages** — explicitly out of scope in TODO.md, no separate data structure, RETAIN bit is ignored.
- **Last Will / Will-Message** — CONNECT flag is read, payload not parsed, no Will storage.
- **QoS 1 and 2** — `MAX_SUPPORTED_QOS = 0`, PUBACK/PUBREC/PUBREL/PUBCOMP not implemented, incoming PUBLISH with QoS > 0 are dropped.
- **Persistent Sessions** — Clean-Session flag is read but ignored, SessionPresent is always 0.
- **Authentication** — Username/Password fields from Connect flags are not parsed and not checked.
- **CLI args** (`--port`, `--host`, `--verbose`) — configuration remains purely TOML-based (ADR-0001).
- **Max-connections limit, max-payload size, graceful shutdown** — polish phase.
- **Observability topics** (`/stats`, `/broker`, throughput metrics) — polish phase.
- **Keep-Alive safety-net** for `keep_alive=0` (broker maximum as DoS protection) — TODO for later.
- **UUID generation for empty Client IDs** — empty Client ID is rejected in MVP, not replaced.

## Further Notes

- The roadmap in `TODO.md` is split into four phases (Foundation, Connection Lifecycle, PUBLISH Routing, Cleanup) and can be worked through phase by phase. Phase 1 is largely mechanical (dependency additions, error refactor, tracing migration); phases 2 and 3 contain the actual new logic; phase 4 is the Definition of Done.
- The verification pipeline from AGENT.md §4 (`cargo fmt --check && cargo check --all-targets && cargo clippy --all-targets -- -D warnings && cargo test`) must be green at the end. The existing 13 tests must be preserved (adjusted as needed to the new `MqttPacket` form).
- `unsafe` is forbidden project-wide (AGENT.md §2.4); no part of this PRD requires it.
- All new `pub` items need `///` doc-comments (AGENT.md §5).
- The codec is deliberately structured so that a later migration to an MQTT 5 variant (property maps, reason codes) is possible by adding new enum variants without touching the rest of the code.
