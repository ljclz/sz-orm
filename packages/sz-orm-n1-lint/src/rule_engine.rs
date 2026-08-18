//! N+1 查询检测规则引擎
//!
//! 可配置的规则引擎，按规则集检测 N+1 查询模式。
//! 支持自定义规则、规则优先级、规则启用/禁用。

use std::collections::HashMap;

use crate::{N1Finding, N1Pattern, N1Severity};

/// 检测规则
#[derive(Debug, Clone)]
pub struct DetectionRule {
    /// 规则 ID
    pub rule_id: String,
    /// 规则描述
    pub description: String,
    /// 匹配的模式
    pub pattern: N1Pattern,
    /// 规则优先级（0=最高）
    pub priority: u8,
    /// 是否启用
    pub enabled: bool,
}

impl DetectionRule {
    /// 创建新规则
    pub fn new(
        rule_id: impl Into<String>,
        description: impl Into<String>,
        pattern: N1Pattern,
    ) -> Self {
        Self {
            rule_id: rule_id.into(),
            description: description.into(),
            pattern,
            priority: N1Severity::from_pattern(pattern) as u8,
            enabled: true,
        }
    }

    /// 设置优先级
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }

    /// 禁用规则
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }
}

/// 规则匹配结果
#[derive(Debug, Clone)]
pub struct RuleMatchResult {
    /// 匹配的规则 ID
    pub rule_id: String,
    /// 匹配的检测结果
    pub finding: N1Finding,
    /// 规则优先级
    pub priority: u8,
}

/// N+1 查询检测规则引擎
///
/// 按配置的规则集检测 N+1 查询，支持规则启用/禁用与优先级排序。
pub struct N1DetectionRuleEngine {
    rules: Vec<DetectionRule>,
}

impl N1DetectionRuleEngine {
    /// 创建空规则引擎
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// 创建带默认规则集的引擎
    pub fn with_default_rules() -> Self {
        let mut engine = Self::new();
        engine.add_rule(DetectionRule::new(
            "query-in-loop",
            "检测循环体内直接查询调用",
            N1Pattern::QueryInLoop,
        ));
        engine.add_rule(DetectionRule::new(
            "conditional-query-in-loop",
            "检测循环体内条件分支中的查询调用",
            N1Pattern::ConditionalQueryInLoop,
        ));
        engine.add_rule(DetectionRule::new(
            "missing-eager-load",
            "检测可批量替代的单条查询",
            N1Pattern::MissingEagerLoadHint,
        ));
        engine
    }

    /// 添加规则
    pub fn add_rule(&mut self, rule: DetectionRule) {
        self.rules.push(rule);
        self.rules.sort_by_key(|r| r.priority);
    }

    /// 按 ID 禁用规则
    pub fn disable_rule(&mut self, rule_id: &str) -> bool {
        let mut found = false;
        for rule in &mut self.rules {
            if rule.rule_id == rule_id {
                rule.enabled = false;
                found = true;
            }
        }
        found
    }

    /// 按 ID 启用规则
    pub fn enable_rule(&mut self, rule_id: &str) -> bool {
        let mut found = false;
        for rule in &mut self.rules {
            if rule.rule_id == rule_id {
                rule.enabled = true;
                found = true;
            }
        }
        found
    }

    /// 获取所有规则
    pub fn rules(&self) -> &[DetectionRule] {
        &self.rules
    }

    /// 启用的规则数
    pub fn enabled_rule_count(&self) -> usize {
        self.rules.iter().filter(|r| r.enabled).count()
    }

    /// 对检测结果应用规则
    ///
    /// 仅保留匹配启用规则的检测结果，并附加规则 ID 与优先级。
    pub fn apply(&self, findings: &[N1Finding]) -> Vec<RuleMatchResult> {
        let mut results = Vec::new();
        for finding in findings {
            for rule in &self.rules {
                if rule.enabled && rule.pattern == finding.pattern {
                    results.push(RuleMatchResult {
                        rule_id: rule.rule_id.clone(),
                        finding: finding.clone(),
                        priority: rule.priority,
                    });
                    break;
                }
            }
        }
        results.sort_by_key(|r| r.priority);
        results
    }

