# ClientRegistry and Takeover Protocol on Client ID Collision

A `ClientRegistry` (`HashMap<ClientId, mpsc::Sender<ConnectionCommand>>`) is maintained alongside the `TopicRouter` and is the single source of truth about which clients are currently connected. The mpsc channel to the Connection Task carries the `ConnectionCommand` enum with two variants: `DeliverFrame(Bytes)` (PUBLISH bytes forwarded by the router, see ADR-0002) and `Disconnect` (server-initiated termination).

When a second CONNECT arrives with the same Client ID, **per MQTT 3.1.1 §3.1.4** the existing connection must be terminated. This happens synchronously in the context of the **new** Connection Task **before** it registers itself in the registry or router:

1. `let old = registry.swap_in(client_id, new_tx)`
2. If present: `old.send(ConnectionCommand::Disconnect).await; old.closed().await;`
3. (Only then) register own subscriptions in the `TopicRouter`

Step 2 waits for the old channel to close — which happens as soon as the old Connection Task exits its `select!` loop and drops its receiver. This guarantees that the old task has completed its `router.remove_client` cleanup before the new task writes anything to the router.

## Considered Options

- **Extended command channel + synchronous kick with `closed().await` (chosen)** — one synchronisation primitive per client, unified cleanup path for "kicked", "self-DISCONNECT", "TCP error" and "read timeout", deterministic elimination of the takeover race.
- **Separate `oneshot::Sender<()>` as kick handle** — second primitive, edge cases when the oneshot has already been consumed by a self-disconnect, no shared code path with regular frame reception.
- **`tokio::task::JoinHandle::abort()`** — tears down the old task mid-`.await`, skipping the `remove_client` cleanup; leaves dangling subscriptions in the router. Rejected.
- **A combined structure** instead of two registries — would force TopicRouter entries for clients without subscriptions, or add a "connected but no subscriptions" special case. Breaks the glossary definition "TopicRouter = who is listening".
- **Kick-and-forget + epoch counter per client** — avoids `closed().await`, but adds an epoch validation per frame and makes the cleanup path non-local. More code, less determinism.

## Consequences

- `MqttServer` holds two `Arc`s: `Arc<Mutex<TopicRouter>>` and `Arc<ClientRegistry>` (`ClientRegistry` encapsulates its own mutex). Both are passed to each Connection Task.
- The Connection Task is the only place that calls `router.remove_client(&self.client_id)` and `registry.remove(&self.client_id)` — never from outside. This gives exactly one cleanup path.
- `mpsc::Sender::closed()` is part of the stable Tokio API; no nightly, no workaround.
- The kick path serialises: the new CONNECT only responds with CONNACK after the old task has fully terminated. In the pathological case (old task stuck in an `await`) this may take time. Acceptable for the MVP; if relevant, `tokio::time::timeout` around step 2 would be the next escalation.
- Before CONNECT (i.e. before `client_id` is known) the Connection Task has **no** registry entry and **no** mpsc channel. The channel is created in the CONNECT handler, not in `accept()`.
