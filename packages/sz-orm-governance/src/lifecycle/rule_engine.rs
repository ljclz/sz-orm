//! 生命周期规则引擎

use std::sync::Arc;

use parking_lot::RwLock;

use super::types::{LifecycleError, LifecycleRule};

/// 生命周期规则引擎
pub struct LifecycleRuleEngine {
    rules: Arc<RwLock<Vec<LifecycleRule>>>,
    reload_version: Arc<RwLock<u64>>,
}

impl LifecycleRuleEngine {
    pub fn new() -> Self {
        Self {
            rules: Arc::new(RwLock::new(Vec::new())),
            reload_version: Arc::new(RwLock::new(0)),
        }
    }

    pub fn load_rules(&self, rules: Vec<LifecycleRule>) -> Result<(), LifecycleError> {
        for rule in &rules {
            self.validate_rule(rule)?;
        }
        let mut guard = self.rules.write();
        *guard = rules;
        let mut version = self.reload_version.write();
        *version += 1;
        Ok(())
    }

    pub fn hot_reload(&self, rules: Vec<LifecycleRule>) -> Result<(), LifecycleError> {
        self.load_rules(rules)
    }

    pub fn rules(&self) -> Vec<LifecycleRule> {
        self.rules.read().clone()
    }

    pub fn reload_version(&self) -> u64 {
        *self.reload_version.read()
    }

    pub fn find_rule_for_table(&self, table: &str) -> Option<LifecycleRule> {
        self.rules
            .read()
            .iter()
            .find(|r| r.enabled && r.target_table == table)
            .cloned()
    }

    fn validate_rule(&self, rule: &LifecycleRule) -> Result<(), LifecycleError> {
        if rule.name.is_empty() {
            return Err(LifecycleError::RuleInvalid("规则名称为空".to_string()));
        }
        if rule.target_table.is_empty() {
            return Err(LifecycleError::RuleInvalid("目标表为空".to_string()));
        }
        if rule.retention_days == 0 {
            return Err(LifecycleError::RuleInvalid("保留天数为 0".to_string()));
        }
        if rule.destruction_days < rule.retention_days {
            return Err(LifecycleError::RuleInvalid(
                "销毁天数不能小于保留天数".to_string(),
            ));
        }
        if rule.migration_window.start_hour >= rule.migration_window.end_hour {
            return Err(LifecycleError::RuleInvalid(
                "迁移窗口开始时间必须小于结束时间".to_string(),
            ));
        }
        Ok(())
    }
}

impl Default for LifecycleRuleEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for LifecycleRuleEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LifecycleRuleEngine")
            .field("rules_count", &self.rules.read().len())
            .field("reload_version", &*self.reload_version.read())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lifecycle::types::{ColdHotThreshold, MigrationWindow};

    fn make_rule(name: &str, table: &str) -> LifecycleRule {
        LifecycleRule {
            name: name.to_string(),
            target_table: table.to_string(),
            cold_hot_threshold: ColdHotThreshold::default(),
            retention_days: 90,
            destruction_days: 365,
            migration_window: MigrationWindow::default(),
            archive_target: "s3://archive".to_string(),
            enabled: true,
        }
    }

    #[test]
    fn test_load_rules_success() {
        let engine = LifecycleRuleEngine::new();
        let rules = vec![make_rule("r1", "orders"), make_rule("r2", "users")];
        assert!(engine.load_rules(rules).is_ok());
        assert_eq!(engine.rules().len(), 2);
        assert_eq!(engine.reload_version(), 1);
    }

    #[test]
    fn test_load_rules_invalid_empty_name() {
        let engine = LifecycleRuleEngine::new();
        let rule = LifecycleRule {
            name: "".to_string(),
            ..make_rule("r1", "orders")
        };
        assert!(engine.load_rules(vec![rule]).is_err());
    }

    #[test]
    fn test_load_rules_invalid_destruction_less_than_retention() {
        let engine = LifecycleRuleEngine::new();
        let rule = LifecycleRule {
            retention_days: 365,
            destruction_days: 90,
            ..make_rule("r1", "orders")
        };
        assert!(engine.load_rules(vec![rule]).is_err());
    }

    #[test]
    fn test_load_rules_invalid_window() {
        let engine = LifecycleRuleEngine::new();
        let rule = LifecycleRule {
            migration_window: MigrationWindow {
                start_hour: 6,
                end_hour: 2,
            },
            ..make_rule("r1", "orders")
        };
        assert!(engine.load_rules(vec![rule]).is_err());
    }

    #[test]
    fn test_hot_reload() {
        let engine = LifecycleRuleEngine::new();
        engine.load_rules(vec![make_rule("r1", "orders")]).unwrap();
        assert_eq!(engine.reload_version(), 1);

        engine.hot_reload(vec![make_rule("r2", "users")]).unwrap();
        assert_eq!(engine.reload_version(), 2);
        assert_eq!(engine.rules()[0].target_table, "users");
    }

    #[test]
    fn test_find_rule_for_table() {
        let engine = LifecycleRuleEngine::new();
        engine
            .load_rules(vec![make_rule("r1", "orders"), make_rule("r2", "users")])
            .unwrap();

        let rule = engine.find_rule_for_table("orders").unwrap();
        assert_eq!(rule.name, "r1");
        assert!(engine.find_rule_for_table("nonexistent").is_none());
    }

    #[test]
    fn test_find_rule_disabled_not_matched() {
        let engine = LifecycleRuleEngine::new();
        let mut rule = make_rule("r1", "orders");
        rule.enabled = false;
        engine.load_rules(vec![rule]).unwrap();

        assert!(engine.find_rule_for_table("orders").is_none());
    }
}
