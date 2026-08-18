//! 脱敏策略引擎：条件脱敏、优先级、多步骤管道。
//!
//! - [`MaskingStrategyEngine`] — 组合字段规则与条件，按优先级应用脱敏
//! - [`MaskingCondition`] — 基于其他字段值决定是否对当前字段脱敏
//! - [`MaskingPipeline`] — 多步骤脱敏管道（先脱敏、再哈希、再审计）

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{DataMasker, MaskingRule};

// ============================================================================
// 条件脱敏
// ============================================================================

/// 脱敏条件：基于其他字段的值决定是否对当前字段脱敏。
///
/// 例如：仅当 `is_vip` 字段为 `"false"` 时才脱敏 `phone` 字段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaskingCondition {
    /// 依赖的字段名
    field: String,
    /// 期望的字段值（相等时触发脱敏）
    expected_value: String,
}

impl MaskingCondition {
    /// 创建条件：当 `field` 的值等于 `expected_value` 时触发
    pub fn new(field: &str, expected_value: &str) -> Self {
        Self {
            field: field.to_string(),
            expected_value: expected_value.to_string(),
        }
    }

    /// 依赖字段名
    pub fn field(&self) -> &str {
        &self.field
    }

    /// 期望值
    pub fn expected_value(&self) -> &str {
        &self.expected_value
    }

    /// 检查条件是否满足
    pub fn is_satisfied(&self, data: &HashMap<String, String>) -> bool {
        data.get(&self.field)
            .map(|v| v == &self.expected_value)
            .unwrap_or(false)
    }
}

// ============================================================================
// 字段级脱敏规则
// ============================================================================

/// 字段级脱敏规则：字段名 + 脱敏规则 + 可选条件 + 优先级。
///
/// 优先级数值越小越先应用（默认 100）。当多个规则匹配同一字段时，
/// 仅应用优先级最高（数值最小）的规则。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldMaskingRule {
    field: String,
    rule: MaskingRule,
    condition: Option<MaskingCondition>,
    priority: u32,
}

impl FieldMaskingRule {
    /// 创建无条件字段规则（默认优先级 100）
    pub fn new(field: &str, rule: MaskingRule) -> Self {
        Self {
            field: field.to_string(),
            rule,
            condition: None,
            priority: 100,
        }
    }

    /// 设置脱敏条件（链式）
    pub fn with_condition(mut self, cond: MaskingCondition) -> Self {
        self.condition = Some(cond);
        self
    }

    /// 设置优先级（链式）
    pub fn with_priority(mut self, priority: u32) -> Self {
        self.priority = priority;
        self
    }

    /// 字段名
    pub fn field(&self) -> &str {
        &self.field
    }

    /// 脱敏规则
    pub fn rule(&self) -> &MaskingRule {
        &self.rule
    }

    /// 脱敏条件
    pub fn condition(&self) -> Option<&MaskingCondition> {
        self.condition.as_ref()
    }

    /// 优先级
    pub fn priority(&self) -> u32 {
        self.priority
    }

    /// 检查规则是否应该应用（条件满足或无条件）
    pub fn should_apply(&self, data: &HashMap<String, String>) -> bool {
        match &self.condition {
            Some(cond) => cond.is_satisfied(data),
            None => true,
        }
    }
}

// ============================================================================
// 脱敏策略引擎
// ============================================================================

/// 脱敏策略引擎：管理多条字段规则，按优先级应用脱敏。
///
/// 当多个规则匹配同一字段时，仅应用优先级最高（数值最小）的规则。
/// 支持条件脱敏：仅在条件满足时才对字段应用脱敏。
#[derive(Debug, Clone, Default)]
pub struct MaskingStrategyEngine {
    rules: Vec<FieldMaskingRule>,
}

impl MaskingStrategyEngine {
    /// 创建空策略引擎
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加字段规则（链式）
    pub fn add_rule(mut self, rule: FieldMaskingRule) -> Self {
        self.rules.push(rule);
        self
    }

    /// 添加多条规则（链式）
    pub fn add_rules(mut self, rules: Vec<FieldMaskingRule>) -> Self {
        self.rules.extend(rules);
        self
    }

    /// 规则数量
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// 清空规则
    pub fn clear(&mut self) {
        self.rules.clear();
    }

    /// 获取某字段应应用的规则（优先级最高且条件满足的）
    pub fn effective_rule(
        &self,
        field: &str,
        data: &HashMap<String, String>,
    ) -> Option<&MaskingRule> {
        let mut candidates: Vec<&FieldMaskingRule> = self
            .rules
            .iter()
            .filter(|r| r.field() == field && r.should_apply(data))
            .collect();
        candidates.sort_by_key(|r| r.priority());
        candidates.first().map(|r| r.rule())
    }

