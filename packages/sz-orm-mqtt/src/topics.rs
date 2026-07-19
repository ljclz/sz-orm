use crate::error::MqttError;

#[derive(Debug, Clone)]
pub struct TopicFilter {
    pattern: String,
    levels: Vec<String>,
}

impl TopicFilter {
    pub fn new(pattern: impl Into<String>) -> Result<Self, MqttError> {
        let pattern = pattern.into();
        if pattern.is_empty() {
            return Err(MqttError::Topic("Topic filter cannot be empty".to_string()));
        }
        let levels: Vec<String> = pattern.split('/').map(|s| s.to_string()).collect();
        for (i, level) in levels.iter().enumerate() {
            if level.contains('#') && (level.len() != 1 || i != levels.len() - 1) {
                return Err(MqttError::Topic(format!(
                    "Invalid topic filter: '#' must occupy the entire last level: {}",
                    pattern
                )));
            }
            if level.contains('+') && level.len() != 1 {
                return Err(MqttError::Topic(format!(
                    "Invalid topic filter: '+' must occupy an entire level: {}",
                    pattern
                )));
            }
        }
        Ok(Self { pattern, levels })
    }

    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    pub fn levels(&self) -> &[String] {
        &self.levels
    }

    pub fn has_wildcard(&self) -> bool {
        self.pattern.contains('#') || self.pattern.contains('+')
    }

    pub fn matches(&self, topic: &str) -> bool {
        topic_matches(topic, &self.pattern)
    }
}

impl From<&str> for TopicFilter {
    fn from(s: &str) -> Self {
        TopicFilter::new(s).unwrap_or_else(|_| TopicFilter {
            pattern: s.to_string(),
            levels: s.split('/').map(|l| l.to_string()).collect(),
        })
    }
}

impl From<String> for TopicFilter {
    fn from(s: String) -> Self {
        TopicFilter::from(s.as_str())
    }
}

pub fn topic_matches(topic: &str, filter: &str) -> bool {
    if topic.is_empty() || filter.is_empty() {
        return false;
    }

    let topic_levels: Vec<&str> = topic.split('/').collect();
    let filter_levels: Vec<&str> = filter.split('/').collect();

    let mut topic_idx = 0;
    let mut filter_idx = 0;

    while filter_idx < filter_levels.len() {
        let filter_part = filter_levels[filter_idx];

        if filter_part == "#" {
            return true;
        }

        if filter_part == "+" {
            if topic_idx >= topic_levels.len() {
                return false;
            }
            topic_idx += 1;
            filter_idx += 1;
            continue;
        }

        if topic_idx >= topic_levels.len() {
            return false;
        }

        if filter_part != topic_levels[topic_idx] {
            return false;
        }

        topic_idx += 1;
        filter_idx += 1;
    }

    topic_idx == topic_levels.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topic_filter_new_valid() {
        let filter = TopicFilter::new("home/living/temperature").unwrap();
        assert_eq!(filter.pattern(), "home/living/temperature");
        assert_eq!(filter.levels().len(), 3);
        assert!(!filter.has_wildcard());
    }

    #[test]
    fn test_topic_filter_new_wildcard_hash() {
        let filter = TopicFilter::new("home/#").unwrap();
        assert_eq!(filter.pattern(), "home/#");
        assert!(filter.has_wildcard());
    }

    #[test]
    fn test_topic_filter_new_wildcard_plus() {
        let filter = TopicFilter::new("home/+/temperature").unwrap();
        assert!(filter.has_wildcard());
    }

    #[test]
    fn test_topic_filter_new_invalid_hash_in_middle() {
        let result = TopicFilter::new("home/#/temperature");
        assert!(result.is_err());
    }

    #[test]
    fn test_topic_filter_new_invalid_hash_with_suffix() {
        let result = TopicFilter::new("home/ab#");
        assert!(result.is_err());
    }

    #[test]
    fn test_topic_filter_new_invalid_plus_with_suffix() {
        let result = TopicFilter::new("home/ab+/temperature");
        assert!(result.is_err());
    }

    #[test]
    fn test_topic_filter_new_empty() {
        let result = TopicFilter::new("");
        assert!(result.is_err());
    }

    #[test]
    fn test_topic_matches_exact() {
        assert!(topic_matches("home/living/temp", "home/living/temp"));
        assert!(!topic_matches("home/living/temp", "home/kitchen/temp"));
    }

    #[test]
    fn test_topic_matches_multi_level_wildcard() {
        assert!(topic_matches("home/living/temp", "home/#"));
        assert!(topic_matches("home/living/temp/extra", "home/#"));
        assert!(topic_matches("home", "home/#"));
        assert!(!topic_matches("office/temp", "home/#"));
    }

    #[test]
    fn test_topic_matches_single_level_wildcard() {
        assert!(topic_matches("home/living/temp", "home/+/temp"));
        assert!(topic_matches("home/kitchen/temp", "home/+/temp"));
        assert!(!topic_matches("home/living/extra/temp", "home/+/temp"));
        assert!(!topic_matches("home/temp", "home/+/temp"));
    }

    #[test]
    fn test_topic_matches_mixed_wildcards() {
        assert!(topic_matches("home/living/temp/extra", "home/+/+/#"));
        assert!(topic_matches("home/living/temp", "home/+/temp"));
        assert!(topic_matches("home/living/temp/extra/deep", "home/#"));
    }

    #[test]
    fn test_topic_matches_root_level() {
        assert!(topic_matches("#", "#"));
        assert!(topic_matches("anything", "#"));
        assert!(topic_matches("anything/here", "#"));
    }

    #[test]
    fn test_topic_matches_empty() {
        assert!(!topic_matches("", "home/#"));
        assert!(!topic_matches("home", ""));
    }

    #[test]
    fn test_topic_filter_matches_method() {
        let filter = TopicFilter::new("home/+/temperature").unwrap();
        assert!(filter.matches("home/living/temperature"));
        assert!(filter.matches("home/kitchen/temperature"));
        assert!(!filter.matches("home/living/humidity"));
    }

    #[test]
    fn test_topic_filter_from_str() {
        let filter: TopicFilter = "sports/tennis/#".into();
        assert_eq!(filter.pattern(), "sports/tennis/#");
        assert!(filter.has_wildcard());
    }

    #[test]
    fn test_topic_matches_plus_at_end() {
        assert!(topic_matches("home/living", "home/+"));
        assert!(!topic_matches("home/living/extra", "home/+"));
    }

    #[test]
    fn test_topic_matches_trailing_slash() {
        assert!(topic_matches("home/", "home/+"));
        assert!(topic_matches("home/living/", "home/living/"));
    }
}
