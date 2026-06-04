# Problem: mosquitto_sub fails with "Connection Refused: identifier rejected"

## Symptom

Running `mosquitto_sub` against the broker without explicitly providing a Client ID results in the following error:

```
Connection error: Connection Refused: identifier rejected.
```

Example command that triggers the issue:

```bash
mosquitto_sub -p 1884 -q 0 -t /hello/world
```

## Root Cause

MQTT 3.1.1 requires a non-empty Client ID in the CONNECT packet when Clean Session is not set. When `mosquitto_sub` is invoked without the `-i` flag, some versions send an empty Client ID string (`""`) in the CONNECT packet.

The broker validates the Client ID length in `validate_connect` (`src/lib.rs:631–639`):

```rust
let cid_len = ((buffer[offset] as usize) << 8) | (buffer[offset + 1] as usize);
offset += 2;
if cid_len == 0 {
    return ConnectValidation::EmptyClientId;  // → CONNACK 0x02
}
```

The connection loop then responds with CONNACK return code `0x02` and closes the socket (`src/lib.rs:299–303`):

```rust
ConnectValidation::EmptyClientId => {
    let connack = [0x20, 0x02, 0x00, 0x02];  // Return Code 0x02 — Identifier Rejected
    writer.write_all(&connack).await?;
    should_break = true;
    break;
}
```

This behaviour is spec-compliant. MQTT 3.1.1 only permits an empty Client ID when `CleanSession=1` — a capability the broker does not currently implement.

## Solution

Provide an explicit Client ID using the `-i` flag:

```bash
mosquitto_sub -p 1884 -q 0 -t /hello/world -i my-client
```

## Notes

Supporting empty Client IDs with `CleanSession=1` (where the broker assigns a unique ID) is an optional extension defined in MQTT 3.1.1 §3.1.3.1. It is not part of the current MVP scope.
