//! 查询模式分析器
//!
//! 提供 [`QueryPatternAnalyzer`] 分析查询日志，识别重复模式、
//! 参数化模板、热点表/列等。

use std::collections::HashMap;
use std::fmt;

/// 查询记录
#[derive(Debug, Clone)]
pub struct QueryRecord {
    /// 原始 SQL
    pub sql: String,
    /// 执行时间戳（Unix 毫秒）
    pub timestamp: u64,
    /// 耗时（毫秒）
    pub elapsed_ms: u64,
    /// 影响行数
    pub rows: u64,
    /// 客户端
    pub client: String,
}

impl QueryRecord {
    /// 创建新查询记录
    #[must_use]
    pub fn new(sql: &str, elapsed_ms: u64) -> Self {
        Self {
            sql: sql.to_string(),
            timestamp: 0,
            elapsed_ms,
            rows: 0,
            client: String::new(),
        }
    }

    /// 设置时间戳
    #[must_use]
    pub fn with_timestamp(mut self, ts: u64) -> Self {
        self.timestamp = ts;
        self
    }

    /// 设置行数
    #[must_use]
    pub fn with_rows(mut self, rows: u64) -> Self {
        self.rows = rows;
        self
    }

    /// 设置客户端
    #[must_use]
    pub fn with_client(mut self, client: &str) -> Self {
        self.client = client.to_string();
        self
    }
}

/// 参数化查询模板
#[derive(Debug, Clone)]
pub struct QueryTemplate {
    /// 模板 SQL（参数替换为 ?）
    pub template: String,
    /// 匹配的原始 SQL 数量
    pub match_count: u64,
    /// 总耗时
    pub total_elapsed_ms: u64,
    /// 最大耗时
    pub max_elapsed_ms: u64,
    /// 最小耗时
    pub min_elapsed_ms: u64,
    /// 平均耗时
    pub avg_elapsed_ms: f64,
    /// 总行数
    pub total_rows: u64,
    /// 涉及的表
    pub tables: Vec<String>,
}

impl QueryTemplate {
    /// 创建新模板
    #[must_use]
    pub fn new(template: &str) -> Self {
        Self {
            template: template.to_string(),
            match_count: 0,
            total_elapsed_ms: 0,
            max_elapsed_ms: 0,
            min_elapsed_ms: u64::MAX,
            avg_elapsed_ms: 0.0,
            total_rows: 0,
            tables: Vec::new(),
        }
    }

    /// 添加一次匹配
    pub fn add_match(&mut self, elapsed_ms: u64, rows: u64) {
        self.match_count += 1;
        self.total_elapsed_ms += elapsed_ms;
        self.max_elapsed_ms = self.max_elapsed_ms.max(elapsed_ms);
        self.min_elapsed_ms = self.min_elapsed_ms.min(elapsed_ms);
        self.total_rows += rows;
        self.avg_elapsed_ms = self.total_elapsed_ms as f64 / self.match_count as f64;
    }

    /// 是否为慢查询模板
    #[must_use]
    pub fn is_slow(&self, threshold_ms: u64) -> bool {
        self.avg_elapsed_ms > threshold_ms as f64
    }
}

/// 查询模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryPatternKind {
    /// 简单等值查询
    SimpleEquality,
    /// 范围查询
    Range,
    /// 多表 JOIN
    Join,
    /// 聚合查询
    Aggregate,
    /// 子查询
    Subquery,
    /// 排序查询
    Sorted,
    /// 分页查询
    Paginated,
    /// 批量插入
    BulkInsert,
    /// 批量更新
    BulkUpdate,
    /// 未知
    Unknown,
}

impl QueryPatternKind {
    /// 返回描述
    #[must_use]
    pub fn description(&self) -> &'static str {
        match self {
            QueryPatternKind::SimpleEquality => "simple equality",
            QueryPatternKind::Range => "range",
            QueryPatternKind::Join => "join",
            QueryPatternKind::Aggregate => "aggregate",
            QueryPatternKind::Subquery => "subquery",
            QueryPatternKind::Sorted => "sorted",
            QueryPatternKind::Paginated => "paginated",
            QueryPatternKind::BulkInsert => "bulk insert",
            QueryPatternKind::BulkUpdate => "bulk update",
            QueryPatternKind::Unknown => "unknown",
        }
    }
}

/// 查询模式分析器
#[derive(Debug, Default)]
pub struct QueryPatternAnalyzer {
    /// 已收集的查询记录
    records: Vec<QueryRecord>,
    /// 已识别的模板
    templates: HashMap<String, QueryTemplate>,
}

