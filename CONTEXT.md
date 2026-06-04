# Context — rusty-mqtt

An MQTT 3.1.1 Broker MVP in Rust/Tokio.

## Glossary

- **Broker** — the server process; accepts TCP connections and routes messages between clients.
- **Client** — an external MQTT participant; identified by a **Client ID** from the CONNECT packet.
- **Connection Task** — the Tokio task spawned per client, which exclusively owns one `TcpStream` and is solely responsible for its lifecycle.
- **Topic** — a concrete, wildcard-free path of a PUBLISH message (e.g. `home/kitchen/temp`).
- **Topic Filter** — a pattern registered by a client via SUBSCRIBE; may contain wildcards `+` and `#` (e.g. `home/+/temp`).
- **Subscriber** — the addressability of a client registered in the `TopicRouter` for a Topic Filter; consists of Client ID, granted QoS, and an `mpsc::Sender` through which the Connection Task receives PUBLISH frames.
- **TopicRouter** — in-memory registry: `HashMap<TopicFilter, Vec<Subscriber>>`. The single source of truth for who receives what.
- **Granted QoS** — the QoS value accepted by the broker per subscription, as reported back in the SUBACK (may be ≤ requested QoS).
- **Frame** — a complete, length-prefixed MQTT Control Packet sequence on the TCP wire. The terms "Frame" and "Packet" are used partly synonymously in the MQTT spec; in the code we reserve **Frame** for the wire-level representation (bytes) and **Packet** for the parsed enum representation (`MqttPacket`).
- **Codec** — `MqttCodec`, an implementation of `tokio_util::codec::{Decoder, Encoder}` that translates Frames ⇄ Packets. The single source of wire-format knowledge.
- **ClientRegistry** — in-memory registry: `HashMap<ClientId, mpsc::Sender<ConnectionCommand>>`. The single source of truth for who is currently connected. Kept separate from `TopicRouter` because a client can exist without any subscriptions.
- **ConnectionCommand** — message type on the mpsc channel to a Connection Task: `DeliverFrame(Bytes)` for PUBLISH frames forwarded by the router, `Disconnect` for server-initiated termination (e.g. on Client ID collision).
