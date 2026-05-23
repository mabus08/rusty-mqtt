use std::collections::HashMap;

pub struct TopicRouter {
    subscriptions: HashMap<u16, String>, // packet_id -> topic_filter for MVP
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

    pub fn subscribe(&mut self, packet_id: u16, filters: Vec<(String, u8)>) {
        for (topic_filter, _) in &filters {
            self.subscriptions
                .insert(packet_id, topic_filter.clone());
        }
    }

    pub fn get_subscriptions_for_topic(&self, topic: &str) -> Vec<u16> {
        let mut result = Vec::new();

        for (packet_id, filter) in &self.subscriptions {
            if self.topic_matches(filter.as_str(), topic) {
                result.push(*packet_id);
                break;
            }
        }

        result
    }

    fn topic_matches(&self, filter: &str, topic: &str) -> bool {
        let p_bytes = filter.as_bytes();
        let t_bytes = topic.as_bytes();

        if p_bytes == t_bytes {
            return true;
        }

        if !p_bytes.contains(&b'+') && !p_bytes.contains(&b'#') {
            return false;
        }

        let mut ip: usize = 0;
        let mut it: usize = 0;

        loop {
            if ip >= p_bytes.len() && it >= t_bytes.len() {
                return true;
            }
            if ip >= p_bytes.len() || it >= t_bytes.len() {
                return false;
            }

            match *p_bytes.get(ip).unwrap_or(&0) {
                b'+' => {
                    ip += 1;
                    it += 1;
                }
                b'#' => return true,
                _ => {
                    if p_bytes[ip] != t_bytes[it] {
                        return false;
                    }
                    ip += 1;
                    it += 1;
                }
            }
        }
    }
}

pub fn create_router() -> TopicRouter {
    TopicRouter::new()
}