impl QueryPatternAnalyzer {
    /// 创建新的分析器
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加查询记录
    pub fn add_record(&mut self, record: QueryRecord) {
        self.records.push(record);
    }

    /// 批量添加查询记录
    pub fn add_records(&mut self, records: Vec<QueryRecord>) {
        self.records.extend(records);
    }

    /// 分析查询模式
    pub fn analyze(&mut self) {
        self.templates.clear();
        for record in &self.records {
            let template = Self::parameterize_sql(&record.sql);
            let tables = Self::extract_tables(&record.sql);
            let entry = self.templates.entry(template.clone()).or_insert_with(|| {
                let mut t = QueryTemplate::new(&template);
                t.tables = tables;
                t
            });
            entry.add_match(record.elapsed_ms, record.rows);
        }
    }

    /// 参数化 SQL（将字面量替换为 ?）
    #[must_use]
    pub fn parameterize_sql(sql: &str) -> String {
        let mut result = String::with_capacity(sql.len());
        let mut in_string = false;
        let mut in_number = false;
        let chars: Vec<char> = sql.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let ch = chars[i];
            if ch == '\'' {
                in_string = !in_string;
                result.push(ch);
                i += 1;
                continue;
            }
            if in_string {
                result.push(ch);
                i += 1;
                continue;
            }
            if ch.is_ascii_digit() {
                if !in_number {
                    in_number = true;
                    result.push('?');
                }
                i += 1;
                continue;
            }
            if in_number {
                in_number = false;
            }
            result.push(ch);
            i += 1;
        }
        result
    }

    /// 从 SQL 提取表名
    #[must_use]
    pub fn extract_tables(sql: &str) -> Vec<String> {
        let upper = sql.to_uppercase();
        let mut tables = Vec::new();
        let mut in_from = false;
        for word in upper.split_whitespace() {
            let word = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
            if word == "FROM" || word == "JOIN" || word == "INTO" || word == "UPDATE" {
                in_from = true;
                continue;
            }
            if in_from {
                if !word.is_empty()
                    && !matches!(
                        word,
                        "WHERE"
                            | "SET"
                            | "VALUES"
                            | "SELECT"
                            | "LEFT"
                            | "RIGHT"
                            | "INNER"
                            | "OUTER"
                            | "ON"
                            | "AND"
                            | "OR"
                            | "GROUP"
                            | "ORDER"
                            | "LIMIT"
                            | "OFFSET"
                            | "HAVING"
                            | "AS"
                    )
                {
                    tables.push(word.to_lowercase());
                }
                in_from = false;
            }
        }
        tables
    }

    /// 识别查询模式类型
    #[must_use]
    pub fn classify_pattern(sql: &str) -> QueryPatternKind {
        let upper = sql.to_uppercase();
        if upper.contains("INSERT INTO")
            && upper.contains("VALUES")
            && upper.matches('(').count() > 1
        {
            return QueryPatternKind::BulkInsert;
        }
        if upper.contains("JOIN") {
            return QueryPatternKind::Join;
        }
        if upper.contains("GROUP BY") || upper.contains("COUNT(") || upper.contains("SUM(") {
            return QueryPatternKind::Aggregate;
        }
        if upper.contains("SUBQUERY") || upper.contains(" EXISTS") || upper.contains(" IN (SELECT")
        {
            return QueryPatternKind::Subquery;
        }
        if upper.contains("ORDER BY") {
            return QueryPatternKind::Sorted;
        }
        if upper.contains("LIMIT") || upper.contains("OFFSET") {
            return QueryPatternKind::Paginated;
        }
        if upper.contains(">") || upper.contains("<") || upper.contains("BETWEEN") {
            return QueryPatternKind::Range;
        }
        if upper.contains("WHERE") && upper.contains('=') {
            return QueryPatternKind::SimpleEquality;
        }
        QueryPatternKind::Unknown
    }

    /// 获取所有模板
    #[must_use]
    pub fn templates(&self) -> Vec<&QueryTemplate> {
        self.templates.values().collect()
    }

    /// 获取慢查询模板
    #[must_use]
    pub fn slow_templates(&self, threshold_ms: u64) -> Vec<&QueryTemplate> {
        self.templates
            .values()
            .filter(|t| t.is_slow(threshold_ms))
            .collect()
    }

    /// 获取最频繁的模板
    #[must_use]
    pub fn most_frequent_templates(&self, limit: usize) -> Vec<&QueryTemplate> {
        let mut templates: Vec<&QueryTemplate> = self.templates.values().collect();
        templates.sort_by_key(|t| std::cmp::Reverse(t.match_count));
        templates.into_iter().take(limit).collect()
    }

    /// 获取最耗时的模板
    #[must_use]
    pub fn most_expensive_templates(&self, limit: usize) -> Vec<&QueryTemplate> {
        let mut templates: Vec<&QueryTemplate> = self.templates.values().collect();
        templates.sort_by_key(|t| std::cmp::Reverse(t.total_elapsed_ms));
        templates.into_iter().take(limit).collect()
    }

    /// 获取热点表
    #[must_use]
    pub fn hot_tables(&self) -> Vec<(String, u64)> {
        let mut table_counts: HashMap<String, u64> = HashMap::new();
        for template in self.templates.values() {
            for table in &template.tables {
                *table_counts.entry(table.clone()).or_insert(0) += template.match_count;
            }
        }
        let mut tables: Vec<(String, u64)> = table_counts.into_iter().collect();
        tables.sort_by_key(|t| std::cmp::Reverse(t.1));
        tables
    }

    /// 记录数
    #[must_use]
    pub fn record_count(&self) -> usize {
        self.records.len()
    }

    /// 模板数
    #[must_use]
    pub fn template_count(&self) -> usize {
        self.templates.len()
    }

    /// 总耗时
    #[must_use]
    pub fn total_elapsed_ms(&self) -> u64 {
        self.records.iter().map(|r| r.elapsed_ms).sum()
    }

    /// 平均耗时
    #[must_use]
    pub fn avg_elapsed_ms(&self) -> f64 {
        if self.records.is_empty() {
            0.0
        } else {
            self.total_elapsed_ms() as f64 / self.records.len() as f64
        }
    }
}

