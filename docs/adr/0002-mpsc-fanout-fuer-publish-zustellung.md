# mpsc-Fanout für PUBLISH-Zustellung

Jeder Connection Task besitzt seinen `TcpStream` exklusiv. Damit ein PUBLISH-empfangender Task Nachrichten an die Sockets anderer Subscriber zustellen kann, registriert jeder Connection Task beim CONNECT einen `mpsc::Sender<Bytes>` als Teil seines `Subscriber`-Eintrags im `TopicRouter`. Der Connection Task `select!`-ed in seiner Hauptschleife zwischen `socket.read()` und `rx.recv()` und schreibt eingehende Frames auf seinen Socket.

Channels sind **bounded** (Kapazität tbd, Größenordnung 64). Publishing erfolgt mit `try_send`: ist der Channel voll, wird die Nachricht **verworfen**, der Publisher wird nicht gebremst.

## Considered Options

- **mpsc-Fanout (gewählt)** — idiomatisches Tokio, keine Lock-Contention im Hot-Path, Backpressure pro Subscriber lokal.
- **`Arc<Mutex<HashMap<ClientId, OwnedWriteHalf>>>`** — globaler Mutex bei jedem Publish; verstößt gegen das Tokio-Idiom „prefer message passing over shared state" aus AGENT.md.
- **`broadcast`-Channel pro Topic** — passt nicht zum Wildcard-Modell; das Routing-Match findet pro PUBLISH einmalig statt, nicht pro Subscription.
- **Unbounded mpsc** — einfacher, aber unbeschränktes RAM-Wachstum bei langsamen Subscribern, kein natürliches Loss-Verhalten.

## Consequences

- Verworfene Nachrichten sind für QoS 0 spec-konform („at most once"), für QoS 1/2 später **nicht** — Re-Design nötig, sobald höhere QoS unterstützt wird.
- `Subscriber` ist nicht mehr `Clone + PartialEq` ohne weiteres (Channel-Sender ist clonebar, aber semantisch heikel) — Tests im Router müssen darauf achten.
- `remove_client` muss weiterhin alle Subscriptions des Clients löschen; der Sender wird durch Drop des `rx` im Connection Task ohnehin obsolet.
- Channel-Kapazität ist ein Tuning-Knopf, der später konfigurierbar gemacht werden kann.
