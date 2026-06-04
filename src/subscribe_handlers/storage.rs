use std::collections::HashMap;

use tokio::sync::mpsc;

use crate::client_registry::ConnectionCommand;

/// A single subscriber for a topic.
///
/// Also holds an `mpsc::Sender` through which pre-rendered PUBLISH frames
/// are pushed by the router fanout to the responsible Connection Task.
#[derive(Debug, Clone)]
pub struct Subscriber {
    /// Client ID of the subscriber.
    pub client_id: String,
    /// QoS value granted by the broker (`granted_qos`).
    pub qos: u8,
    /// Channel sender to the Connection Task; pre-rendered frames go here.
    pub tx: mpsc::Sender<ConnectionCommand>,
}

/// Global topic router: stores subscriptions by topic filter.
/// Key = Topic Filter string, Value = list of subscribers.
pub struct TopicRouter {
    subscriptions: HashMap<String, Vec<Subscriber>>,
}

impl Default for TopicRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl TopicRouter {
    pub fn new() -> Self {
        Self {
            subscriptions: HashMap::new(),
        }
    }

    /// Registers subscriptions for a client.
    /// On duplicates (same client_id + same topic filter) the QoS is updated (replace semantics).
    /// Returns the granted QoS values (in the order of the provided filters).
    pub fn subscribe(
        &mut self,
        client_id: &str,
        filters: &[(String, u8)],
        tx: &mpsc::Sender<ConnectionCommand>,
    ) -> Vec<u8> {
        let mut granted_qos = Vec::with_capacity(filters.len());

        for (topic_filter, qos) in filters {
            let subscribers = self.subscriptions.entry(topic_filter.clone()).or_default();

            // Replace semantics: update existing entry for the same client_id
            if let Some(existing) = subscribers.iter_mut().find(|s| s.client_id == client_id) {
                existing.qos = *qos;
                existing.tx = tx.clone();
            } else {
                subscribers.push(Subscriber {
                    client_id: client_id.to_string(),
                    qos: *qos,
                    tx: tx.clone(),
                });
            }

            granted_qos.push(*qos);
        }

        granted_qos
    }

    /// Returns all subscribers whose topic filter matches the given topic.
    pub fn get_subscribers_for_topic(&self, topic: &str) -> Vec<&Subscriber> {
        let mut result = Vec::new();

        for (filter, subscribers) in &self.subscriptions {
            if topic_matches(filter, topic) {
                result.extend(subscribers.iter());
            }
        }

        result
    }

    /// Removes subscriptions of a client for the specified topic filters.
    /// Filters that are not present are silently ignored (no-op).
    pub fn unsubscribe(&mut self, client_id: &str, filters: &[String]) {
        for filter in filters {
            if let Some(subs) = self.subscriptions.get_mut(filter) {
                subs.retain(|s| s.client_id != client_id);
            }
        }
        self.subscriptions.retain(|_, subs| !subs.is_empty());
    }

    /// Removes all subscriptions of a client (e.g. on disconnect).
    pub fn remove_client(&mut self, client_id: &str) {
        for subscribers in self.subscriptions.values_mut() {
            subscribers.retain(|s| s.client_id != client_id);
        }
        // Clean up empty topic entries
        self.subscriptions.retain(|_, subs| !subs.is_empty());
    }
}

/// MQTT topic wildcard matching per MQTT 3.1.1 spec.
///
/// - `+` matches exactly one topic level (everything between two `/`)
/// - `#` matches zero or more remaining levels (must appear at the end)
/// - Without wildcards: exact string comparison
pub fn topic_matches(filter: &str, topic: &str) -> bool {
    if filter == topic {
        return true;
    }

    let filter_levels: Vec<&str> = filter.split('/').collect();
    let topic_levels: Vec<&str> = topic.split('/').collect();

    let mut fi = 0;
    let mut ti = 0;

    while fi < filter_levels.len() {
        let f_level = filter_levels[fi];

        if f_level == "#" {
            // # matches all remaining levels (including zero)
            return true;
        }

        if ti >= topic_levels.len() {
            // Topic has fewer levels than the filter (without #)
            return false;
        }

        if f_level != "+" && f_level != topic_levels[ti] {
            return false;
        }

        // + matches exactly this one level, or it was an exact match
        fi += 1;
        ti += 1;
    }

    // Both must be at the end simultaneously
    fi == filter_levels.len() && ti == topic_levels.len()
}

