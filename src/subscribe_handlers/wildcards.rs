use std::sync::Arc;
use std::collections::HashMap;

pub struct TopicRouter {
    subscriptions: Arc<HashMap<String, Vec<Subscription>>>,
}

#[derive(Debug, Clone)]
pub struct Subscription {
    pub packet_id: u16,
    pub qos: u8,
    pub topic_filter: String,
}

impl TopicRouter {
    pub fn new() -> Self {
        let subscriptions = Arc::new(HashMap::new());
        Self { subscriptions }
    }

    #[inline]
    pub fn register_subscription(&self, client_id: &str, packet_id: u16, qos: u8, topic_filter: &str) {
        *self.subscriptions.entry(client_id.to_string()).or_insert_with(Vec::new).push(Subscription {
            packet_id,
            qos,
            topic_filter: topic_filter.to_string(),
        });
    }

    pub fn add_subscription(&mut self, client_id: String, subscription: Subscription) {
        self.subscriptions.entry(client_id).or_insert_with(Vec::new).push(subscription);
    }

    #[inline]
    pub fn get_subscriptions(&self, client_id: &str) -> Option<Vec<Subscription>> {
        self.subscriptions.get(client_id).cloned()
    }

    /// Returns all subscriptions for a given client ID that match the topic
    pub fn get_subscriptions_for_topic(&self, client_id: &str, incoming_topic: &str) -> Vec<Subscription> {
        self.subscriptions.get(client_id).map(|v| {
            v.iter()
                .filter(|s| topic_match_wildcard(&s.topic_filter, incoming_topic))
                .cloned()
                .collect()
        }).unwrap_or_default()
    }

    /// Returns a count of subscriptions for a client ID that match the topic
    pub fn matching_subscription_count(&self, client_id: &str, incoming_topic: &str) -> usize {
        self.subscriptions.get(client_id).map(|v| {
            v.iter().filter(|s| topic_match_wildcard(&s.topic_filter, incoming_topic)).count()
        }).unwrap_or(0)
    }

    /// Checks if this pattern matches the topic using MQTT standard wildcards:
    /// - '+' : Matches exactly one level in topic path (e.g., /home/user/+/news)
    /// - '#' : Matches any sequence of levels at END of pattern only
    pub fn topic_matches_pattern(pattern: &str, incoming_topic: &str) -> bool {
        if pattern == incoming_topic {
            return true;
        }

        // No wildcards? Must be exact match which is already handled
        if !pattern.contains('+') && !pattern.contains('#') {
            return false;
        }

        topic_match_wildcard(pattern, incoming_topic)
    }
}

/// Returns all subscribers that could receive a message for this topic
pub struct WildcardMatcher {}

impl TopicRouter {
    /// Convenience method to check matching subscriptions for routing
    pub fn match_and_return_subs(&self, client_id: &str, topic: &str) -> Vec<Subscription> {
        self.get_subscriptions_for_topic(client_id, topic)
    }
}

/// MQTT Standard Topic Wildcard Matching Logic
fn topic_match_wildcard(pattern: &str, incoming_topic: &str) -> bool {
    if pattern == incoming_topic {
        return true;
    }

    if !pattern.contains('+') && !pattern.contains('#') {
        return false;
    }

    let p_bytes = pattern.as_bytes();
    let t_bytes = incoming_topic.as_bytes();

    // '#' must come after '+' (MQTT spec violation but we handle gracefully)
    let last_plus_or_hash = p_bytes.iter().rposition(|&b| b == b'+' || b == b'#').unwrap_or(0);
    
    if let Some(hash_pos) = p_bytes.windows(last_plus_or_hash as usize + 1).find(|chunk| chunk.contains(&b'#')) {
        let hash_char_pos = p_bytes.iter().position(|&b| b == b'#').unwrap();
        // Check that nothing after # in pattern (MQTT spec requirement)
    }

    // Match character by character until we hit a wildcard or end of topic  
    let mut i_pattern = 0;
    let mut i_topic = 0;

    loop {
        if i_pattern >= pattern.len() && i_topic >= incoming_topic.len() {
            return true; // Both finished - successful match
        }
        if i_pattern >= pattern.len() || i_topic >= incoming_topic.len() {
            return false; // One finished but not both
        }

        let pattern_byte = p_bytes[i_pattern];
        let topic_word = t_bytes[i_topic];

        if pattern_byte == '+' {
            i_pattern += 1;
            i_topic += 1;
        } else if pattern_byte == '#' {
            // '#' matches all remaining levels
            return true;
        } else if pattern_byte != topic_word {
            return false;
        }

        i_pattern += 1;
        i_topic += 1;
    }
}

/// Helper to create a new empty router for testing
pub fn get_empty_router() -> TopicRouter {
    TopicRouter::new()
}