impl fmt::Display for QueryPatternAnalyzer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "QueryPatternAnalyzer(records={}, templates={})",
            self.record_count(),
            self.template_count()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_record_new() {
        let r = QueryRecord::new("SELECT 1", 10);
        assert_eq!(r.sql, "SELECT 1");
        assert_eq!(r.elapsed_ms, 10);
    }

    #[test]
    fn test_query_template_new() {
        let t = QueryTemplate::new("SELECT * FROM t WHERE id = ?");
        assert_eq!(t.match_count, 0);
    }

    #[test]
    fn test_query_template_add_match() {
        let mut t = QueryTemplate::new("test");
        t.add_match(10, 1);
        t.add_match(20, 2);
        assert_eq!(t.match_count, 2);
        assert_eq!(t.total_elapsed_ms, 30);
        assert_eq!(t.max_elapsed_ms, 20);
        assert_eq!(t.min_elapsed_ms, 10);
        assert!((t.avg_elapsed_ms - 15.0).abs() < 1e-10);
    }

    #[test]
    fn test_query_template_is_slow() {
        let mut t = QueryTemplate::new("test");
        t.add_match(200, 1);
        assert!(t.is_slow(100));
        assert!(!t.is_slow(300));
    }

    #[test]
    fn test_query_pattern_kind_description() {
        assert_eq!(QueryPatternKind::Join.description(), "join");
        assert_eq!(QueryPatternKind::Aggregate.description(), "aggregate");
    }

    #[test]
    fn test_parameterize_sql_numbers() {
        let p = QueryPatternAnalyzer::parameterize_sql("SELECT * FROM t WHERE id = 42");
        assert_eq!(p, "SELECT * FROM t WHERE id = ?");
    }

    #[test]
    fn test_parameterize_sql_string() {
        let p = QueryPatternAnalyzer::parameterize_sql("SELECT * FROM t WHERE name = 'abc123'");
        assert_eq!(p, "SELECT * FROM t WHERE name = 'abc123'");
    }

    #[test]
    fn test_parameterize_sql_multiple() {
        let p = QueryPatternAnalyzer::parameterize_sql("SELECT * FROM t WHERE a = 1 AND b = 2");
        assert_eq!(p, "SELECT * FROM t WHERE a = ? AND b = ?");
    }

    #[test]
    fn test_extract_tables() {
        let tables = QueryPatternAnalyzer::extract_tables("SELECT * FROM users WHERE id = 1");
        assert!(tables.contains(&"users".to_string()));
    }

    #[test]
    fn test_extract_tables_join() {
        let tables = QueryPatternAnalyzer::extract_tables(
            "SELECT * FROM orders JOIN users ON orders.user_id = users.id",
        );
        assert!(tables.contains(&"orders".to_string()));
        assert!(tables.contains(&"users".to_string()));
    }

    #[test]
    fn test_classify_pattern_join() {
        let kind = QueryPatternAnalyzer::classify_pattern("SELECT * FROM a JOIN b ON a.id = b.id");
        assert_eq!(kind, QueryPatternKind::Join);
    }

    #[test]
    fn test_classify_pattern_aggregate() {
        let kind = QueryPatternAnalyzer::classify_pattern("SELECT COUNT(*) FROM t");
        assert_eq!(kind, QueryPatternKind::Aggregate);
    }

    #[test]
    fn test_classify_pattern_sorted() {
        let kind = QueryPatternAnalyzer::classify_pattern("SELECT * FROM t ORDER BY id");
        assert_eq!(kind, QueryPatternKind::Sorted);
    }

    #[test]
    fn test_classify_pattern_paginated() {
        let kind = QueryPatternAnalyzer::classify_pattern("SELECT * FROM t LIMIT 10 OFFSET 0");
        assert_eq!(kind, QueryPatternKind::Paginated);
    }

    #[test]
    fn test_classify_pattern_range() {
        let kind = QueryPatternAnalyzer::classify_pattern("SELECT * FROM t WHERE id > 100");
        assert_eq!(kind, QueryPatternKind::Range);
    }

    #[test]
    fn test_classify_pattern_equality() {
        let kind = QueryPatternAnalyzer::classify_pattern("SELECT * FROM t WHERE id = 1");
        assert_eq!(kind, QueryPatternKind::SimpleEquality);
    }

    #[test]
    fn test_analyzer_add_record() {
        let mut a = QueryPatternAnalyzer::new();
        a.add_record(QueryRecord::new("SELECT 1", 10));
        assert_eq!(a.record_count(), 1);
    }

    #[test]
    fn test_analyzer_analyze() {
        let mut a = QueryPatternAnalyzer::new();
        a.add_record(QueryRecord::new("SELECT * FROM t WHERE id = 1", 10));
        a.add_record(QueryRecord::new("SELECT * FROM t WHERE id = 2", 20));
        a.analyze();
        assert_eq!(a.template_count(), 1);
    }

    #[test]
    fn test_analyzer_slow_templates() {
        let mut a = QueryPatternAnalyzer::new();
        a.add_record(QueryRecord::new("SELECT * FROM t WHERE id = 1", 200));
        a.analyze();
        let slow = a.slow_templates(100);
        assert_eq!(slow.len(), 1);
    }

    #[test]
    fn test_analyzer_most_frequent() {
        let mut a = QueryPatternAnalyzer::new();
        a.add_record(QueryRecord::new("SELECT * FROM t WHERE id = 1", 10));
        a.add_record(QueryRecord::new("SELECT * FROM t WHERE id = 2", 10));
        a.add_record(QueryRecord::new("SELECT * FROM u WHERE id = 1", 10));
        a.analyze();
        let freq = a.most_frequent_templates(2);
        assert!(!freq.is_empty());
    }

    #[test]
    fn test_analyzer_most_expensive() {
        let mut a = QueryPatternAnalyzer::new();
        a.add_record(QueryRecord::new("SELECT * FROM t WHERE id = 1", 100));
        a.add_record(QueryRecord::new("SELECT * FROM u WHERE id = 1", 10));
        a.analyze();
        let expensive = a.most_expensive_templates(1);
        assert!(!expensive.is_empty());
    }

    #[test]
    fn test_analyzer_hot_tables() {
        let mut a = QueryPatternAnalyzer::new();
        a.add_record(QueryRecord::new("SELECT * FROM users WHERE id = 1", 10));
        a.add_record(QueryRecord::new("SELECT * FROM users WHERE id = 2", 10));
        a.add_record(QueryRecord::new("SELECT * FROM orders WHERE id = 1", 10));
        a.analyze();
        let hot = a.hot_tables();
        assert!(!hot.is_empty());
        assert_eq!(hot[0].0, "users");
    }

    #[test]
    fn test_analyzer_total_elapsed() {
        let mut a = QueryPatternAnalyzer::new();
        a.add_record(QueryRecord::new("SELECT 1", 10));
        a.add_record(QueryRecord::new("SELECT 2", 20));
        assert_eq!(a.total_elapsed_ms(), 30);
    }

    #[test]
    fn test_analyzer_avg_elapsed() {
        let mut a = QueryPatternAnalyzer::new();
        a.add_record(QueryRecord::new("SELECT 1", 10));
        a.add_record(QueryRecord::new("SELECT 2", 20));
        assert!((a.avg_elapsed_ms() - 15.0).abs() < 1e-10);
    }

    #[test]
    fn test_analyzer_display() {
        let a = QueryPatternAnalyzer::new();
        let s = format!("{}", a);
        assert!(s.contains("QueryPatternAnalyzer"));
    }
}
