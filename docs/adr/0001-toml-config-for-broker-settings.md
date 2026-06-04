# TOML Configuration File for Broker Settings

On startup the broker reads optional settings (host, port) from a TOML file (`rusty-mqtt.toml`) in the working directory. If the file is absent, the broker starts with defaults (`127.0.0.1:1884`). On parse or validation errors the broker exits with a clear error message.

## Considered Options

- **TOML** (chosen): de-facto standard in Rust projects, consistent with `Cargo.toml`, easy to read and edit.
- **YAML**: well supported, but unrelated to the Rust ecosystem and error-prone due to whitespace semantics.
- **JSON**: well supported, but no comments possible — unsuitable for a configuration file that should be documented.
- **INI/.properties**: Java convention, uncommon in Rust and without type support.

## Consequences

- All future configuration fields (timeouts, QoS, max payload, logging) will be added to the same TOML file.
- The default port is intentionally `1884` instead of the MQTT standard `1883`, because port 1883 is occupied by another application on the development machine.
- CLI arguments (e.g. `--config <path>`) can be added later but are not needed for the current scope.
