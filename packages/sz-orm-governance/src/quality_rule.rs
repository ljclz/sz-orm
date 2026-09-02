//! 数据质量规则自动生成（TASK-019 + TASK-038 冲突检测+回滚）

use crate::types::GovernanceError;
use serde::{Deserialize, Serialize};

/// 质量规则类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum QualityRuleType {
    NullCheck,
    Uniqueness,
    RangeCheck,
    FormatCheck,
}

/// 质量规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityRule {
    pub rule_id: String,
    pub table: String,
    pub column: String,
    pub rule_type: QualityRuleType,
    pub expression: String,
    pub confidence: f64,
    pub rationale: String,
}

/// 质量规则生成器
pub struct QualityRuleGenerator;

impl QualityRuleGenerator {
    pub fn new() -> Self {
        Self
    }

    /// 为表和字段生成质量规则
    pub fn generate(&self, table: &str, columns: &[(&str, &str)]) -> Vec<QualityRule> {
        let mut rules = Vec::new();
        for (col, col_type) in columns {
            rules.push(QualityRule {
                rule_id: format!("null_{table}_{col}"),
                table: table.to_string(),
                column: col.to_string(),
                rule_type: QualityRuleType::NullCheck,
                expression: format!("{col} IS NOT NULL"),
                confidence: 0.95,
                rationale: format!("{col_type} 列不应为空"),
            });

            rules.push(QualityRule {
                rule_id: format!("uniq_{table}_{col}"),
                table: table.to_string(),
                column: col.to_string(),
                rule_type: QualityRuleType::Uniqueness,
                expression: format!("COUNT(DISTINCT {col}) = COUNT({col})"),
                confidence: 0.80,
                rationale: "ID 列应唯一".to_string(),
            });

            if *col_type == "INTEGER" || *col_type == "BIGINT" {
                rules.push(QualityRule {
                    rule_id: format!("range_{table}_{col}"),
                    table: table.to_string(),
                    column: col.to_string(),
                    rule_type: QualityRuleType::RangeCheck,
                    expression: format!("{col} >= 0 AND {col} <= 999999999"),
                    confidence: 0.70,
                    rationale: "数值列应在合理范围".to_string(),
                });
            }

            if *col_type == "VARCHAR" || *col_type == "TEXT" {
                rules.push(QualityRule {
                    rule_id: format!("format_{table}_{col}"),
                    table: table.to_string(),
                    column: col.to_string(),
                    rule_type: QualityRuleType::FormatCheck,
                    expression: format!("LENGTH({col}) > 0 AND LENGTH({col}) <= 255"),
                    confidence: 0.75,
                    rationale: "字符串长度应在合理范围".to_string(),
                });
            }
        }
        rules
    }
}

impl Default for QualityRuleGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// 质量规则冲突
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuleConflict {
    pub rule_a_id: String,
    pub rule_b_id: String,
    pub conflict_type: ConflictType,
    pub description: String,
}

/// 冲突类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConflictType {
    Contradictory,
    Overlapping,
    Redundant,
}

/// 质量规则集（支持冲突检测+回滚 TASK-038）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityRuleSet {
    pub rules: Vec<QualityRule>,
    pub version: u64,
    pub history: Vec<Vec<QualityRule>>,
}

impl QualityRuleSet {
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            version: 1,
            history: Vec::new(),
        }
    }

    /// 添加规则（保存历史用于回滚）
    pub fn add_rules(&mut self, new_rules: Vec<QualityRule>) -> Result<(), GovernanceError> {
        let conflicts = self.detect_conflicts(&new_rules);
        if !conflicts.is_empty() {
            return Err(GovernanceError::ComplianceAuditFailed(format!(
                "检测到 {} 个冲突: {}",
                conflicts.len(),
                conflicts
                    .iter()
                    .map(|c| c.description.clone())
                    .collect::<Vec<_>>()
                    .join("; ")
            )));
        }

        self.history.push(self.rules.clone());
        self.rules.extend(new_rules);
        self.version += 1;
        Ok(())
    }

    /// 强制添加规则（不检测冲突）
    pub fn add_rules_force(&mut self, new_rules: Vec<QualityRule>) {
        self.history.push(self.rules.clone());
        self.rules.extend(new_rules);
        self.version += 1;
    }

    /// 检测新规则与现有规则的冲突
    pub fn detect_conflicts(&self, new_rules: &[QualityRule]) -> Vec<RuleConflict> {
        let mut conflicts = Vec::new();

        for new_rule in new_rules {
            for existing in &self.rules {
                if existing.table != new_rule.table || existing.column != new_rule.column {
                    continue;
                }

                if existing.rule_type == QualityRuleType::NullCheck
                    && new_rule.rule_type == QualityRuleType::NullCheck
                {
                    conflicts.push(RuleConflict {
                        rule_a_id: existing.rule_id.clone(),
                        rule_b_id: new_rule.rule_id.clone(),
                        conflict_type: ConflictType::Redundant,
                        description: format!(
                            "表 {}.列 {} 的空值检查规则重复",
                            existing.table, existing.column
                        ),
                    });
                }

                if existing.rule_type == QualityRuleType::RangeCheck
                    && new_rule.rule_type == QualityRuleType::RangeCheck
                    && existing.expression != new_rule.expression
                {
                    conflicts.push(RuleConflict {
                        rule_a_id: existing.rule_id.clone(),
                        rule_b_id: new_rule.rule_id.clone(),
                        conflict_type: ConflictType::Contradictory,
                        description: format!(
                            "表 {}.列 {} 的范围检查规则矛盾",
                            existing.table, existing.column
                        ),
                    });
                }
            }
        }

        conflicts
    }

    /// 回滚到上一个版本
    pub fn rollback(&mut self) -> Result<(), GovernanceError> {
        if self.history.is_empty() {
            return Err(GovernanceError::ComplianceAuditFailed(
                "无历史版本可回滚".to_string(),
            ));
        }
        self.rules = self.history.pop().unwrap();
        self.version += 1;
        Ok(())
    }

    /// 回滚到指定版本
    pub fn rollback_to(&mut self, target_version: u64) -> Result<(), GovernanceError> {
        if target_version >= self.version {
            return Err(GovernanceError::ComplianceAuditFailed(format!(
                "无法回滚到更高或相同版本 {}（当前 {}）",
                target_version, self.version
            )));
        }
        let rollback_count = self.version - target_version;
        for _ in 0..rollback_count {
            if self.history.is_empty() {
                return Err(GovernanceError::ComplianceAuditFailed(
                    "无历史版本可回滚".to_string(),
                ));
            }
            self.rules = self.history.pop().unwrap();
        }
        self.version = target_version;
        Ok(())
    }

    /// 获取当前规则数
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }
}

