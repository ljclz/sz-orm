//! 查询历史学习（TASK-032）

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 查询历史记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryHistoryEntry {
    pub nl_query: String,
    pub generated_sql: String,
    pub success: bool,
    pub user_feedback: Option<bool>,
    pub timestamp: String,
}

/// 学习到的模式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearnedPattern {
    pub pattern: String,
    pub sql_template: String,
    pub frequency: usize,
    pub success_rate: f64,
}

/// 查询历史学习器
pub struct HistoryLearner {
    history: Vec<QueryHistoryEntry>,
    patterns: HashMap<String, LearnedPattern>,
}

impl HistoryLearner {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            patterns: HashMap::new(),
        }
    }

    /// 记录查询历史
    pub fn record(&mut self, entry: QueryHistoryEntry) {
        self.update_patterns(&entry);
        self.history.push(entry);
    }

    /// 从历史中学习模式
    fn update_patterns(&mut self, entry: &QueryHistoryEntry) {
        let pattern = Self::extract_pattern(&entry.nl_query);
        let pattern_entry = self
            .patterns
            .entry(pattern.clone())
            .or_insert(LearnedPattern {
                pattern: pattern.clone(),
                sql_template: entry.generated_sql.clone(),
                frequency: 0,
                success_rate: 0.0,
            });

        pattern_entry.frequency += 1;
        let total = self
            .history
            .iter()
            .filter(|h| Self::extract_pattern(&h.nl_query) == pattern)
            .count()
            + 1;
        let successes = self
            .history
            .iter()
            .filter(|h| Self::extract_pattern(&h.nl_query) == pattern && h.success)
            .count()
            + if entry.success { 1 } else { 0 };
        pattern_entry.success_rate = successes as f64 / total as f64;
    }

    /// 提取查询模式（简化：取关键词）
    fn extract_pattern(nl: &str) -> String {
        let lower = nl.to_lowercase();
        let mut keywords = Vec::new();

        let pattern_keywords = [
            "查询", "统计", "计算", "排序", "分组", "筛选", "count", "sum", "avg", "where",
            "order", "group", "所有", "活跃", "最近", "最大", "最小",
        ];

        for kw in pattern_keywords {
            if lower.contains(kw) {
                keywords.push(kw.to_string());
            }
        }

        if keywords.is_empty() {
            "generic".to_string()
        } else {
            keywords.join("+")
        }
    }

    /// 根据自然语言查询推荐 SQL 模板
    pub fn recommend(&self, nl_query: &str) -> Option<&LearnedPattern> {
        let pattern = Self::extract_pattern(nl_query);
        self.patterns.get(&pattern).filter(|p| p.success_rate > 0.5)
    }

    /// 获取所有学习到的模式
    pub fn patterns(&self) -> Vec<&LearnedPattern> {
        let mut patterns: Vec<_> = self.patterns.values().collect();
        patterns.sort_by_key(|a| std::cmp::Reverse(a.frequency));
        patterns
    }

    /// 获取历史记录
    pub fn history(&self) -> &[QueryHistoryEntry] {
        &self.history
    }

    /// 导出学习数据
    pub fn export(&self) -> serde_json::Value {
        serde_json::json!({
            "history": self.history,
            "patterns": self.patterns.values().collect::<Vec<_>>(),
        })
    }
}

impl Default for HistoryLearner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(nl: &str, sql: &str, success: bool) -> QueryHistoryEntry {
        QueryHistoryEntry {
            nl_query: nl.to_string(),
            generated_sql: sql.to_string(),
            success,
            user_feedback: None,
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_record_and_learn() {
        let mut learner = HistoryLearner::new();
        learner.record(make_entry("查询所有用户", "SELECT * FROM users", true));
        learner.record(make_entry("查询所有订单", "SELECT * FROM orders", true));
        learner.record(make_entry("查询所有产品", "SELECT * FROM products", true));

        let patterns = learner.patterns();
        assert!(!patterns.is_empty(), "应学习到模式");
    }

    #[test]
    fn test_recommend_from_history() {
        let mut learner = HistoryLearner::new();
        learner.record(make_entry("查询所有用户", "SELECT * FROM users", true));
        learner.record(make_entry("查询所有订单", "SELECT * FROM orders", true));

        let recommended = learner.recommend("查询所有产品");
        assert!(recommended.is_some(), "应能推荐相似模式");
    }

    #[test]
    fn test_no_recommendation_for_low_success() {
        let mut learner = HistoryLearner::new();
        learner.record(make_entry("统计用户", "SELECT COUNT(*) FROM users", false));
        learner.record(make_entry("统计用户", "SELECT COUNT(*) FROM users", false));
        learner.record(make_entry("统计用户", "SELECT COUNT(*) FROM users", false));

        let recommended = learner.recommend("统计订单");
        assert!(recommended.is_none(), "成功率低不应推荐");
    }

    #[test]
    fn test_pattern_extraction() {
        let p1 = HistoryLearner::extract_pattern("查询所有用户");
        let p2 = HistoryLearner::extract_pattern("查询所有订单");
        assert_eq!(p1, p2, "相似查询应有相同模式");
    }

    #[test]
    fn test_export() {
        let mut learner = HistoryLearner::new();
        learner.record(make_entry("查询用户", "SELECT * FROM users", true));

        let exported = learner.export();
        assert!(exported["history"].is_array());
        assert!(exported["patterns"].is_array());
    }
}