    /// 对 HashMap 应用策略脱敏
    pub fn apply_to_map(&self, data: &HashMap<String, String>) -> HashMap<String, String> {
        data.iter()
            .map(|(k, v)| match self.effective_rule(k, data) {
                Some(rule) => (k.clone(), DataMasker::apply(rule, v)),
                None => (k.clone(), v.clone()),
            })
            .collect()
    }

    /// 对 JSON 字符串应用策略脱敏
    pub fn apply_to_json(&self, json: &str) -> String {
        let Ok(mut value) = serde_json::from_str::<serde_json::Value>(json) else {
            return json.to_string();
        };
        let Some(obj) = value.as_object_mut() else {
            return json.to_string();
        };
        // 构建临时 HashMap 用于条件求值
        let snapshot: HashMap<String, String> = obj
            .iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
            .collect();
        let keys: Vec<String> = obj.keys().cloned().collect();
        for key in keys {
            if let Some(rule) = self.effective_rule(&key, &snapshot) {
                if let Some(serde_json::Value::String(s)) = obj.get_mut(&key) {
                    *s = DataMasker::apply(rule, s);
                }
            }
        }
        serde_json::to_string(&value).unwrap_or_else(|_| json.to_string())
    }

    /// 按字段名移除所有规则
    pub fn remove_rules_for_field(&mut self, field: &str) -> usize {
        let before = self.rules.len();
        self.rules.retain(|r| r.field() != field);
        before - self.rules.len()
    }
}

// ============================================================================
// 脱敏管道
// ============================================================================

/// 管道阶段：一个命名的脱敏步骤
#[derive(Debug, Clone)]
pub struct PipelineStage {
    name: String,
    rules: HashMap<String, MaskingRule>,
}

impl PipelineStage {
    /// 创建空阶段
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            rules: HashMap::new(),
        }
    }

    /// 阶段名
    pub fn name(&self) -> &str {
        &self.name
    }

    /// 添加字段规则（链式）
    pub fn with_rule(mut self, field: &str, rule: MaskingRule) -> Self {
        self.rules.insert(field.to_string(), rule);
        self
    }

    /// 规则数
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// 对数据应用本阶段脱敏
    pub fn apply(&self, data: &HashMap<String, String>) -> HashMap<String, String> {
        DataMasker::mask_map(&self.rules, data)
    }

    /// 对 JSON 应用本阶段脱敏
    pub fn apply_to_json(&self, json: &str) -> String {
        DataMasker::mask_json(&self.rules, json)
    }
}

/// 脱敏管道：按顺序执行多个脱敏阶段。
///
/// 每个阶段的输出是下一阶段的输入，允许组合不同粒度的脱敏操作。
/// 例如：阶段 1 按字段类型脱敏，阶段 2 对残留的敏感字段哈希脱敏。
#[derive(Debug, Clone, Default)]
pub struct MaskingPipeline {
    stages: Vec<PipelineStage>,
}

impl MaskingPipeline {
    /// 创建空管道
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加阶段（链式）
    pub fn add_stage(mut self, stage: PipelineStage) -> Self {
        self.stages.push(stage);
        self
    }

    /// 阶段数
    pub fn stage_count(&self) -> usize {
        self.stages.len()
    }

    /// 阶段名称列表
    pub fn stage_names(&self) -> Vec<&str> {
        self.stages.iter().map(|s| s.name()).collect()
    }

    /// 对 HashMap 按顺序执行所有阶段
    pub fn apply_to_map(&self, data: &HashMap<String, String>) -> HashMap<String, String> {
        let mut current = data.clone();
        for stage in &self.stages {
            current = stage.apply(&current);
        }
        current
    }

    /// 对 JSON 按顺序执行所有阶段
    pub fn apply_to_json(&self, json: &str) -> String {
        let mut current = json.to_string();
        for stage in &self.stages {
            current = stage.apply_to_json(&current);
        }
        current
    }

    /// 清空所有阶段
    pub fn clear(&mut self) {
        self.stages.clear();
    }
}

// ============================================================================
// 脱敏结果验证
// ============================================================================

/// 脱敏验证器：检查脱敏结果是否泄露了原始信息。
#[derive(Debug, Clone, Default)]
pub struct MaskingValidator;

impl MaskingValidator {
    /// 创建验证器
    pub fn new() -> Self {
        Self
    }