impl Default for QualityRuleSet {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_all_rule_types() {
        let generator = QualityRuleGenerator::new();
        let rules = generator.generate("users", &[("id", "BIGINT"), ("name", "VARCHAR")]);

        let types: Vec<_> = rules.iter().map(|r| &r.rule_type).collect();
        assert!(types.contains(&&QualityRuleType::NullCheck));
        assert!(types.contains(&&QualityRuleType::Uniqueness));
        assert!(types.contains(&&QualityRuleType::RangeCheck));
        assert!(types.contains(&&QualityRuleType::FormatCheck));
    }

    #[test]
    fn test_rule_confidence_range() {
        let generator = QualityRuleGenerator::new();
        let rules = generator.generate("orders", &[("amount", "INTEGER")]);
        for rule in &rules {
            assert!(rule.confidence > 0.0 && rule.confidence <= 1.0);
        }
    }

    #[test]
    fn test_detect_redundant_conflict() {
        let mut rule_set = QualityRuleSet::new();
        let generator = QualityRuleGenerator::new();
        let rules = generator.generate("users", &[("id", "BIGINT")]);
        rule_set.add_rules_force(rules.clone());

        let conflicts = rule_set.detect_conflicts(&rules);
        assert!(
            conflicts
                .iter()
                .any(|c| c.conflict_type == ConflictType::Redundant),
            "应检测到冗余冲突"
        );
    }

    #[test]
    fn test_add_rules_with_conflict_fails() {
        let mut rule_set = QualityRuleSet::new();
        let generator = QualityRuleGenerator::new();
        let rules = generator.generate("users", &[("id", "BIGINT")]);
        rule_set.add_rules_force(rules.clone());

        let result = rule_set.add_rules(rules);
        assert!(result.is_err(), "冲突规则应添加失败");
    }

    #[test]
    fn test_rollback() {
        let mut rule_set = QualityRuleSet::new();
        let generator = QualityRuleGenerator::new();
        let rules = generator.generate("users", &[("id", "BIGINT")]);
        rule_set.add_rules_force(rules);

        assert!(rule_set.rule_count() > 0);
        rule_set.rollback().unwrap();
        assert_eq!(rule_set.rule_count(), 0, "回滚后应无规则");
    }

    #[test]
    fn test_rollback_empty_fails() {
        let mut rule_set = QualityRuleSet::new();
        assert!(rule_set.rollback().is_err());
    }

    #[test]
    fn test_rollback_to_version() {
        let mut rule_set = QualityRuleSet::new();
        let generator = QualityRuleGenerator::new();

        let rules1 = generator.generate("users", &[("id", "BIGINT")]);
        rule_set.add_rules_force(rules1);
        let v1 = rule_set.version;

        let rules2 = generator.generate("orders", &[("id", "BIGINT")]);
        rule_set.add_rules_force(rules2);

        rule_set.rollback_to(v1).unwrap();
        assert!(rule_set.rule_count() > 0);
    }

    #[test]
    fn test_no_conflict_different_tables() {
        let mut rule_set = QualityRuleSet::new();
        let generator = QualityRuleGenerator::new();
        let rules1 = generator.generate("users", &[("id", "BIGINT")]);
        let rules2 = generator.generate("orders", &[("id", "BIGINT")]);

        rule_set.add_rules_force(rules1);
        let conflicts = rule_set.detect_conflicts(&rules2);
        assert!(conflicts.is_empty(), "不同表不应有冲突");
    }
}
