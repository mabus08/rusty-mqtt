# PRD 0001 — MQTT 3.1.1 Broker MVP: End-to-End Pub/Sub

**Status:** ready-for-agent
**Glossar:** [`CONTEXT.md`](../../CONTEXT.md)
**Architektur:** [`docs/adr/0002`](../adr/0002-mpsc-fanout-fuer-publish-zustellung.md), [`0003`](../adr/0003-framed-codec-und-typisierte-fehler.md), [`0004`](../adr/0004-clientregistry-und-takeover-protokoll.md)
**Roadmap:** [`TODO.md`](../../TODO.md)

## Problem Statement

Als MQTT-Client-Entwickler kann ich mich derzeit zwar mit dem Broker verbinden, SUBSCRIBE-Pakete absenden und PING-Roundtrips fahren — ich kann aber **keine Nachrichten zwischen zwei Clients austauschen**. Ein PUBLISH wird vom Broker still verschluckt, kein Subscriber empfängt jemals etwas. Damit ist der Broker für seinen eigentlichen Zweck unbrauchbar.

Zusätzlich gibt es mehrere stille Spec-Verletzungen, die in realen Setups zu schwer diagnostizierbaren Fehlern führen: der Keep-Alive-Wert aus dem CONNECT wird ignoriert (tote Clients hängen minutenlang als verbunden), zwei parallele CONNECTs mit derselben Client ID werden beide angenommen (anstatt die alte Verbindung zu beenden), und SUBACK granted QoS-Werte, die der Broker faktisch nicht liefert.

## Solution

Der Broker wird zum vollwertigen MQTT-3.1.1-MVP für QoS 0 ausgebaut. Ein PUBLISH eines Clients wird an alle Subscriber, deren Topic Filter (inkl. `+`/`#`-Wildcards) auf das Topic matchen, zugestellt. Keep-Alive aus dem CONNECT wird respektiert, Client-ID-Kollisionen führen zum spec-konformen Takeover, und SUBACK reflektiert ehrlich, was der Broker liefern kann.

Konkret kann der Nutzer am Ende mit zwei `mosquitto`-Clients eine vollständige Pub/Sub-Session fahren: ein `mosquitto_sub -t 'home/+/temp'` empfängt jede `mosquitto_pub -t 'home/kitchen/temp' -m '21.5'`-Nachricht, idle Clients werden nach Ablauf ihres Keep-Alive-Intervalls geräumt, und ein zweiter Connect mit demselben `--id` killt die erste Session sauber.

## User Stories