    /// 按规则 ID 分组统计
    pub fn group_by_rule(&self, findings: &[N1Finding]) -> HashMap<String, Vec<N1Finding>> {
        let mut groups: HashMap<String, Vec<N1Finding>> = HashMap::new();
        for finding in findings {
            for rule in &self.rules {
                if rule.enabled && rule.pattern == finding.pattern {
                    groups
                        .entry(rule.rule_id.clone())
                        .or_default()
                        .push(finding.clone());
                    break;
                }
            }
        }
        groups
    }

    /// 清空规则
    pub fn clear(&mut self) {
        self.rules.clear();
    }
}

impl Default for N1DetectionRuleEngine {
    fn default() -> Self {
        Self::with_default_rules()
    }
}

/// 规则引擎统计
#[derive(Debug, Clone, Default)]
pub struct RuleEngineStats {
    /// 总匹配数
    pub total_matches: usize,
    /// 按规则 ID 统计
    pub matches_by_rule: HashMap<String, usize>,
    /// 按模式统计
    pub matches_by_pattern: HashMap<N1Pattern, usize>,
}

impl RuleEngineStats {
    /// 创建空统计
    pub fn new() -> Self {
        Self::default()
    }

    /// 从匹配结果构建统计
    pub fn from_results(results: &[RuleMatchResult]) -> Self {
        let mut stats = Self::new();
        stats.total_matches = results.len();
        for result in results {
            *stats
                .matches_by_rule
                .entry(result.rule_id.clone())
                .or_insert(0) += 1;
            *stats
                .matches_by_pattern
                .entry(result.finding.pattern)
                .or_insert(0) += 1;
        }
        stats
    }

    /// 某规则的匹配数
    pub fn count_for_rule(&self, rule_id: &str) -> usize {
        self.matches_by_rule.get(rule_id).copied().unwrap_or(0)
    }

