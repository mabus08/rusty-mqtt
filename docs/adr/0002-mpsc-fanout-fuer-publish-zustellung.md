# mpsc Fanout for PUBLISH Delivery

Each Connection Task owns its `TcpStream` exclusively. To allow a PUBLISH-receiving task to deliver messages to the sockets of other subscribers, each Connection Task registers an `mpsc::Sender<Bytes>` as part of its `Subscriber` entry in the `TopicRouter` on CONNECT. The Connection Task `select!`s in its main loop between `socket.read()` and `rx.recv()` and writes incoming frames to its socket.

Channels are **bounded** (capacity tbd, order of magnitude 64). Publishing uses `try_send`: if the channel is full the message is **dropped** and the publisher is not blocked.

## Considered Options

- **mpsc fanout (chosen)** — idiomatic Tokio, no lock contention in the hot path, backpressure per subscriber is local.
- **`Arc<Mutex<HashMap<ClientId, OwnedWriteHalf>>>`** — global mutex on every publish; violates the Tokio idiom "prefer message passing over shared state" from AGENT.md.
- **`broadcast` channel per topic** — does not fit the wildcard model; the routing match happens once per PUBLISH, not per subscription.
- **Unbounded mpsc** — simpler, but unbounded RAM growth for slow subscribers and no natural loss behaviour.

## Consequences

- Dropped messages are spec-compliant for QoS 0 ("at most once"), but **not** for QoS 1/2 later — a re-design will be needed once higher QoS is supported.
- `Subscriber` is no longer trivially `Clone + PartialEq` (the channel sender is cloneable but semantically tricky) — router tests must account for this.
- `remove_client` must continue to remove all subscriptions of the client; the sender becomes obsolete anyway when `rx` is dropped in the Connection Task.
- Channel capacity is a tuning knob that can be made configurable later.
