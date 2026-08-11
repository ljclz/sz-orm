//! 数据质量自动检测模块（v4.1.0，`data-quality` feature gate）
//!
//! 提供六类统计学规则检测数据质量，生成质量报告。

use std::collections::HashMap;

/// 质量规则类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QualityRuleType {
    /// 完整性（非空检查）
    Completeness,
    /// 唯一性（重复值检查）
    Uniqueness,
    /// 有效性（值域检查）
    Validity,
    /// 一致性（跨字段/表一致性）
    Consistency,
    /// 及时性（数据新鲜度）
    Timeliness,
    /// 准确性（与参考值比对）
    Accuracy,
}

/// 质量规则
#[derive(Debug, Clone)]
pub struct QualityRule {
    /// 规则名
    pub name: String,
    /// 规则类型
    pub rule_type: QualityRuleType,
    /// 字段名
    pub field: String,
    /// 规则参数
    pub params: HashMap<String, f64>,
}

impl QualityRule {
    /// 创建规则
    pub fn new(name: String, rule_type: QualityRuleType, field: String) -> Self {
        Self {
            name,
            rule_type,
            field,
            params: HashMap::new(),
        }
    }

    /// 添加参数
    pub fn with_param(mut self, key: &str, value: f64) -> Self {
        self.params.insert(key.to_string(), value);
        self
    }
}

/// 质量检测结果
#[derive(Debug, Clone)]
pub struct QualityResult {
    /// 规则名
    pub rule_name: String,
    /// 规则类型
    pub rule_type: QualityRuleType,
    /// 通过数
    pub passed: usize,
    /// 失败数
    pub failed: usize,
    /// 总数
    pub total: usize,
    /// 失败样本（前 10 个）
    pub failure_samples: Vec<String>,
}

impl QualityResult {
    /// 通过率
    pub fn pass_rate(&self) -> f64 {
        if self.total == 0 {
            1.0
        } else {
            self.passed as f64 / self.total as f64
        }
    }

    /// 是否通过
    pub fn is_passed(&self) -> bool {
        self.failed == 0
    }
}

/// 质量报告
#[derive(Debug, Clone)]
pub struct QualityReport {
    /// 各规则检测结果
    pub results: Vec<QualityResult>,
    /// 总规则数
    pub total_rules: usize,
    /// 通过规则数
    pub passed_rules: usize,
    /// 总检测记录数
    pub total_records: usize,
}

impl QualityReport {
    /// 整体通过率
    pub fn overall_pass_rate(&self) -> f64 {
        if self.total_rules == 0 {
            1.0
        } else {
            self.passed_rules as f64 / self.total_rules as f64
        }
    }
}

/// 数据质量引擎
pub struct DataQualityEngine {
    /// 规则列表
    rules: Vec<QualityRule>,
}

impl DataQualityEngine {
    /// 创建引擎
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// 添加规则
    pub fn add_rule(&mut self, rule: QualityRule) {
        self.rules.push(rule);
    }

    /// 执行质量检测
    pub fn check(&self, data: &[HashMap<String, String>]) -> QualityReport {
        let mut results = Vec::with_capacity(self.rules.len());
        let total_records = data.len();

        for rule in &self.rules {
            let result = self.check_rule(rule, data);
            results.push(result);
        }

        let total_rules = results.len();
        let passed_rules = results.iter().filter(|r| r.is_passed()).count();

        QualityReport {
            results,
            total_rules,
            passed_rules,
            total_records,
        }
    }

    fn check_rule(&self, rule: &QualityRule, data: &[HashMap<String, String>]) -> QualityResult {
        match rule.rule_type {
            QualityRuleType::Completeness => self.check_completeness(rule, data),
            QualityRuleType::Uniqueness => self.check_uniqueness(rule, data),
            QualityRuleType::Validity => self.check_validity(rule, data),
            QualityRuleType::Consistency => self.check_consistency(rule, data),
            QualityRuleType::Timeliness => self.check_timeliness(rule, data),
            QualityRuleType::Accuracy => self.check_accuracy(rule, data),
        }
    }

    fn check_completeness(
        &self,
        rule: &QualityRule,
        data: &[HashMap<String, String>],
    ) -> QualityResult {
        let mut passed = 0;
        let mut failed = 0;
        let mut samples = Vec::new();

        for (i, record) in data.iter().enumerate() {
            match record.get(&rule.field) {
                Some(v) if !v.is_empty() => passed += 1,
                _ => {
                    failed += 1;
                    if samples.len() < 10 {
                        samples.push(format!("row {} field '{}' is null/empty", i, rule.field));
                    }
                }
            }
        }

        QualityResult {
            rule_name: rule.name.clone(),
            rule_type: rule.rule_type.clone(),
            passed,
            failed,
            total: data.len(),
            failure_samples: samples,
        }
    }