/// Creates a new empty [`TopicRouter`] (convenience function).
pub fn create_router() -> TopicRouter {
    TopicRouter::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_tx() -> mpsc::Sender<ConnectionCommand> {
        let (tx, _rx) = mpsc::channel(1);
        tx
    }

    // --- topic_matches tests ---

    #[test]
    fn test_exact_match() {
        assert!(topic_matches("home/temp", "home/temp"));
        assert!(!topic_matches("home/temp", "home/humidity"));
    }

    #[test]
    fn test_plus_single_level() {
        assert!(topic_matches("home/+/temp", "home/kitchen/temp"));
        assert!(topic_matches("home/+/temp", "home/livingroom/temp"));
        assert!(!topic_matches("home/+/temp", "home/temp"));
        assert!(!topic_matches("home/+/temp", "home/a/b/temp"));
    }

    #[test]
    fn test_hash_multi_level() {
        assert!(topic_matches("#", "anything"));
        assert!(topic_matches("#", "a/b/c/d"));
        assert!(topic_matches("home/#", "home/kitchen/temp"));
        assert!(topic_matches("home/#", "home"));
        assert!(topic_matches("home/#", "home/a/b/c"));
    }

    #[test]
    fn test_hash_alone_matches_everything() {
        assert!(topic_matches("#", ""));
        assert!(topic_matches("#", "a"));
        assert!(topic_matches("#", "a/b/c"));
    }

    #[test]
    fn test_plus_and_hash_combined() {
        assert!(topic_matches("+/news/#", "sport/news/articles/123"));
        assert!(topic_matches("+/news/#", "tech/news"));
        assert!(!topic_matches("+/news/#", "sport/articles/123"));
    }

    #[test]
    fn test_leading_slash() {
        assert!(topic_matches("/news/#", "/news/articles/123"));
        assert!(topic_matches("/+/news", "/sport/news"));
        assert!(!topic_matches("/news/#", "news/articles"));
    }

    // --- TopicRouter tests ---

    #[test]
    fn test_subscribe_and_lookup() {
        let mut router = TopicRouter::new();
        let granted = router.subscribe("client-1", &[("home/temp".to_string(), 0)], &dummy_tx());
        assert_eq!(granted, vec![0]);

        let subs = router.get_subscribers_for_topic("home/temp");
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].client_id, "client-1");
        assert_eq!(subs[0].qos, 0);
    }

    #[test]
    fn test_subscribe_replace_semantics() {
        let mut router = TopicRouter::new();
        router.subscribe("client-1", &[("home/temp".to_string(), 0)], &dummy_tx());
        router.subscribe("client-1", &[("home/temp".to_string(), 1)], &dummy_tx());

        let subs = router.get_subscribers_for_topic("home/temp");
        assert_eq!(subs.len(), 1, "Should replace, not duplicate");
        assert_eq!(subs[0].qos, 1, "QoS should be updated to 1");
    }

    #[test]
    fn test_multiple_clients_same_topic() {
        let mut router = TopicRouter::new();
        router.subscribe("client-1", &[("home/temp".to_string(), 0)], &dummy_tx());
        router.subscribe("client-2", &[("home/temp".to_string(), 1)], &dummy_tx());

        let subs = router.get_subscribers_for_topic("home/temp");
        assert_eq!(subs.len(), 2);
    }

    #[test]
    fn test_wildcard_subscription_lookup() {
        let mut router = TopicRouter::new();
        router.subscribe("client-1", &[("home/+/temp".to_string(), 0)], &dummy_tx());

        let subs = router.get_subscribers_for_topic("home/kitchen/temp");
        assert_eq!(subs.len(), 1);

        let subs = router.get_subscribers_for_topic("home/temp");
        assert_eq!(subs.len(), 0);
    }

    #[test]
    fn test_remove_client() {
        let mut router = TopicRouter::new();
        router.subscribe("client-1", &[("home/temp".to_string(), 0)], &dummy_tx());
        router.subscribe("client-2", &[("home/temp".to_string(), 1)], &dummy_tx());

        router.remove_client("client-1");

        let subs = router.get_subscribers_for_topic("home/temp");
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].client_id, "client-2");
    }
}
