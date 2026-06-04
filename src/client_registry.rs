//! ClientRegistry: einzige Wahrheit darueber, welche Clients aktuell verbunden
//! sind. Kapselt das Takeover-Protokoll bei Client-ID-Kollision (ADR-0004).

use std::collections::HashMap;
use std::sync::Mutex;

use bytes::Bytes;
use tokio::sync::mpsc;

/// Nachricht an einen Connection Task ueber dessen mpsc-Channel.
#[derive(Debug, Clone)]
pub enum ConnectionCommand {
    /// Vorgerendertes PUBLISH-Frame vom Router-Fanout.
    DeliverFrame(Bytes),
    /// Serverseitiges Beenden (z.B. Takeover bei Client-ID-Kollision).
    Disconnect,
}

/// Registry der aktuell verbundenen Clients.
///
/// Trennt sich vom `TopicRouter`, weil ein Client auch ohne Subscriptions
/// verbunden sein kann.
#[derive(Default)]
pub struct ClientRegistry {
    inner: Mutex<HashMap<String, mpsc::Sender<ConnectionCommand>>>,
}

impl ClientRegistry {
    /// Erzeugt eine leere Registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Atomisches Insert-or-Replace. Gibt den vorher unter `client_id`
    /// eingetragenen Sender zurueck (falls vorhanden), damit der Caller die
    /// alte Session kicken kann.
    pub fn swap_in(
        &self,
        client_id: &str,
        new_tx: mpsc::Sender<ConnectionCommand>,
    ) -> Option<mpsc::Sender<ConnectionCommand>> {
        let mut map = self.inner.lock().expect("registry mutex poisoned");
        map.insert(client_id.to_string(), new_tx)
    }

    /// Entfernt den Eintrag fuer `client_id` (idempotent).
    ///
    /// Wichtig: entfernt nur, wenn der gespeicherte Sender mit dem uebergebenen
    /// `own_tx` identisch ist. Das verhindert, dass ein verdraengter alter Task
    /// beim Aufraeumen den frisch eingetragenen Nachfolger ausloescht.
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