1. Als IoT-Entwickler möchte ich ein PUBLISH mit konkretem Topic an den Broker senden, sodass alle Subscriber mit passendem Topic Filter die Nachricht empfangen.
2. Als IoT-Entwickler möchte ich beim SUBSCRIBE einen Topic Filter mit `+`-Wildcard angeben, sodass ich exakt ein Topic-Level frei lassen kann (z. B. `home/+/temp`).
3. Als IoT-Entwickler möchte ich beim SUBSCRIBE einen Topic Filter mit terminalem `#` angeben, sodass ich beliebig viele Subtopics auf einmal abonniere (z. B. `sensors/#`).
4. Als IoT-Entwickler möchte ich Payloads beliebiger Größe (innerhalb der Varint-Grenze) versenden, sodass auch JSON-Sensordaten von einigen KB sauber durchgereicht werden.
5. Als IoT-Entwickler möchte ich, dass mein Subscriber binäre Payloads byte-genau erhält, sodass ich keine Encoding-Verluste habe.
6. Als IoT-Entwickler möchte ich, dass ein PUBLISH mit Wildcards im Topic vom Broker abgelehnt wird, sodass die Spec-Garantien für Topic-Namen erhalten bleiben.
7. Als IoT-Entwickler möchte ich, dass mein PUBLISH mit QoS > 0 vorerst stillschweigend verworfen wird (mit Log-Warnung), sodass der Broker stabil bleibt, bis QoS 1/2 unterstützt wird.
8. Als IoT-Entwickler möchte ich ein SUBSCRIBE mit angefordertem QoS 1 oder 2 absenden können und ein SUBACK mit granted QoS 0 erhalten, sodass meine Client-Library spec-konform downgraden kann.
9. Als IoT-Entwickler möchte ich, dass mehrere Subscriber gleichzeitig auf dasselbe Topic abonniert sind und alle die Nachricht erhalten, sodass Fan-Out funktioniert.
10. Als IoT-Entwickler möchte ich, dass ich ein UNSUBSCRIBE absetzen kann und danach für diese Filter keine PUBLISH-Nachrichten mehr erhalte, sodass mein Client gezielt Topics abbestellen kann.
11. Als IoT-Entwickler möchte ich, dass auf mein UNSUBSCRIBE ein UNSUBACK mit derselben Packet ID zurückkommt, sodass meine Client-Library die Quittung zuordnen kann.
12. Als IoT-Entwickler möchte ich, dass mein Client beim CONNECT einen Keep-Alive-Wert übermittelt und der Broker mich erst nach 1,5× dieses Intervalls als tot betrachtet, sodass ich die Heartbeat-Frequenz selbst bestimmen kann.
13. Als IoT-Entwickler möchte ich, dass ein PINGREQ innerhalb des Keep-Alive-Intervalls den Read-Timer beim Broker zurücksetzt, sodass mein langlebiger Subscriber online bleibt.
14. Als IoT-Entwickler möchte ich, dass jeder beliebige Control Packet (nicht nur PINGREQ) den Keep-Alive-Timer resettet, sodass aktive Publisher nicht zusätzlich pingen müssen.
15. Als IoT-Entwickler möchte ich `keep_alive=0` setzen können, sodass mein Client ohne Heartbeat-Pflicht arbeiten kann.
16. Als IoT-Entwickler möchte ich, dass ein zweiter CONNECT mit derselben Client ID die alte Verbindung sauber beendet, bevor er CONNACK auf die neue Verbindung schickt, sodass es nie zwei aktive Sessions mit derselben ID gibt.
17. Als IoT-Entwickler möchte ich, dass beim Takeover die Subscriptions der alten Session vollständig geräumt sind, bevor die neue Session ihre eigenen registriert, sodass kein Cleanup-Race meine neuen Subscriptions versehentlich löscht.
18. Als IoT-Entwickler möchte ich, dass mein CONNECT mit falschem Protocol Name vom Broker ohne CONNACK abgelehnt wird, sodass mein Client schnell erkennt, dass er nicht mit MQTT 3.1.1 spricht.
19. Als IoT-Entwickler möchte ich, dass ein CONNECT mit Protocol Level ≠ 4 ein CONNACK mit Return Code 0x01 (Unacceptable Protocol Version) erhält und die Verbindung danach geschlossen wird, sodass mein Client eine eindeutige Diagnose hat.
20. Als IoT-Entwickler möchte ich, dass ein CONNECT mit leerer Client ID mit CONNACK Return Code 0x02 (Identifier Rejected) abgelehnt wird, sodass anonyme Connects nicht stillschweigend funktionieren.
21. Als IoT-Entwickler möchte ich, dass ein CONNECT mit gesetztem Reserved-Bit verworfen wird, sodass Spec-Violationen früh sichtbar werden.
22. Als IoT-Entwickler möchte ich, dass mein DISCONNECT vom Broker als regulärer Abschluss behandelt wird (kein Error-Log), sodass meine Test-Suiten saubere Outputs haben.
23. Als IoT-Entwickler möchte ich, dass nach einem DISCONNECT oder Read-Timeout sowohl die ClientRegistry- als auch alle TopicRouter-Einträge meines Clients entfernt werden, sodass ich nach Reconnect keinen Ghost-State erbe.
24. Als IoT-Entwickler möchte ich, dass mein Clean-Session-Flag im CONNECT gelesen und SessionPresent im CONNACK konsistent auf 0 gesetzt wird, sodass mein Client weiß, dass keine Session resumed wird.
25. Als Broker-Betreiber möchte ich, dass alle Connection-Lifecycle-Events (Connect, Subscribe, Publish, Disconnect, Kick) via `tracing` strukturiert geloggt werden, sodass ich Probleme nachvollziehen kann.
26. Als Broker-Betreiber möchte ich, dass `println!` nirgends mehr im Library-Code verwendet wird, sodass Logging einheitlich konfigurierbar ist.
27. Als Broker-Betreiber möchte ich, dass langsame Subscriber ihre eigenen Frames verlieren statt den Publisher zu blockieren, sodass ein einzelner kaputter Client den Broker nicht lahmlegt.
28. Als Broker-Betreiber möchte ich konfigurieren können, mit welchem Host/Port der Broker bindet (via TOML, wie bereits in ADR-0001 entschieden), sodass ich Deployment-Umgebungen unterscheiden kann.
29. Als Rust-Entwickler am Projekt möchte ich, dass Public-API-Fehler typisiert via `MqttError` zurückgegeben werden, sodass aufrufende Code-Stellen Fehlerfälle pattern-matchen können.
30. Als Rust-Entwickler am Projekt möchte ich, dass der Codec isoliert von Tokio testbar ist, sodass Wire-Format-Bugs in Sekunden-Tests gefunden werden statt in End-to-End-Setups.
31. Als Rust-Entwickler am Projekt möchte ich, dass der TCP-Read-Loop nicht mehr ein Paket pro `read()`-Aufruf annimmt, sodass fragmentierte oder gebündelte Frames spec-konform behandelt werden.
32. Als Rust-Entwickler am Projekt möchte ich, dass alle `pub`-Items dokumentiert sind (`///`), sodass die generierte rustdoc-Dokumentation vollständig ist.
33. Als Rust-Entwickler am Projekt möchte ich, dass `cargo clippy --all-targets -- -D warnings` ohne Warnings durchläuft, sodass die Definition-of-Done aus AGENT.md erfüllt ist.