    fn check_uniqueness(
        &self,
        rule: &QualityRule,
        data: &[HashMap<String, String>],
    ) -> QualityResult {
        let mut seen: HashMap<String, usize> = HashMap::new();
        for record in data {
            if let Some(v) = record.get(&rule.field) {
                *seen.entry(v.clone()).or_insert(0) += 1;
            }
        }
        let duplicates: Vec<(String, usize)> = seen.into_iter().filter(|(_, c)| *c > 1).collect();
        let failed: usize = duplicates.iter().map(|(_, c)| c - 1).sum();
        let passed = data.len().saturating_sub(failed);
        let samples: Vec<String> = duplicates
            .into_iter()
            .take(10)
            .map(|(v, c)| format!("value '{}' appears {} times", v, c))
            .collect();

        QualityResult {
            rule_name: rule.name.clone(),
            rule_type: rule.rule_type.clone(),
            passed,
            failed,
            total: data.len(),
            failure_samples: samples,
        }
    }

    fn check_validity(
        &self,
        rule: &QualityRule,
        data: &[HashMap<String, String>],
    ) -> QualityResult {
        let min = rule.params.get("min").copied().unwrap_or(f64::NEG_INFINITY);
        let max = rule.params.get("max").copied().unwrap_or(f64::INFINITY);
        let mut passed = 0;
        let mut failed = 0;
        let mut samples = Vec::new();

        for (i, record) in data.iter().enumerate() {
            if let Some(v) = record.get(&rule.field) {
                if let Ok(n) = v.parse::<f64>() {
                    if n >= min && n <= max {
                        passed += 1;
                    } else {
                        failed += 1;
                        if samples.len() < 10 {
                            samples
                                .push(format!("row {} value {} out of [{}, {}]", i, n, min, max));
                        }
                    }
                } else {
                    failed += 1;
                    if samples.len() < 10 {
                        samples.push(format!("row {} value '{}' not numeric", i, v));
                    }
                }
            } else {
                failed += 1;
            }
        }

        QualityResult {
            rule_name: rule.name.clone(),
            rule_type: rule.rule_type.clone(),
            passed,
            failed,
            total: data.len(),
            failure_samples: samples,
        }
    }

    fn check_consistency(
        &self,
        rule: &QualityRule,
        data: &[HashMap<String, String>],
    ) -> QualityResult {
        let mut passed = 0;
        let mut failed = 0;
        for record in data {
            if record.contains_key(&rule.field) {
                passed += 1;
            } else {
                failed += 1;
            }
        }
        QualityResult {
            rule_name: rule.name.clone(),
            rule_type: rule.rule_type.clone(),
            passed,
            failed,
            total: data.len(),
            failure_samples: Vec::new(),
        }
    }

    fn check_timeliness(
        &self,
        rule: &QualityRule,
        data: &[HashMap<String, String>],
    ) -> QualityResult {
        let max_age = rule
            .params
            .get("max_age_seconds")
            .copied()
            .unwrap_or(f64::INFINITY);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs_f64();
        let mut passed = 0;
        let mut failed = 0;

        for record in data {
            if let Some(v) = record.get(&rule.field) {
                if let Ok(ts) = v.parse::<f64>() {
                    if now - ts <= max_age {
                        passed += 1;
                    } else {
                        failed += 1;
                    }
                } else {
                    failed += 1;
                }
            } else {
                failed += 1;
            }
        }

        QualityResult {
            rule_name: rule.name.clone(),
            rule_type: rule.rule_type.clone(),
            passed,
            failed,
            total: data.len(),
            failure_samples: Vec::new(),
        }
    }

    fn check_accuracy(
        &self,
        rule: &QualityRule,
        data: &[HashMap<String, String>],
    ) -> QualityResult {
        let expected = rule.params.get("expected").copied();
        let tolerance = rule.params.get("tolerance").copied().unwrap_or(0.0);
        let mut passed = 0;
        let mut failed = 0;

        for record in data {
            if let (Some(exp), Some(v)) = (expected, record.get(&rule.field)) {
                if let Ok(n) = v.parse::<f64>() {
                    if (n - exp).abs() <= tolerance {
                        passed += 1;
                    } else {
                        failed += 1;
                    }
                } else {
                    failed += 1;
                }
            } else {
                passed += 1;
            }
        }

        QualityResult {
            rule_name: rule.name.clone(),
            rule_type: rule.rule_type.clone(),
            passed,
            failed,
            total: data.len(),
            failure_samples: Vec::new(),
        }
    }
}

