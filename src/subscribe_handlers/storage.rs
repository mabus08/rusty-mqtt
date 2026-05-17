use std::sync::Arc;
use std::collections::HashMap;

/// Information eines Subscriber für message routing
#[derive(Debug, Clone)]
pub struct SubscriberSubscription {
    /// Unique packet identifier for SUBSCRIBE/PUBACK correlation (2 byte)  
    pub packet_id: u16,
    
    /// QoS level for message delivery  
    pub qos: u8, // 0 = AtMostOnce, 1 = AtLeastOnce
    
    /// Topic filter pattern that this client subscribed to
    pub topic_filter: String,
}

/// Topic Router mit subscriber storage (client-centric)  
/// Speichert: client_id → Liste von SubscriberSubscription structs
pub struct TopicRouter {
    /// Speicherung: client_id → Vec<SubscriberSubscription>
    subscriptions: Arc<HashMap<String, Vec<SubscriberSubscription>>>,
}

impl TopicRouter {
    /// Erstellt neuen Topic Router mit leerem storage  
    pub fn new() -> Self {
        let subscriptions = Arc::new(HashMap::new());
        Self { subscriptions }
    }

    /// Registriert einen neuen Subscriber-Record  
    /// 
    /// # Arguments
    /// - `client_id`: MQTT Client Identifier (Connect Packet)
    /// - `topic_filter`: Topic Muster wie "/home" oder "/news/#"
    /// - `packet_id`: SUBSCRIBE packet identifier (2 bytes)  
    /// - `qos`: QoS level (0 oder 1)
    pub fn register_subscription(&self, client_id: &str, topic_filter: &str, 
                                packet_id: u16, qos: u8) {
        let subscription = SubscriberSubscription {
            packet_id,
            qos,
            topic_filter: topic_filter.to_string(),
        };

        self.subscriptions
            .entry(client_id.to_string())
            .or_insert_with(Vec::new)
            .push(subscription);
    }

    /// Rückgibt alle Subscribtions für einen Client-Topics (optional mit filtering)  
    pub fn get_subscriptions(&self, client_id: &str) -> Option<Vec<SubscriberSubscription>> {
        self.subscriptions.get(client_id).cloned()
    }

    /// Zählt die Anzahl der Subscriptons für einen Client
    pub fn subscription_count(&self, client_id: &str) -> usize {
        *self.subscriptions.get(client_id).map(|v| v.len()).unwrap_or(&0)
    }

    /// Gibt true falls client subscribtions existieren  
    pub fn has_subscriptions(&self, client_id: &str) -> bool {
        self.subscriptions.contains_key(client_id) && !self.subscription_count(client_id) == 0 
    }
    
    /// Iteriert über alle Subscription für ein Topic (topic filtering später AP2+)
    pub fn topics_for_client(&self, client_id: &str) -> Vec<SubscriberSubscription> {
        self.subscriptions.get(client_id).cloned().unwrap_or_default()
    }
}

/// Test-Funktion zum Reset (falls in Tests gebraucht)  
pub fn create_router() -> TopicRouter {
    TopicRouter::new()
}
