# Context — rusty-mqtt

Ein MQTT 3.1.1 Broker MVP in Rust/Tokio.

## Glossar

- **Broker** — der Server-Prozess; nimmt TCP-Verbindungen an und vermittelt Nachrichten zwischen Clients.
- **Client** — externer MQTT-Teilnehmer; identifiziert durch eine **Client ID** aus dem CONNECT-Paket.
- **Connection Task** — der pro Client gespawnte Tokio-Task, der genau einen `TcpStream` besitzt und für seinen Lebenszyklus alleinverantwortlich ist.
- **Topic** — konkreter, wildcard-freier Pfad einer PUBLISH-Nachricht (z. B. `home/kitchen/temp`).
- **Topic Filter** — Muster, das ein Client beim SUBSCRIBE registriert, kann Wildcards `+` und `#` enthalten (z. B. `home/+/temp`).
- **Subscriber** — die im `TopicRouter` registrierte Adressierbarkeit eines Clients für einen Topic Filter; besteht aus Client ID, granted QoS und einem `mpsc::Sender`, über den der Connection Task PUBLISH-Frames empfängt.
- **TopicRouter** — In-Memory Registry: `HashMap<TopicFilter, Vec<Subscriber>>`. Einzige Wahrheit, wer wohin zugestellt bekommt.
- **Granted QoS** — der vom Broker akzeptierte QoS-Wert je Subscription, wie im SUBACK zurückgemeldet (kann ≤ requested QoS sein).
- **Frame** — eine vollständige, längen-präfixierte MQTT-Control-Packet-Sequenz auf der TCP-Leitung. Die Begriffe „Frame" und „Packet" werden in der MQTT-Spec teils synonym verwendet; im Code reservieren wir **Frame** für die wire-level Darstellung (Bytes) und **Packet** für die geparste enum-Repräsentation (`MqttPacket`).
- **Codec** — `MqttCodec`, eine Implementierung von `tokio_util::codec::{Decoder, Encoder}`, die Frames ⇄ Packets übersetzt. Einzige Quelle für Wire-Format-Wissen.
- **ClientRegistry** — In-Memory Registry: `HashMap<ClientId, mpsc::Sender<ConnectionCommand>>`. Einzige Wahrheit, wer aktuell verbunden ist. Getrennt vom `TopicRouter`, weil ein Client ohne Subscriptions existieren kann.
- **ConnectionCommand** — Nachrichtentyp auf dem mpsc-Channel zu einem Connection Task: `DeliverFrame(Bytes)` für vom Router weitergeleitete PUBLISH-Frames, `Disconnect` für serverseitiges Beenden (etwa bei Client-ID-Kollision).