## Implementation Decisions

### Module

- **`codec`** (neu, deep) — `MqttCodec` implementiert `tokio_util::codec::{Decoder, Encoder}`. Einzige Stelle, an der Wire-Format-Wissen lebt (Fixed-Header-Bits, Varint-Längen, UTF-8-Prefixe, Connect-Validierung). `Decoder::Item = MqttPacket`. Pure Logik auf `BytesMut`, keine Tokio-Socket-Abhängigkeit.
- **`topic_router`** (existiert, modifiziert) — Subscriber-Typ wird um `tx: mpsc::Sender<ConnectionCommand>` erweitert, Feld `qos` wird zu `granted_qos` umbenannt, neue Methode `unsubscribe(client_id, &[topic_filter])` ergänzt `remove_client` (das weiterhin alle Subscriptions löscht). Wildcard-Matching bleibt wie ist.
- **`client_registry`** (neu, deep) — `HashMap<ClientId, mpsc::Sender<ConnectionCommand>>` hinter `Mutex`. Methoden: `swap_in(client_id, tx) → Option<old_tx>` (atomisches Insert-or-Replace, gibt den verdrängten Sender zurück), `remove(client_id)`. Kapselt die Race-kritische Takeover-Policy.
- **`error`** (neu) — `MqttError`-Enum via `thiserror`. Ersetzt schrittweise alle `Box<dyn Error>` und `String`-Errors in der Public-API. Varianten wachsen organisch (`Io`, `MalformedFrame`, `UnsupportedQos`, `PublishTopicHasWildcards`, …).
- **`connection_task`** (existiert als private fn, wird sauber modularisiert) — Orchestriert den per-Client-Loop: `Framed`-basierter `select!` über drei Arme (eingehende Frames, ausgehende `ConnectionCommand`s aus dem mpsc, Keep-Alive-Sleep). Inhärent Tokio-gekoppelt, daher Integrationstest-Subjekt.
- **`server`** (existiert, vereinfacht) — `MqttServer`-Accept-Loop, hält `Arc<Mutex<TopicRouter>>` und `Arc<ClientRegistry>`, reicht beide pro Verbindung an `connection_task` durch.
- **`config`** (existiert, unverändert) — `BrokerConfig` aus TOML, bereits getestet.