    /// 验证脱敏后的值不等于原始值（除非原始值本身就是兜底值）
    pub fn is_masked(original: &str, masked: &str) -> bool {
        original != masked || original.is_empty()
    }

    /// 验证脱敏后的值包含掩码字符 `*`
    pub fn contains_mask_char(masked: &str) -> bool {
        masked.contains('*')
    }

    /// 验证脱敏后不再包含原始敏感子串
    pub fn no_sensitive_substring(masked: &str, sensitive: &str) -> bool {
        if sensitive.len() <= 2 {
            return true;
        }
        !masked.contains(sensitive)
    }

    /// 批量验证字段是否已脱敏
    pub fn validate_map(
        data: &HashMap<String, String>,
        masked: &HashMap<String, String>,
    ) -> Vec<String> {
        data.iter()
            .filter(|(k, v)| masked.get(*k).map(|m| m == *v).unwrap_or(true) && !v.is_empty())
            .map(|(k, _)| k.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ----- MaskingCondition -----

    #[test]
    fn condition_new() {
        let c = MaskingCondition::new("is_vip", "false");
        assert_eq!(c.field(), "is_vip");
        assert_eq!(c.expected_value(), "false");
    }

    #[test]
    fn condition_satisfied() {
        let c = MaskingCondition::new("is_vip", "false");
        let mut data = HashMap::new();
        data.insert("is_vip".to_string(), "false".to_string());
        assert!(c.is_satisfied(&data));
    }

    #[test]
    fn condition_not_satisfied() {
        let c = MaskingCondition::new("is_vip", "false");
        let mut data = HashMap::new();
        data.insert("is_vip".to_string(), "true".to_string());
        assert!(!c.is_satisfied(&data));
    }

    #[test]
    fn condition_field_missing() {
        let c = MaskingCondition::new("is_vip", "false");
        let data = HashMap::new();
        assert!(!c.is_satisfied(&data));
    }

    // ----- FieldMaskingRule -----

    #[test]
    fn field_rule_new() {
        let r = FieldMaskingRule::new("phone", MaskingRule::Phone);
        assert_eq!(r.field(), "phone");
        assert_eq!(r.rule(), &MaskingRule::Phone);
        assert!(r.condition().is_none());
        assert_eq!(r.priority(), 100);
    }

    #[test]
    fn field_rule_with_condition() {
        let r = FieldMaskingRule::new("phone", MaskingRule::Phone)
            .with_condition(MaskingCondition::new("is_vip", "false"));
        assert!(r.condition().is_some());
    }

    #[test]
    fn field_rule_with_priority() {
        let r = FieldMaskingRule::new("phone", MaskingRule::Phone).with_priority(10);
        assert_eq!(r.priority(), 10);
    }

    #[test]
    fn field_rule_should_apply_no_condition() {
        let r = FieldMaskingRule::new("phone", MaskingRule::Phone);
        let data = HashMap::new();
        assert!(r.should_apply(&data));
    }

    #[test]
    fn field_rule_should_apply_condition_met() {
        let r = FieldMaskingRule::new("phone", MaskingRule::Phone)
            .with_condition(MaskingCondition::new("is_vip", "false"));
        let mut data = HashMap::new();
        data.insert("is_vip".to_string(), "false".to_string());
        assert!(r.should_apply(&data));
    }

    #[test]
    fn field_rule_should_apply_condition_not_met() {
        let r = FieldMaskingRule::new("phone", MaskingRule::Phone)
            .with_condition(MaskingCondition::new("is_vip", "false"));
        let mut data = HashMap::new();
        data.insert("is_vip".to_string(), "true".to_string());
        assert!(!r.should_apply(&data));
    }

    // ----- MaskingStrategyEngine -----

    #[test]
    fn strategy_engine_default_empty() {
        let e = MaskingStrategyEngine::new();
        assert_eq!(e.rule_count(), 0);
    }

    #[test]
    fn strategy_engine_add_rule() {
        let e = MaskingStrategyEngine::new()
            .add_rule(FieldMaskingRule::new("phone", MaskingRule::Phone));
        assert_eq!(e.rule_count(), 1);
    }

    #[test]
    fn strategy_engine_add_rules_batch() {
        let rules = vec![
            FieldMaskingRule::new("phone", MaskingRule::Phone),
            FieldMaskingRule::new("email", MaskingRule::Email),
        ];
        let e = MaskingStrategyEngine::new().add_rules(rules);
        assert_eq!(e.rule_count(), 2);
    }

    #[test]
    fn strategy_engine_apply_to_map() {
        let e = MaskingStrategyEngine::new()
            .add_rule(FieldMaskingRule::new("phone", MaskingRule::Phone));
        let mut data = HashMap::new();
        data.insert("phone".to_string(), "13812345678".to_string());
        data.insert("name".to_string(), "Alice".to_string());
        let result = e.apply_to_map(&data);
        assert_eq!(result["phone"], "138****5678");
        assert_eq!(result["name"], "Alice");
    }

    #[test]
    fn strategy_engine_apply_to_json() {
        let e = MaskingStrategyEngine::new()
            .add_rule(FieldMaskingRule::new("phone", MaskingRule::Phone));
        let json = r#"{"phone":"13812345678","name":"Alice"}"#;
        let result = e.apply_to_json(json);
        assert!(result.contains("138****5678"));
        assert!(result.contains("Alice"));
    }

    #[test]
    fn strategy_engine_apply_to_json_invalid() {
        let e = MaskingStrategyEngine::new()
            .add_rule(FieldMaskingRule::new("phone", MaskingRule::Phone));
        assert_eq!(e.apply_to_json("not json"), "not json");
    }

    #[test]
    fn strategy_engine_conditional_masking() {
        let e = MaskingStrategyEngine::new().add_rule(
            FieldMaskingRule::new("phone", MaskingRule::Phone)
                .with_condition(MaskingCondition::new("is_vip", "false")),
        );
        let mut data = HashMap::new();
        data.insert("phone".to_string(), "13812345678".to_string());
        data.insert("is_vip".to_string(), "true".to_string());
        let result = e.apply_to_map(&data);
        // is_vip=true → 不脱敏
        assert_eq!(result["phone"], "13812345678");
    }

    #[test]
    fn strategy_engine_conditional_masking_applied() {
        let e = MaskingStrategyEngine::new().add_rule(
            FieldMaskingRule::new("phone", MaskingRule::Phone)
                .with_condition(MaskingCondition::new("is_vip", "false")),
        );
        let mut data = HashMap::new();
        data.insert("phone".to_string(), "13812345678".to_string());
        data.insert("is_vip".to_string(), "false".to_string());
        let result = e.apply_to_map(&data);
        assert_eq!(result["phone"], "138****5678");
    }

    #[test]
    fn strategy_engine_priority_resolution() {
        let e = MaskingStrategyEngine::new()
            .add_rule(FieldMaskingRule::new("phone", MaskingRule::Password).with_priority(50))
            .add_rule(FieldMaskingRule::new("phone", MaskingRule::Phone).with_priority(10));
        let mut data = HashMap::new();
        data.insert("phone".to_string(), "13812345678".to_string());
        let result = e.apply_to_map(&data);
        // 优先级 10 的 Phone 规则胜出
        assert_eq!(result["phone"], "138****5678");
    }

    #[test]
    fn strategy_engine_remove_rules_for_field() {
        let mut e = MaskingStrategyEngine::new()
            .add_rule(FieldMaskingRule::new("phone", MaskingRule::Phone))
            .add_rule(FieldMaskingRule::new("email", MaskingRule::Email));
        let removed = e.remove_rules_for_field("phone");
        assert_eq!(removed, 1);
        assert_eq!(e.rule_count(), 1);
    }

    #[test]
    fn strategy_engine_clear() {
        let mut e = MaskingStrategyEngine::new()
            .add_rule(FieldMaskingRule::new("phone", MaskingRule::Phone));
        e.clear();
        assert_eq!(e.rule_count(), 0);
    }

    #[test]
    fn strategy_engine_effective_rule_none() {
        let e = MaskingStrategyEngine::new();
        let data = HashMap::new();
        assert!(e.effective_rule("phone", &data).is_none());
    }

    // ----- PipelineStage -----

    #[test]
    fn pipeline_stage_new() {
        let s = PipelineStage::new("stage1");
        assert_eq!(s.name(), "stage1");
        assert_eq!(s.rule_count(), 0);
    }

    #[test]
    fn pipeline_stage_with_rule() {
        let s = PipelineStage::new("stage1").with_rule("phone", MaskingRule::Phone);
        assert_eq!(s.rule_count(), 1);
    }

    #[test]
    fn pipeline_stage_apply() {
        let s = PipelineStage::new("stage1").with_rule("phone", MaskingRule::Phone);
        let mut data = HashMap::new();
        data.insert("phone".to_string(), "13812345678".to_string());
        let result = s.apply(&data);
        assert_eq!(result["phone"], "138****5678");
    }

    #[test]
    fn pipeline_stage_apply_to_json() {
        let s = PipelineStage::new("stage1").with_rule("phone", MaskingRule::Phone);
        let json = r#"{"phone":"13812345678"}"#;
        let result = s.apply_to_json(json);
        assert!(result.contains("138****5678"));
    }

    // ----- MaskingPipeline -----

    #[test]
    fn pipeline_default_empty() {
        let p = MaskingPipeline::new();
        assert_eq!(p.stage_count(), 0);
    }

    #[test]
    fn pipeline_add_stage() {
        let p = MaskingPipeline::new().add_stage(PipelineStage::new("s1"));
        assert_eq!(p.stage_count(), 1);
        assert_eq!(p.stage_names(), vec!["s1"]);
    }

    #[test]
    fn pipeline_apply_single_stage() {
        let p = MaskingPipeline::new()
            .add_stage(PipelineStage::new("mask").with_rule("phone", MaskingRule::Phone));
        let mut data = HashMap::new();
        data.insert("phone".to_string(), "13812345678".to_string());
        let result = p.apply_to_map(&data);
        assert_eq!(result["phone"], "138****5678");
    }

    #[test]
    fn pipeline_apply_multi_stage() {
        let p = MaskingPipeline::new()
            .add_stage(
                PipelineStage::new("type_mask")
                    .with_rule("phone", MaskingRule::Phone)
                    .with_rule("email", MaskingRule::Email),
            )
            .add_stage(PipelineStage::new("name_mask").with_rule("name", MaskingRule::Name));
        let mut data = HashMap::new();
        data.insert("phone".to_string(), "13812345678".to_string());
        data.insert("email".to_string(), "test@example.com".to_string());
        data.insert("name".to_string(), "Alice".to_string());
        let result = p.apply_to_map(&data);
        assert_eq!(result["phone"], "138****5678");
        assert_eq!(result["email"], "t***@example.com");
        assert_eq!(result["name"], "A****");
    }

    #[test]
    fn pipeline_apply_to_json() {
        let p = MaskingPipeline::new()
            .add_stage(PipelineStage::new("mask").with_rule("phone", MaskingRule::Phone));
        let json = r#"{"phone":"13812345678"}"#;
        let result = p.apply_to_json(json);
        assert!(result.contains("138****5678"));
    }

    #[test]
    fn pipeline_apply_to_json_invalid() {
        let p = MaskingPipeline::new().add_stage(PipelineStage::new("mask"));
        assert_eq!(p.apply_to_json("not json"), "not json");
    }

    #[test]
    fn pipeline_clear() {
        let mut p = MaskingPipeline::new().add_stage(PipelineStage::new("s1"));
        p.clear();
        assert_eq!(p.stage_count(), 0);
    }

    #[test]
    fn pipeline_empty_passthrough() {
        let p = MaskingPipeline::new();
        let mut data = HashMap::new();
        data.insert("phone".to_string(), "13812345678".to_string());
        let result = p.apply_to_map(&data);
        assert_eq!(result["phone"], "13812345678");
    }

    // ----- MaskingValidator -----

    #[test]
    fn validator_is_masked() {
        assert!(MaskingValidator::is_masked("13812345678", "138****5678"));
        assert!(!MaskingValidator::is_masked("same", "same"));
        assert!(MaskingValidator::is_masked("", ""));
    }

    #[test]
    fn validator_contains_mask_char() {
        assert!(MaskingValidator::contains_mask_char("138****5678"));
        assert!(!MaskingValidator::contains_mask_char("13812345678"));
    }

    #[test]
    fn validator_no_sensitive_substring() {
        assert!(MaskingValidator::no_sensitive_substring(
            "138****5678",
            "1234"
        ));
        assert!(!MaskingValidator::no_sensitive_substring(
            "13812345678",
            "1234"
        ));
    }

    #[test]
    fn validator_no_sensitive_substring_short() {
        // 短子串（≤2）不检查
        assert!(MaskingValidator::no_sensitive_substring("ab", "ab"));
    }

    #[test]
    fn validator_validate_map() {
        let mut original = HashMap::new();
        original.insert("phone".to_string(), "13812345678".to_string());
        original.insert("name".to_string(), "Alice".to_string());
        let mut masked = HashMap::new();
        masked.insert("phone".to_string(), "138****5678".to_string());
        masked.insert("name".to_string(), "Alice".to_string());
        let unmasked = MaskingValidator::validate_map(&original, &masked);
        // name 未被脱敏
        assert!(unmasked.contains(&"name".to_string()));
        assert!(!unmasked.contains(&"phone".to_string()));
    }
}
