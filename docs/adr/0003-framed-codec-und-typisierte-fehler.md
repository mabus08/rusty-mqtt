# Framed-Codec und typisierte Fehler

Der Broker liest und schreibt MQTT-Frames über `tokio_util::codec::Framed` mit einem broker-eigenen `MqttCodec`, der sowohl `Decoder<Item = MqttPacket>` als auch `Encoder<MqttPacket>` implementiert. Der Codec ist die einzige Stelle im Code, an der Wire-Format-Wissen (Varint-Längen, Bit-Layouts, UTF-8-Längen-Prefixe) lebt. Parallel wird ein typisiertes `MqttError`-Enum via `thiserror` eingeführt, das alle Public-API-Signaturen ersetzt, in denen aktuell `Box<dyn Error>` oder `String` als Fehlertyp verwendet werden.

## Considered Options

- **`Framed` + Codec (gewählt)** — kanonischer Tokio-Weg, sauber isoliert testbar, Buffer-Management vom Framework, Encoder/Decoder spiegeln sich.
- **Manueller `BytesMut`-Buffer mit `read_buf`-Schleife** — keine zusätzliche Dependency, aber subtile Fehlerquellen (vergessene `advance`, fragmentierte Frames, gebündelte Frames in einem Read), und Encoder/Decoder müssten getrennt von Hand gebaut werden.
- **Spezial-Crate wie `mqttbytes`** — würde das Lernziel des Projekts (MQTT selbst implementieren) unterlaufen.

Für die Fehlerbehandlung:

- **`thiserror` mit `MqttError`-Enum (gewählt)** — AGENT.md §2 fordert es für Lib-Code; ergibt sich aus dem Decoder-Trait, das einen typisierten Error braucht; macht aufrufende Logik mustermatchbar.
- **`anyhow::Error`** — bequem, verwischt aber Fehlerkategorien an der API-Grenze und ist laut AGENT.md für Applikationen, nicht für die Lib gedacht.
- **`Box<dyn Error>` + `String`** — Status quo; keine Strukturinformation, kein Pattern-Matching, keine `From`-Impls.

## Consequences

- Neue Dependency `tokio-util` mit Feature `codec` — klein, gehört zum Tokio-Ökosystem, kein Drittanbieter-Risiko.
- `parse_packet` (lib.rs:310), `parse_connect_client_id` (lib.rs:280), `handle_subscribe_impl` (lib.rs:229) und `generate_suback` (subscribe_handlers/mod.rs:41) ziehen schrittweise in `MqttCodec` um; die alten Funktionen verschwinden oder werden zu thin wrappers.
- `MqttServer::run` und `handle_connection` bekommen `Result<(), MqttError>` als Signatur. `main.rs` darf weiterhin `anyhow`/`Box<dyn Error>` verwenden, weil es Application-Code ist.
- Der Connection Task wird ein `tokio::select!` zwischen `framed.next()` (eingehende Frames) und `rx.recv()` (auszusendende Bytes vom Router) — der „ein read = ein Paket"-Bug des Stack-Buffer-Loops verschwindet automatisch.
- `MqttError`-Varianten werden bei Bedarf inkrementell ergänzt; ein vorab vollständig spezifiziertes Enum lohnt sich nicht, weil sich die Varianten erst beim Decoder-Schreiben herauskristallisieren.