### Datentypen

- **`MqttPacket`** — ein einziges Enum für Inbound und Outbound (vgl. Grilling-Entscheidung g1), strukturierte Varianten mit benannten Feldern. PUBLISH-Variante: `{ topic: String, payload: Bytes, qos: u8 }`. Encoder hat `unreachable!()`-Arme für Inbound-Only-Varianten — dokumentiert die Asymmetrie ohne zwei separate Enums zu erzwingen.
- **`ConnectionCommand`** — Enum mit zwei Varianten: `DeliverFrame(Bytes)` (vorgerenderte PUBLISH-Frames vom Router-Fanout) und `Disconnect` (serverseitiges Beenden bei Client-ID-Kollision).
- **`Subscriber`** — `{ client_id: String, granted_qos: u8, tx: mpsc::Sender<ConnectionCommand> }`.

### Konstanten

- `MAX_SUPPORTED_QOS: u8 = 0` — physische Code-Capability, nicht Konfiguration. Wird hochgesetzt, wenn QoS 1/2 implementiert wird.
- `SUBSCRIBER_CHANNEL_CAPACITY: usize = 32` — pro Connection. Bei Vollheit `try_send` → drop, kein Backpressure auf den Publisher.

### Verhaltens-Entscheidungen

- **PUBLISH-Routing**: Fanout per `Bytes::clone()` (Refcount-Kopie, billig). Der Connection Task, der den eingehenden PUBLISH dekodiert, ist verantwortlich für die Serialisierung des outbound PUBLISH-Frames und das `try_send` an jeden gematchten Subscriber. Drop-on-Full ist QoS-0-konform („at most once").
- **PUBLISH-Validierung**: Wildcards (`+`/`#`) im Topic-Namen werden verworfen + geloggt (spec §4.7.1.1: Topic Names dürfen keine Wildcards enthalten). QoS > 0 wird verworfen + geloggt. DUP-Bit wird ignoriert. RETAIN-Bit wird ignoriert (Retained Messages out-of-scope).
- **CONNECT-Validierung**: Protocol Name ≠ `"MQTT"` → Frame als malformed verwerfen, Socket schließen ohne CONNACK. Protocol Level ≠ 4 → CONNACK mit Return Code 0x01 senden, danach Socket schließen. Reserved Bit ≠ 0 → malformed. Client ID leer → CONNACK 0x02, dann schließen.
- **CONNACK-Format**: SessionPresent immer 0 (keine Session-Persistenz im MVP). Return Code 0x00 bei Erfolg.
- **SUBACK-Granted-QoS**: `granted = min(requested, MAX_SUPPORTED_QOS)` pro Topic Filter (silent downgrade). Failure-Code 0x80 wird im MVP nicht verwendet.
- **Keep-Alive**: Read-Deadline = 1.5 × `keep_alive` aus CONNECT. `keep_alive == 0` → kein Read-Timeout. Implementierung als dritter `select!`-Arm mit `tokio::time::Sleep`, dessen Deadline bei jedem empfangenen Frame via `as_mut().reset(...)` neu gesetzt wird (saubere Symmetrie der Arme).
- **Client-ID-Takeover**: Der **neue** Connection Task ist verantwortlich für den Kick — synchron, **bevor** er sich selbst in TopicRouter einträgt:
  1. `let old = registry.swap_in(client_id, new_tx)`
  2. Falls `old.is_some()`: `old.send(ConnectionCommand::Disconnect).await; old.closed().await;`
  3. CONNACK senden
  4. Eigene Subscriptions im Router registrieren (passiert ohnehin erst beim SUBSCRIBE)

  Das `closed().await` garantiert, dass der alte Task seinen `router.remove_client`-Cleanup beendet hat, bevor der neue Task fortfährt — damit ist die Takeover-Race deterministisch eliminiert.
- **Cleanup-Pfad**: Genau eine Stelle — das Ende der `connection_task`-Funktion — ruft `router.remove_client` + `registry.remove`. Greift einheitlich für DISCONNECT, Read-Timeout, TCP-Error, Takeover-Kick.
- **Clean-Session-Flag**: gelesen, geloggt, sonst ignoriert. SessionPresent im CONNACK bleibt 0 — spec-konform, da der Broker keine Sessions persistiert.
- **Stream-Framing**: Wird durch `Framed` strukturell gelöst — der „ein read() = ein Paket"-Bug des aktuellen Stack-Buffer-Loops verschwindet ohne expliziten Fix.

### Dependencies (neu)

- `tokio-util = { version = "0.7", features = ["codec"] }`
- `thiserror = "1"`
- `tracing = "0.1"`
- `tracing-subscriber = { version = "0.3", features = ["env-filter"] }`

`bytes` ist bereits indirekt verfügbar (Cargo.lock) und wird explizit als Direct-Dep deklariert.

### Architektur-Verweise

- ADR-0002 — mpsc-Fanout, bounded, drop-on-full
- ADR-0003 — Framed-Codec, `thiserror`/`MqttError`
- ADR-0004 — ClientRegistry, Takeover-Protokoll mit `closed().await`

## Testing Decisions

### Test-Philosophie

Tests beschreiben **beobachtbares Verhalten**, nicht Implementierungsdetails. Konkret:

- Codec-Tests prüfen Byte-Layouts (Input → Output), nicht interne Hilfsfunktionen.
- Router-Tests prüfen, was `get_subscribers_for_topic` zurückgibt — nicht, wie die HashMap intern strukturiert ist.
- Registry-Tests prüfen die Semantik von `swap_in` (was kommt zurück, was steht danach drin) — nicht, ob ein Mutex verwendet wird.
- Integrationstests fahren echte TCP-Sessions und prüfen, dass Bytes zwischen zwei Sockets fließen — nicht, welche Tokio-Tasks gespawnt werden.

### Zu testende Module

- **`codec`** (höchste Test-Dichte):
  - Decoder-Happy-Path pro Pakettyp: CONNECT, PUBLISH (mit/ohne Payload, mit/ohne Varint-Multibyte), SUBSCRIBE (mit mehreren Filtern), UNSUBSCRIBE, PINGREQ, DISCONNECT.
  - Decoder-Malformed-Cases: zu kurzer Buffer, Varint-Overflow (5. Byte), falscher Protocol Name, Protocol Level ≠ 4, Reserved Bit gesetzt, leere Client ID, Wildcards in PUBLISH-Topic, QoS > 2.
  - Decoder-Incomplete-Cases: halber Frame → `Ok(None)` (Decoder fragt nach mehr Bytes).
  - Encoder-Happy-Path pro Outbound-Pakettyp: CONNACK (mit verschiedenen Return Codes), PUBLISH, SUBACK, UNSUBACK, PINGRESP.
  - Round-Trip-Tests: `encode(p) then decode == p` für PUBLISH (das einzige bidirektionale Paket mit nicht-trivialer Struktur).
- **`topic_router`** (existierende 9 Tests erhalten, ergänzt um):
  - `unsubscribe` entfernt einzelnen Filter, lässt andere stehen.
  - `unsubscribe` auf nicht-existenten Filter ist no-op (kein Error).
  - `subscribe` mit Channel-tx: `get_subscribers_for_topic` liefert den richtigen tx zurück; `try_send` über diesen tx erreicht einen Mock-Receiver.
- **`client_registry`**:
  - `swap_in` auf leeren Slot → `None`.
  - `swap_in` mit existierendem Eintrag → alter Sender als `Some` zurück, neuer Sender ist danach gespeichert.
  - `remove` löscht; danach `swap_in` → `None`.
- **`connection_task`** (End-to-End-Integrationstests in `tests/`):
  - Zwei TCP-Clients: einer subt `home/+/temp`, anderer pubt `home/kitchen/temp` mit Payload — Subscriber empfängt die Bytes.
  - Mehrere Subscriber auf demselben Topic — alle empfangen.
  - Subscriber abonniert mit `#` — empfängt Nachrichten auf beliebiger Topic-Tiefe.
  - Client-ID-Kollision: zweiter CONNECT mit gleicher ID → erster Client erhält TCP-Close, zweiter Client erhält CONNACK 0x00.
  - Keep-Alive-Timeout: Client mit `keep_alive=1` und kein Traffic für > 1.5s → Broker schließt die Verbindung.
  - DISCONNECT: regulärer Abschluss, kein Error-Log, Subscriptions geräumt.

### Prior Art im Repo

- `tests/integration_subscribe.rs` — Byte-Level-Tests von SUBSCRIBE/SUBACK-Frames, Vorlage für Codec-Encoder-Tests.
- `tests/integration_tests.rs` — Wildcard-Matching-Tests, ergänzt durch Router-Tests im neuen Stil.
- `tests/connection_test.rs` — echte TCP-Roundtrips mit `TcpStream`, Vorlage für die End-to-End-Tests.
- `src/subscribe_handlers/storage.rs` Modul-Tests (`#[cfg(test)] mod tests`) — Inline-Unit-Tests als Konvention, wird für `codec`, `client_registry`, `error` übernommen.

## Out of Scope

- **Retained Messages** — explizit verworfen in TODO.md, keine separate Datenstruktur, RETAIN-Bit wird ignoriert.
- **Last Will / Will-Message** — CONNECT-Flag wird gelesen, Payload nicht geparst, keine Will-Speicherung.
- **QoS 1 und 2** — `MAX_SUPPORTED_QOS = 0`, PUBACK/PUBREC/PUBREL/PUBCOMP nicht implementiert, eingehende PUBLISH mit QoS > 0 werden verworfen.
- **Persistente Sessions** — Clean-Session-Flag wird gelesen aber ignoriert, SessionPresent bleibt immer 0.
- **Authentifizierung** — Username/Password-Felder aus den Connect-Flags werden nicht geparst, nicht geprüft.
- **CLI-Args** (`--port`, `--host`, `--verbose`) — Konfiguration bleibt rein TOML-basiert (ADR-0001).
- **Max-Connections-Limit, Max-Payload-Size, Graceful Shutdown** — Polish-Phase.
- **Observability-Topics** (`/stats`, `/broker`, Throughput-Metriken) — Polish-Phase.
- **Keep-Alive-Safety-Net** für `keep_alive=0` (Broker-Maximalwert als DoS-Schutz) — TODO für später.
- **UUID-Generierung für leere Client IDs** — leere Client ID wird im MVP abgelehnt, nicht ersetzt.

## Further Notes

- Die Roadmap in `TODO.md` ist in vier Phasen geschnitten (Foundation, Connection Lifecycle, PUBLISH-Routing, Cleanup) und kann phasenweise abgearbeitet werden. Phase 1 ist überwiegend mechanisch (Dep-Adds, Error-Refactor, Tracing-Migration); Phase 2 und 3 enthalten die eigentliche neue Logik; Phase 4 ist Definition-of-Done.
- Die Verifikationspipeline aus AGENT.md §4 (`cargo fmt --check && cargo check --all-targets && cargo clippy --all-targets -- -D warnings && cargo test`) muss am Ende grün sein. Die aktuellen 13 Tests müssen erhalten bleiben (gegebenenfalls angepasst an die neue `MqttPacket`-Form).
- `unsafe` ist projektweit verboten (AGENT.md §2.4); keine Stelle dieses PRDs erfordert es.
- Alle neuen `pub` Items brauchen `///`-Doc-Comments (AGENT.md §5).
- Der Codec ist absichtlich so geschnitten, dass eine spätere Migration auf eine MQTT-5-Variante (Property-Maps, Reason Codes) durch Hinzufügen neuer Enum-Varianten möglich bleibt, ohne den Rest des Codes anzufassen.