    /// 某模式的匹配数
    pub fn count_for_pattern(&self, pattern: N1Pattern) -> usize {
        self.matches_by_pattern.get(&pattern).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(pattern: N1Pattern, line: usize) -> N1Finding {
        N1Finding {
            pattern,
            file: "test.rs".to_string(),
            line,
            message: "test".to_string(),
        }
    }

    // --- DetectionRule tests ---

    #[test]
    fn rule_new() {
        let rule = DetectionRule::new("r1", "test rule", N1Pattern::QueryInLoop);
        assert_eq!(rule.rule_id, "r1");
        assert_eq!(rule.description, "test rule");
        assert_eq!(rule.pattern, N1Pattern::QueryInLoop);
        assert!(rule.enabled);
    }

    #[test]
    fn rule_with_priority() {
        let rule = DetectionRule::new("r1", "test", N1Pattern::QueryInLoop).with_priority(5);
        assert_eq!(rule.priority, 5);
    }

    #[test]
    fn rule_disabled() {
        let rule = DetectionRule::new("r1", "test", N1Pattern::QueryInLoop).disabled();
        assert!(!rule.enabled);
    }

    #[test]
    fn rule_priority_from_pattern() {
        let r1 = DetectionRule::new("r1", "test", N1Pattern::QueryInLoop);
        let r2 = DetectionRule::new("r2", "test", N1Pattern::ConditionalQueryInLoop);
        let r3 = DetectionRule::new("r3", "test", N1Pattern::MissingEagerLoadHint);
        assert!(r1.priority >= r2.priority);
        assert!(r2.priority >= r3.priority);
    }

    // --- N1DetectionRuleEngine tests ---

    #[test]
    fn engine_new_empty() {
        let e = N1DetectionRuleEngine::new();
        assert_eq!(e.rules().len(), 0);
        assert_eq!(e.enabled_rule_count(), 0);
    }

    #[test]
    fn engine_with_default_rules() {
        let e = N1DetectionRuleEngine::with_default_rules();
        assert_eq!(e.rules().len(), 3);
        assert_eq!(e.enabled_rule_count(), 3);
    }

    #[test]
    fn engine_add_rule() {
        let mut e = N1DetectionRuleEngine::new();
        e.add_rule(DetectionRule::new("r1", "test", N1Pattern::QueryInLoop));
        assert_eq!(e.rules().len(), 1);
    }

    #[test]
    fn engine_disable_rule() {
        let mut e = N1DetectionRuleEngine::with_default_rules();
        assert!(e.disable_rule("query-in-loop"));
        assert_eq!(e.enabled_rule_count(), 2);
    }

    #[test]
    fn engine_disable_nonexistent() {
        let mut e = N1DetectionRuleEngine::with_default_rules();
        assert!(!e.disable_rule("nonexistent"));
    }

    #[test]
    fn engine_enable_rule() {
        let mut e = N1DetectionRuleEngine::with_default_rules();
        e.disable_rule("query-in-loop");
        assert!(e.enable_rule("query-in-loop"));
        assert_eq!(e.enabled_rule_count(), 3);
    }

    #[test]
    fn engine_apply_filters_disabled() {
        let mut e = N1DetectionRuleEngine::with_default_rules();
        e.disable_rule("query-in-loop");
        let findings = vec![finding(N1Pattern::QueryInLoop, 1)];
        let results = e.apply(&findings);
        assert!(results.is_empty());
    }

    #[test]
    fn engine_apply_matches_enabled() {
        let e = N1DetectionRuleEngine::with_default_rules();
        let findings = vec![finding(N1Pattern::QueryInLoop, 1)];
        let results = e.apply(&findings);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].rule_id, "query-in-loop");
    }

    #[test]
    fn engine_apply_multiple_findings() {
        let e = N1DetectionRuleEngine::with_default_rules();
        let findings = vec![
            finding(N1Pattern::QueryInLoop, 1),
            finding(N1Pattern::ConditionalQueryInLoop, 2),
            finding(N1Pattern::MissingEagerLoadHint, 3),
        ];
        let results = e.apply(&findings);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn engine_apply_sorted_by_priority() {
        let e = N1DetectionRuleEngine::with_default_rules();
        let findings = vec![
            finding(N1Pattern::MissingEagerLoadHint, 1),
            finding(N1Pattern::QueryInLoop, 2),
        ];
        let results = e.apply(&findings);
        // priority 0 = 最高，升序排列后小的在前
        assert!(results[0].priority <= results[1].priority);
    }

    #[test]
    fn engine_group_by_rule() {
        let e = N1DetectionRuleEngine::with_default_rules();
        let findings = vec![
            finding(N1Pattern::QueryInLoop, 1),
            finding(N1Pattern::QueryInLoop, 2),
            finding(N1Pattern::ConditionalQueryInLoop, 3),
        ];
        let groups = e.group_by_rule(&findings);
        assert_eq!(groups.get("query-in-loop").unwrap().len(), 2);
        assert_eq!(groups.get("conditional-query-in-loop").unwrap().len(), 1);
    }

    #[test]
    fn engine_clear() {
        let mut e = N1DetectionRuleEngine::with_default_rules();
        e.clear();
        assert_eq!(e.rules().len(), 0);
    }

    #[test]
    fn engine_default() {
        let e = N1DetectionRuleEngine::default();
        assert_eq!(e.rules().len(), 3);
    }

    #[test]
    fn engine_rules_ref() {
        let e = N1DetectionRuleEngine::with_default_rules();
        assert_eq!(e.rules().len(), 3);
    }

    // --- RuleEngineStats tests ---

    #[test]
    fn stats_new_empty() {
        let s = RuleEngineStats::new();
        assert_eq!(s.total_matches, 0);
    }

    #[test]
    fn stats_from_results() {
        let e = N1DetectionRuleEngine::with_default_rules();
        let findings = vec![
            finding(N1Pattern::QueryInLoop, 1),
            finding(N1Pattern::QueryInLoop, 2),
            finding(N1Pattern::ConditionalQueryInLoop, 3),
        ];
        let results = e.apply(&findings);
        let stats = RuleEngineStats::from_results(&results);
        assert_eq!(stats.total_matches, 3);
        assert_eq!(stats.count_for_rule("query-in-loop"), 2);
        assert_eq!(stats.count_for_rule("conditional-query-in-loop"), 1);
    }

    #[test]
    fn stats_count_for_pattern() {
        let e = N1DetectionRuleEngine::with_default_rules();
        let findings = vec![
            finding(N1Pattern::QueryInLoop, 1),
            finding(N1Pattern::ConditionalQueryInLoop, 2),
        ];
        let results = e.apply(&findings);
        let stats = RuleEngineStats::from_results(&results);
        assert_eq!(stats.count_for_pattern(N1Pattern::QueryInLoop), 1);
        assert_eq!(
            stats.count_for_pattern(N1Pattern::ConditionalQueryInLoop),
            1
        );
    }

    #[test]
    fn stats_count_for_nonexistent() {
        let s = RuleEngineStats::new();
        assert_eq!(s.count_for_rule("nonexistent"), 0);
        assert_eq!(s.count_for_pattern(N1Pattern::QueryInLoop), 0);
    }
}
