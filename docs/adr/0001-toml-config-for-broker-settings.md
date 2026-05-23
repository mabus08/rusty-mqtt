# TOML-Konfigurationsdatei fuer Broker-Einstellungen

Der Broker liest beim Start optionale Einstellungen (Host, Port) aus einer TOML-Datei (`rusty-mqtt.toml`) im Arbeitsverzeichnis. Fehlt die Datei, startet der Broker mit Defaults (`127.0.0.1:1884`). Bei Parse- oder Validierungsfehlern bricht der Broker mit einer klaren Fehlermeldung ab.

## Considered Options

- **TOML** (gewaehlt): De-facto-Standard in Rust-Projekten, konsistent mit `Cargo.toml`, einfach lesbar und editierbar.
- **YAML**: Gut unterstuetzt, aber kein Bezug zum Rust-Oekosystem und fehleranfaellig durch Whitespace-Semantik.
- **JSON**: Gut unterstuetzt, aber keine Kommentare moeglich -- ungeeignet fuer eine Konfigurationsdatei, die dokumentiert werden soll.
- **INI/.properties**: Java-Konvention, in Rust unueblich und ohne Typisierung.

## Consequences

- Alle kuenftigen Konfigurationsfelder (Timeouts, QoS, Max Payload, Logging) werden in derselben TOML-Datei ergaenzt.
- Der Default-Port ist bewusst `1884` statt des MQTT-Standards `1883`, da Port 1883 auf der Entwicklungsmaschine durch eine andere Applikation belegt ist.
- CLI-Argumente (z.B. `--config <pfad>`) koennen spaeter ergaenzt werden, sind aber fuer den aktuellen Scope nicht noetig.
