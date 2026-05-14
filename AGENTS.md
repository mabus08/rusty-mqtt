# rusty-mqtt — agent guidance

## Rust toolchain
- `edition = "2024"` → requires **Rust 1.85+** (`rustc 2025-02-20` or later).
- No `rust-toolchain.toml` — agent must ensure the active toolchain is new enough.
- No `rustfmt.toml` or `clippy.toml` — defaults apply.

## Project structure
- Single crate (no workspace). Crate name `rusty-mqtt`, lib name `rusty_mqtt`.
- **Binary** entrypoint: `src/main.rs` — listens on `127.0.0.1:1883`.
- **Library** entrypoint: `src/lib.rs` — exports `MqttServer` and `MqttPacket`.
- Single **integration test**: `tests/connection_test.rs` — uses port **1885** (different from binary's 1883).

## Commands
```sh
# build
cargo build

# run broker (binds 127.0.0.1:1883)
cargo run

# run all tests
cargo test

# run just the integration test
cargo test --test connection_test

# lint (only if clippy is installed; no custom config)
cargo clippy --all-targets

# format (uses rustfmt defaults)
cargo fmt --check
cargo fmt
```

## Testing quirks
- `test_external_connection` spawns a `MqttServer` in a background task (port 1885), waits 50ms, then connects and sends a CONNECT packet. Expects CONNACK (`0x20`).
- Test uses `#[tokio::test]` — requires tokio runtime.
- Port 1885 is hardcoded in the test; ensure no other process occupies it.

## Codebase conventions
- All user-facing strings and comments are in **German**.
- Strings use emoji markers: `"👍 Connection succesfull"`, `"😒 Client Timeout"`.
- Packet parsing: `MqttPacket` enum with variants `Connect`, `Disconnect`, `Unknown`. Parsed via `(buffer[0] >> 4)` where `1` → `Connect`, `14` → `Disconnect`.
- CONNACK response is hardcoded `[0x20, 0x02, 0x00, 0x00]`.

## Known incomplete (from README)
- Subscription protocol (Task01)
- Publish protocol (Task02)
- Interface to request connected clients (Task03)
- Interface to request subscribed topics (Task04)
- Configuration file for timeouts and listener port (Task05)