impl Default for DataQualityEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_record(fields: &[(&str, &str)]) -> HashMap<String, String> {
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn test_completeness_check() {
        let mut engine = DataQualityEngine::new();
        engine.add_rule(QualityRule::new(
            "non_null_name".to_string(),
            QualityRuleType::Completeness,
            "name".to_string(),
        ));
        let data = vec![
            make_record(&[("name", "Alice"), ("age", "30")]),
            make_record(&[("age", "25")]),
            make_record(&[("name", "Bob"), ("age", "40")]),
        ];
        let report = engine.check(&data);
        assert_eq!(report.total_rules, 1);
        assert_eq!(report.results[0].passed, 2);
        assert_eq!(report.results[0].failed, 1);
        assert!(!report.results[0].is_passed());
    }

    #[test]
    fn test_uniqueness_check() {
        let mut engine = DataQualityEngine::new();
        engine.add_rule(QualityRule::new(
            "unique_id".to_string(),
            QualityRuleType::Uniqueness,
            "id".to_string(),
        ));
        let data = vec![
            make_record(&[("id", "1")]),
            make_record(&[("id", "2")]),
            make_record(&[("id", "1")]),
        ];
        let report = engine.check(&data);
        assert_eq!(report.results[0].failed, 1);
    }

    #[test]
    fn test_validity_check() {
        let mut engine = DataQualityEngine::new();
        engine.add_rule(
            QualityRule::new(
                "age_range".to_string(),
                QualityRuleType::Validity,
                "age".to_string(),
            )
            .with_param("min", 0.0)
            .with_param("max", 150.0),
        );
        let data = vec![
            make_record(&[("age", "30")]),
            make_record(&[("age", "200")]),
            make_record(&[("age", "-5")]),
        ];
        let report = engine.check(&data);
        assert_eq!(report.results[0].passed, 1);
        assert_eq!(report.results[0].failed, 2);
    }

    #[test]
    fn test_accuracy_check() {
        let mut engine = DataQualityEngine::new();
        engine.add_rule(
            QualityRule::new(
                "score_accuracy".to_string(),
                QualityRuleType::Accuracy,
                "score".to_string(),
            )
            .with_param("expected", 100.0)
            .with_param("tolerance", 5.0),
        );
        let data = vec![
            make_record(&[("score", "98")]),
            make_record(&[("score", "103")]),
            make_record(&[("score", "50")]),
        ];
        let report = engine.check(&data);
        assert_eq!(report.results[0].passed, 2);
        assert_eq!(report.results[0].failed, 1);
    }

    #[test]
    fn test_empty_data() {
        let mut engine = DataQualityEngine::new();
        engine.add_rule(QualityRule::new(
            "rule1".to_string(),
            QualityRuleType::Completeness,
            "name".to_string(),
        ));
        let report = engine.check(&[]);
        assert_eq!(report.total_records, 0);
        assert!(report.results[0].is_passed());
    }

    #[test]
    fn test_multiple_rules() {
        let mut engine = DataQualityEngine::new();
        engine.add_rule(QualityRule::new(
            "non_null".to_string(),
            QualityRuleType::Completeness,
            "name".to_string(),
        ));
        engine.add_rule(
            QualityRule::new(
                "age_valid".to_string(),
                QualityRuleType::Validity,
                "age".to_string(),
            )
            .with_param("min", 0.0)
            .with_param("max", 150.0),
        );
        let data = vec![
            make_record(&[("name", "Alice"), ("age", "30")]),
            make_record(&[("name", "Bob"), ("age", "200")]),
        ];
        let report = engine.check(&data);
        assert_eq!(report.total_rules, 2);
        assert_eq!(report.passed_rules, 1);
        assert_eq!(report.overall_pass_rate(), 0.5);
    }

    #[test]
    fn test_pass_rate() {
        let result = QualityResult {
            rule_name: "test".to_string(),
            rule_type: QualityRuleType::Completeness,
            passed: 8,
            failed: 2,
            total: 10,
            failure_samples: Vec::new(),
        };
        assert_eq!(result.pass_rate(), 0.8);
    }

    #[test]
    fn test_consistency_check() {
        let mut engine = DataQualityEngine::new();
        engine.add_rule(QualityRule::new(
            "has_email".to_string(),
            QualityRuleType::Consistency,
            "email".to_string(),
        ));
        let data = vec![
            make_record(&[("name", "Alice"), ("email", "a@b.com")]),
            make_record(&[("name", "Bob")]),
        ];
        let report = engine.check(&data);
        assert_eq!(report.results[0].passed, 1);
        assert_eq!(report.results[0].failed, 1);
    }
}
