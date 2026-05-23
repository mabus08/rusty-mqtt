use std::collections::HashMap;

/// Ein einzelner Subscriber fuer ein Topic.
#[derive(Debug, Clone, PartialEq)]
pub struct Subscriber {
    pub client_id: String,
    pub qos: u8,
}

/// Globaler Topic Router: speichert Subscriptions nach Topic Filter.
/// Key = Topic Filter String, Value = Liste der Subscriber.
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

    /// Registriert Subscriptions fuer einen Client.
    /// Bei Duplikaten (gleiche client_id + gleicher Topic Filter) wird der QoS aktualisiert (Replace-Semantik).
    /// Gibt die granted QoS-Werte zurueck (in der Reihenfolge der uebergebenen Filter).
    pub fn subscribe(&mut self, client_id: &str, filters: &[(String, u8)]) -> Vec<u8> {
        let mut granted_qos = Vec::with_capacity(filters.len());

        for (topic_filter, qos) in filters {
            let subscribers = self.subscriptions.entry(topic_filter.clone()).or_default();

            // Replace-Semantik: existierenden Eintrag fuer gleiche client_id aktualisieren
            if let Some(existing) = subscribers.iter_mut().find(|s| s.client_id == client_id) {
                existing.qos = *qos;
            } else {
                subscribers.push(Subscriber {
                    client_id: client_id.to_string(),
                    qos: *qos,
                });
            }

            granted_qos.push(*qos);
        }

        granted_qos
    }

    /// Gibt alle Subscriber zurueck, deren Topic Filter auf das gegebene Topic matchen.
    pub fn get_subscribers_for_topic(&self, topic: &str) -> Vec<&Subscriber> {
        let mut result = Vec::new();

        for (filter, subscribers) in &self.subscriptions {
            if topic_matches(filter, topic) {
                result.extend(subscribers.iter());
            }
        }

        result
    }

    /// Entfernt alle Subscriptions eines Clients (z.B. bei Disconnect).
    pub fn remove_client(&mut self, client_id: &str) {
        for subscribers in self.subscriptions.values_mut() {
            subscribers.retain(|s| s.client_id != client_id);
        }
        // Leere Topic-Eintraege aufraeumen
        self.subscriptions.retain(|_, subs| !subs.is_empty());
    }
}

/// MQTT Topic Wildcard Matching nach MQTT 3.1.1 Spec.
///
/// - `+` matcht exakt ein Topic-Level (alles zwischen zwei `/`)
/// - `#` matcht null oder mehr verbleibende Levels (muss am Ende stehen)
/// - Ohne Wildcards: exakter String-Vergleich
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
            // # matcht alle verbleibenden Levels (inklusive null)
            return true;
        }

        if ti >= topic_levels.len() {
            // Topic hat weniger Levels als der Filter (ohne #)
            return false;
        }

        if f_level != "+" && f_level != topic_levels[ti] {
            return false;
        }

        // + matcht genau dieses eine Level, oder es war ein exakter Match
        fi += 1;
        ti += 1;
    }

    // Beide muessen gleichzeitig am Ende sein
    fi == filter_levels.len() && ti == topic_levels.len()
}

pub fn create_router() -> TopicRouter {
    TopicRouter::new()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let granted = router.subscribe("client-1", &[("home/temp".to_string(), 0)]);
        assert_eq!(granted, vec![0]);

        let subs = router.get_subscribers_for_topic("home/temp");
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].client_id, "client-1");
        assert_eq!(subs[0].qos, 0);
    }

    #[test]
    fn test_subscribe_replace_semantics() {
        let mut router = TopicRouter::new();
        router.subscribe("client-1", &[("home/temp".to_string(), 0)]);
        router.subscribe("client-1", &[("home/temp".to_string(), 1)]);

        let subs = router.get_subscribers_for_topic("home/temp");
        assert_eq!(subs.len(), 1, "Should replace, not duplicate");
        assert_eq!(subs[0].qos, 1, "QoS should be updated to 1");
    }

    #[test]
    fn test_multiple_clients_same_topic() {
        let mut router = TopicRouter::new();
        router.subscribe("client-1", &[("home/temp".to_string(), 0)]);
        router.subscribe("client-2", &[("home/temp".to_string(), 1)]);

        let subs = router.get_subscribers_for_topic("home/temp");
        assert_eq!(subs.len(), 2);
    }

    #[test]
    fn test_wildcard_subscription_lookup() {
        let mut router = TopicRouter::new();
        router.subscribe("client-1", &[("home/+/temp".to_string(), 0)]);

        let subs = router.get_subscribers_for_topic("home/kitchen/temp");
        assert_eq!(subs.len(), 1);

        let subs = router.get_subscribers_for_topic("home/temp");
        assert_eq!(subs.len(), 0);
    }

    #[test]
    fn test_remove_client() {
        let mut router = TopicRouter::new();
        router.subscribe("client-1", &[("home/temp".to_string(), 0)]);
        router.subscribe("client-2", &[("home/temp".to_string(), 1)]);

        router.remove_client("client-1");

        let subs = router.get_subscribers_for_topic("home/temp");
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].client_id, "client-2");
    }
}
