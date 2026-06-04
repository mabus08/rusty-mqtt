# Framed Codec and Typed Errors

The broker reads and writes MQTT frames via `tokio_util::codec::Framed` with a broker-owned `MqttCodec` that implements both `Decoder<Item = MqttPacket>` and `Encoder<MqttPacket>`. The codec is the single place in the code where wire-format knowledge lives (Varint lengths, bit layouts, UTF-8 length prefixes). In parallel, a typed `MqttError` enum is introduced via `thiserror`, replacing all public API signatures that currently use `Box<dyn Error>` or `String` as error types.

## Considered Options

- **`Framed` + Codec (chosen)** — canonical Tokio approach, cleanly isolated and testable, buffer management by the framework, encoder/decoder are symmetric.
- **Manual `BytesMut` buffer with `read_buf` loop** — no additional dependency, but subtle pitfalls (forgotten `advance`, fragmented frames, bundled frames in one read), and encoder/decoder would have to be built separately by hand.
- **Specialised crate such as `mqttbytes`** — would undermine the learning goal of the project (implementing MQTT from scratch).

For error handling:

- **`thiserror` with `MqttError` enum (chosen)** — AGENT.md §2 requires it for library code; it follows from the Decoder trait needing a typed error; makes calling logic pattern-matchable.
- **`anyhow::Error`** — convenient, but blurs error categories at the API boundary and is intended for applications, not libraries, per AGENT.md.
- **`Box<dyn Error>` + `String`** — status quo; no structural information, no pattern-matching, no `From` impls.

## Consequences

- New dependency `tokio-util` with feature `codec` — small, part of the Tokio ecosystem, no third-party risk.
- `parse_packet` (lib.rs:310), `parse_connect_client_id` (lib.rs:280), `handle_subscribe_impl` (lib.rs:229) and `generate_suback` (subscribe_handlers/mod.rs:41) migrate incrementally into `MqttCodec`; the old functions disappear or become thin wrappers.
- `MqttServer::run` and `handle_connection` get `Result<(), MqttError>` as their signature. `main.rs` may continue to use `anyhow`/`Box<dyn Error>` because it is application code.
- The Connection Task becomes a `tokio::select!` between `framed.next()` (incoming frames) and `rx.recv()` (bytes to send from the router) — the "one read = one packet" bug of the current stack-buffer loop disappears automatically.
- `MqttError` variants are added incrementally as needed; specifying the full enum upfront is not worthwhile because the variants only crystallise during decoder development.
