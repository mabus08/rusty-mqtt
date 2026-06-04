# ClientRegistry und Takeover-Protokoll bei Client-ID-Kollision

Eine `ClientRegistry` (`HashMap<ClientId, mpsc::Sender<ConnectionCommand>>`) wird neben dem `TopicRouter` gehalten und ist die einzige Wahrheit darüber, welche Clients aktuell verbunden sind. Der mpsc-Channel zum Connection Task trägt das Enum `ConnectionCommand` mit zwei Varianten: `DeliverFrame(Bytes)` (vom Router weitergeleitete PUBLISH-Bytes, vgl. ADR-0002) und `Disconnect` (serverseitig angeordnetes Beenden).

Tritt ein zweiter CONNECT mit derselben Client ID ein, **muss** laut MQTT 3.1.1 §3.1.4 die bestehende Verbindung beendet werden. Das geschieht synchron im Kontext des **neuen** Connection Task **bevor** dieser sich selbst in Registry oder Router einträgt:

1. `let old = registry.swap_in(client_id, new_tx)`
2. Falls vorhanden: `old.send(ConnectionCommand::Disconnect).await; old.closed().await;`
3. (Erst jetzt) eigene Subscriptions im `TopicRouter` registrieren

Schritt 2 wartet auf das Schließen des alten Channels — was passiert, sobald der alte Connection Task seinen `select!`-Loop verlässt und seinen Receiver fallen lässt. Damit ist garantiert, dass der alte Task seinen `router.remove_client`-Cleanup beendet hat, bevor der neue Task irgendetwas in den Router einträgt.

## Considered Options

- **Erweiterter Command-Channel + synchroner Kick mit `closed().await` (gewählt)** — eine Synchronisations-Primitive pro Client, einheitlicher Cleanup-Pfad für „kicked", „self-DISCONNECT", „TCP error" und „read timeout", deterministische Eliminierung der Takeover-Race.
- **Separater `oneshot::Sender<()>` als Kick-Handle** — zweites Primitive, Edge Cases wenn der oneshot bereits durch Self-Disconnect konsumiert wurde, kein gemeinsamer Code-Pfad mit dem regulären Frame-Empfang.
- **`tokio::task::JoinHandle::abort()`** — reißt den alten Task mitten im `.await` ab, überspringt damit den `remove_client`-Cleanup; hinterlässt dangling Subscriptions im Router. Verworfen.
- **Eine kombinierte Struktur** statt zweier Registries — würde TopicRouter-Einträge für Clients ohne Subscriptions erzwingen oder einen Sonderfall „verbunden, aber keine Subscriptions" mitführen. Bricht die Glossardefinition „TopicRouter = wer hört zu".
- **Kick-and-forget + Epoch-Counter pro Client** — vermeidet das `closed().await`, fügt aber pro Frame eine Epoch-Validierung hinzu und macht den Cleanup-Pfad nicht-lokal. Mehr Code, weniger Determinismus.

## Consequences

- `MqttServer` hält zwei `Arc`s: `Arc<Mutex<TopicRouter>>` und `Arc<ClientRegistry>` (`ClientRegistry` kapselt seinen Mutex selbst). Beide werden an jeden Connection Task übergeben.
- Der Connection Task ist die einzige Stelle, die `router.remove_client(&self.client_id)` und `registry.remove(&self.client_id)` aufruft — niemals von außen. Damit gibt es genau einen Cleanup-Pfad.
- `mpsc::Sender::closed()` ist Teil der stabilen Tokio-API; kein nightly, kein Workaround.
- Der Kick-Pfad serialisiert: der neue CONNECT antwortet erst mit CONNACK, nachdem der alte Task vollständig beendet ist. Im Pathologie-Fall (alter Task hängt in einem `await`) dauert das. Für den MVP akzeptabel; falls relevant, wäre `tokio::time::timeout` um Schritt 2 die nächste Eskalationsstufe.
- Vor dem CONNECT (also bevor `client_id` bekannt ist) hat der Connection Task **keinen** Registry-Eintrag und **keinen** mpsc-Channel. Der Channel wird im CONNECT-Handler erzeugt, nicht in `accept()`.
