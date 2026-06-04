//! ClientRegistry: single source of truth about which clients are currently
//! connected. Encapsulates the takeover protocol on Client ID collision (ADR-0004).

use std::collections::HashMap;
use std::sync::Mutex;

use bytes::Bytes;
use tokio::sync::mpsc;

/// Message to a Connection Task via its mpsc channel.
#[derive(Debug, Clone)]
pub enum ConnectionCommand {
    /// Pre-rendered PUBLISH frame from the router fanout.
    DeliverFrame(Bytes),
    /// Server-initiated termination (e.g. takeover on Client ID collision).
    Disconnect,
}

/// Registry of currently connected clients.
///
/// Kept separate from `TopicRouter` because a client can be connected
/// without having any subscriptions.
#[derive(Default)]
pub struct ClientRegistry {
    inner: Mutex<HashMap<String, mpsc::Sender<ConnectionCommand>>>,
}

impl ClientRegistry {
    /// Creates an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Atomic insert-or-replace. Returns the sender previously stored under
    /// `client_id` (if any) so that the caller can kick the old session.
    pub fn swap_in(
        &self,
        client_id: &str,
        new_tx: mpsc::Sender<ConnectionCommand>,
    ) -> Option<mpsc::Sender<ConnectionCommand>> {
        let mut map = self.inner.lock().expect("registry mutex poisoned");
        map.insert(client_id.to_string(), new_tx)
    }

    /// Removes the entry for `client_id` (idempotent).
    ///
    /// Important: only removes if the stored sender is identical to the
    /// provided `own_tx`. This prevents a displaced old task from deleting
    /// the freshly registered successor during cleanup.
    pub fn remove_if_owner(&self, client_id: &str, own_tx: &mpsc::Sender<ConnectionCommand>) {
        let mut map = self.inner.lock().expect("registry mutex poisoned");
        if let Some(stored) = map.get(client_id)
            && stored.same_channel(own_tx)
        {
            map.remove(client_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_channel() -> (
        mpsc::Sender<ConnectionCommand>,
        mpsc::Receiver<ConnectionCommand>,
    ) {
        mpsc::channel(4)
    }

    #[test]
    fn swap_in_empty_slot_returns_none() {
        let reg = ClientRegistry::new();
        let (tx, _rx) = mk_channel();
        assert!(reg.swap_in("c1", tx).is_none());
    }

    #[test]
    fn swap_in_existing_returns_previous_sender() {
        let reg = ClientRegistry::new();
        let (tx1, _rx1) = mk_channel();
        let (tx2, _rx2) = mk_channel();
        reg.swap_in("c1", tx1.clone());
        let old = reg.swap_in("c1", tx2).expect("old sender expected");
        assert!(old.same_channel(&tx1));
    }

    #[test]
    fn remove_if_owner_skips_when_not_owner() {
        let reg = ClientRegistry::new();
        let (tx1, _rx1) = mk_channel();
        let (tx2, _rx2) = mk_channel();
        reg.swap_in("c1", tx1.clone());
        // tx2 is not the owner; remove must be a no-op
        reg.remove_if_owner("c1", &tx2);
        // owner remove succeeds
        reg.remove_if_owner("c1", &tx1);
        // After owner remove, slot is empty
        let (tx3, _rx3) = mk_channel();
        assert!(reg.swap_in("c1", tx3).is_none());
    }
}
