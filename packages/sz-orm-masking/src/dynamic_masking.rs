//! v6.7.0 动态脱敏策略热更新。

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MaskingRule {
    MaskMiddle,
    MaskAll,
    Hash,
    Truncate(usize),
}

pub struct DynamicMaskingConfig {
    rules: Arc<RwLock<HashMap<String, MaskingRule>>>,
}

impl DynamicMaskingConfig {
    pub fn new() -> Self {
        Self {
            rules: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn set_rule(&self, field: &str, rule: MaskingRule) {
        self.rules.write().unwrap().insert(field.to_string(), rule);
    }

    pub fn hot_update(&self, new_rules: HashMap<String, MaskingRule>) {
        let mut rules = self.rules.write().unwrap();
        *rules = new_rules;
    }

    pub fn get_rule(&self, field: &str) -> Option<MaskingRule> {
        self.rules.read().unwrap().get(field).cloned()
    }

    pub fn snapshot(&self) -> HashMap<String, MaskingRule> {
        self.rules.read().unwrap().clone()
    }
}

impl Default for DynamicMaskingConfig {
    fn default() -> Self {
        Self::new()
    }
}

pub fn apply_mask(value: &str, rule: &MaskingRule) -> String {
    match rule {
        MaskingRule::MaskAll => "*".repeat(value.chars().count()),
        MaskingRule::MaskMiddle => mask_middle(value),
        MaskingRule::Hash => {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            value.hash(&mut hasher);
            format!("{:016x}", hasher.finish())
        }
        MaskingRule::Truncate(n) => {
            let chars: Vec<char> = value.chars().take(*n).collect();
            chars.into_iter().collect()
        }
    }
}

fn mask_middle(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    let len = chars.len();
    if len <= 4 {
        return "*".repeat(len);
    }
    let prefix = len / 3;
    let suffix = len / 3;
    let mut result = String::new();
    for (i, ch) in chars.iter().enumerate() {
        if i < prefix || i >= len - suffix {
            result.push(*ch);
        } else {
            result.push('*');
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_middle_phone() {
        let masked = apply_mask("13800001234", &MaskingRule::MaskMiddle);
        assert!(masked.starts_with("13"));
        assert!(masked.ends_with("34"));
        assert!(masked.contains('*'));
    }

    #[test]
    fn mask_all_replaces_everything() {
        let masked = apply_mask("secret", &MaskingRule::MaskAll);
        assert_eq!(masked, "******");
    }

    #[test]
    fn hash_produces_consistent_output() {
        let h1 = apply_mask("test", &MaskingRule::Hash);
        let h2 = apply_mask("test", &MaskingRule::Hash);
        assert_eq!(h1, h2);
        assert_ne!(h1, "test");
    }

    #[test]
    fn truncate_limits_length() {
        let truncated = apply_mask("hello world", &MaskingRule::Truncate(5));
        assert_eq!(truncated, "hello");
    }

    #[test]
    fn hot_update_replaces_all_rules() {
        let config = DynamicMaskingConfig::new();
        config.set_rule("phone", MaskingRule::MaskAll);
        let mut new_rules = HashMap::new();
        new_rules.insert("email".to_string(), MaskingRule::Hash);
        config.hot_update(new_rules);
        assert!(config.get_rule("phone").is_none());
        assert!(config.get_rule("email").is_some());
    }

    #[test]
    fn snapshot_is_consistent() {
        let config = DynamicMaskingConfig::new();
        config.set_rule("a", MaskingRule::MaskAll);
        config.set_rule("b", MaskingRule::Hash);
        let snap = config.snapshot();
        assert_eq!(snap.len(), 2);
    }

    #[test]
    fn mask_is_irreversible() {
        let original = "13800001234";
        let masked = apply_mask(original, &MaskingRule::MaskMiddle);
        assert_ne!(masked, original);
        assert!(!masked.contains("0000"));
    }

    #[test]
    fn short_string_mask_middle() {
        assert_eq!(apply_mask("ab", &MaskingRule::MaskMiddle), "**");
    }
}
