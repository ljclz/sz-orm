//! SQL Server 索引优化建议
//!
//! 提供 [`IndexOptimizationAdvisor`] 基于查询模式、缺失索引 DMV、
//! 索引使用统计生成索引优化建议。

use std::fmt;

/// 索引类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexType {
    /// 聚集索引
    Clustered,
    /// 非聚集索引
    NonClustered,
    /// 唯一聚集索引
    UniqueClustered,
    /// 唯一非聚集索引
    UniqueNonClustered,
    /// 列存储索引
    Columnstore,
    /// 非聚集列存储索引
    NonClusteredColumnstore,
    /// 包含索引（WITH INCLUDE）
    Included,
    /// 筛选索引（WITH FILTER）
    Filtered,
}

impl IndexType {
    /// 返回 SQL 关键字
    #[must_use]
    pub fn as_sql(&self) -> &'static str {
        match self {
            IndexType::Clustered => "CLUSTERED",
            IndexType::NonClustered => "NONCLUSTERED",
            IndexType::UniqueClustered => "UNIQUE CLUSTERED",
            IndexType::UniqueNonClustered => "UNIQUE NONCLUSTERED",
            IndexType::Columnstore => "CLUSTERED COLUMNSTORE",
            IndexType::NonClusteredColumnstore => "NONCLUSTERED COLUMNSTORE",
            IndexType::Included => "NONCLUSTERED",
            IndexType::Filtered => "NONCLUSTERED",
        }
    }

    /// 是否为聚集索引
    #[must_use]
    pub fn is_clustered(&self) -> bool {
        matches!(
            self,
            IndexType::Clustered | IndexType::UniqueClustered | IndexType::Columnstore
        )
    }

    /// 是否为唯一索引
    #[must_use]
    pub fn is_unique(&self) -> bool {
        matches!(
            self,
            IndexType::UniqueClustered | IndexType::UniqueNonClustered
        )
    }

    /// 是否为列存储索引
    #[must_use]
    pub fn is_columnstore(&self) -> bool {
        matches!(
            self,
            IndexType::Columnstore | IndexType::NonClusteredColumnstore
        )
    }
}

impl fmt::Display for IndexType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_sql())
    }
}

/// 索引建议
#[derive(Debug, Clone)]
pub struct IndexSuggestion {
    /// 建议名
    pub name: String,
    /// 表名
    pub table: String,
    /// schema
    pub schema: String,
    /// 索引类型
    pub index_type: IndexType,
    /// 键列
    pub key_columns: Vec<String>,
    /// 包含列（INCLUDED）
    pub included_columns: Vec<String>,
    /// 筛选条件（WHERE）
    pub filter_predicate: Option<String>,
    /// 预期收益（0.0~1.0）
    pub estimated_benefit: f64,
    /// 建议原因
    pub reason: String,
}

impl IndexSuggestion {
    /// 创建新的索引建议
    #[must_use]
    pub fn new(table: &str, index_type: IndexType, key_columns: &[&str]) -> Self {
        Self {
            name: String::new(),
            table: table.to_string(),
            schema: "dbo".to_string(),
            index_type,
            key_columns: key_columns.iter().map(|s| s.to_string()).collect(),
            included_columns: Vec::new(),
            filter_predicate: None,
            estimated_benefit: 0.0,
            reason: String::new(),
        }
    }

    /// 设置建议名
    #[must_use]
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    /// 设置 schema
    #[must_use]
    pub fn with_schema(mut self, schema: &str) -> Self {
        self.schema = schema.to_string();
        self
    }

    /// 设置包含列
    #[must_use]
    pub fn with_included(mut self, cols: &[&str]) -> Self {
        self.included_columns = cols.iter().map(|s| s.to_string()).collect();
        self
    }

    /// 设置筛选条件
    #[must_use]
    pub fn with_filter(mut self, predicate: &str) -> Self {
        self.filter_predicate = Some(predicate.to_string());
        self
    }

    /// 设置预期收益
    #[must_use]
    pub fn with_benefit(mut self, benefit: f64) -> Self {
        self.estimated_benefit = benefit.clamp(0.0, 1.0);
        self
    }

    /// 设置原因
    #[must_use]
    pub fn with_reason(mut self, reason: &str) -> Self {
        self.reason = reason.to_string();
        self
    }

    /// 生成 CREATE INDEX DDL
    #[must_use]
    pub fn to_create_ddl(&self) -> String {
        let name = if self.name.is_empty() {
            format!("IX_{}_{}", self.table, self.key_columns.join("_"))
        } else {
            self.name.clone()
        };
        let key_cols = self.key_columns.join(", ");
        let mut sql = format!(
            "CREATE {} INDEX {} ON {}.{} ({})",
            self.index_type.as_sql(),
            name,
            self.schema,
            self.table,
            key_cols
        );
        if !self.included_columns.is_empty() {
            sql.push_str(&format!(" INCLUDE ({})", self.included_columns.join(", ")));
        }
        if let Some(ref filter) = self.filter_predicate {
            sql.push_str(&format!(" WHERE {filter}"));
        }
        sql.push(';');
        sql
    }

    /// 生成 DROP INDEX DDL
    #[must_use]
    pub fn to_drop_ddl(&self) -> String {
        let name = if self.name.is_empty() {
            format!("IX_{}_{}", self.table, self.key_columns.join("_"))
        } else {
            self.name.clone()
        };
        format!("DROP INDEX {} ON {};", name, self.table)
    }
}

impl fmt::Display for IndexSuggestion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_create_ddl())
    }
}

/// 索引使用统计
#[derive(Debug, Clone, Default)]
pub struct IndexUsageStats {
    /// 索引名
    pub index_name: String,
    /// 表名
    pub table_name: String,
    /// 用户查找次数
    pub user_seeks: u64,
    /// 用户扫描次数
    pub user_scans: u64,
    /// 用户查找次数（lookup）
    pub user_lookups: u64,
    /// 用户更新次数
    pub user_updates: u64,
    /// 最后用户查找时间
    pub last_user_seek: Option<String>,
}

impl IndexUsageStats {
    /// 创建新的使用统计
    #[must_use]
    pub fn new(index_name: &str, table_name: &str) -> Self {
        Self {
            index_name: index_name.to_string(),
            table_name: table_name.to_string(),
            ..Self::default()
        }
    }

    /// 总读取次数
    #[must_use]
    pub fn total_reads(&self) -> u64 {
        self.user_seeks + self.user_scans + self.user_lookups
    }

    /// 读写比
    #[must_use]
    pub fn read_write_ratio(&self) -> f64 {
        if self.user_updates == 0 {
            self.total_reads() as f64
        } else {
            self.total_reads() as f64 / self.user_updates as f64
        }
    }

    /// 是否从未使用
    #[must_use]
    pub fn is_unused(&self) -> bool {
        self.total_reads() == 0 && self.user_updates == 0
    }

    /// 使用率评分（0.0~1.0）
    #[must_use]
    pub fn usage_score(&self) -> f64 {
        let reads = self.total_reads() as f64;
        let updates = self.user_updates as f64;
        let total = reads + updates;
        if total == 0.0 {
            0.0
        } else {
            reads / total
        }
    }
}

/// 缺失索引信息（来自 DMV）
#[derive(Debug, Clone)]
pub struct MissingIndexInfo {
    /// 表名
    pub table: String,
    /// schema
    pub schema: String,
    /// 等值列
    pub equality_columns: Vec<String>,
    /// 不等值列
    pub inequality_columns: Vec<String>,
    /// 包含列
    pub included_columns: Vec<String>,
    /// 用户影响（0.0~1.0）
    pub user_impact: f64,
    /// 用户查询次数
    pub user_seeks: u64,
    /// 平均用户成本
    pub avg_user_cost: f64,
}

impl MissingIndexInfo {
    /// 创建新的缺失索引信息
    #[must_use]
    pub fn new(table: &str) -> Self {
        Self {
            table: table.to_string(),
            schema: "dbo".to_string(),
            equality_columns: Vec::new(),
            inequality_columns: Vec::new(),
            included_columns: Vec::new(),
            user_impact: 0.0,
            user_seeks: 0,
            avg_user_cost: 0.0,
        }
    }

    /// 转换为索引建议
    #[must_use]
    pub fn to_suggestion(&self) -> IndexSuggestion {
        let mut key_cols = self.equality_columns.clone();
        key_cols.extend(self.inequality_columns.iter().cloned());
        let key_refs: Vec<&str> = key_cols.iter().map(|s| s.as_str()).collect();
        let included_refs: Vec<&str> = self.included_columns.iter().map(|s| s.as_str()).collect();
        let reason = format!(
            "missing index detected by DMV (user_seeks={}, avg_cost={:.2})",
            self.user_seeks, self.avg_user_cost
        );
        IndexSuggestion::new(&self.table, IndexType::NonClustered, &key_refs)
            .with_schema(&self.schema)
            .with_included(&included_refs)
            .with_benefit(self.user_impact / 100.0)
            .with_reason(&reason)
    }

    /// 综合评分
    #[must_use]
    pub fn composite_score(&self) -> f64 {
        self.user_impact * self.user_seeks as f64 * self.avg_user_cost
    }
}

/// 索引优化建议器
#[derive(Debug, Default)]
pub struct IndexOptimizationAdvisor {
    /// 已收集的索引使用统计
    usage_stats: Vec<IndexUsageStats>,
    /// 已收集的缺失索引信息
    missing_indexes: Vec<MissingIndexInfo>,
    /// 已生成的建议
    suggestions: Vec<IndexSuggestion>,
}

impl IndexOptimizationAdvisor {
    /// 创建新的建议器
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加索引使用统计
    pub fn add_usage_stats(&mut self, stats: IndexUsageStats) {
        self.usage_stats.push(stats);
    }

    /// 添加缺失索引信息
    pub fn add_missing_index(&mut self, info: MissingIndexInfo) {
        self.missing_indexes.push(info);
    }

    /// 分析并生成建议
    pub fn analyze(&mut self) -> &[IndexSuggestion] {
        self.suggestions.clear();
        for info in &self.missing_indexes {
            self.suggestions.push(info.to_suggestion());
        }
        for stats in &self.usage_stats {
            if stats.is_unused() && !stats.index_name.is_empty() {
                self.suggestions.push(
                    IndexSuggestion::new(&stats.table_name, IndexType::NonClustered, &[])
                        .with_name(&stats.index_name)
                        .with_benefit(0.0)
                        .with_reason("index is unused, consider dropping"),
                );
            }
        }
        self.suggestions.sort_by(|a, b| {
            b.estimated_benefit
                .partial_cmp(&a.estimated_benefit)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        &self.suggestions
    }

    /// 获取所有建议
    #[must_use]
    pub fn suggestions(&self) -> &[IndexSuggestion] {
        &self.suggestions
    }

    /// 生成所有 CREATE INDEX DDL
    #[must_use]
    pub fn build_create_ddls(&self) -> Vec<String> {
        self.suggestions.iter().map(|s| s.to_create_ddl()).collect()
    }

    /// 生成所有 DROP INDEX DDL（仅未使用索引）
    #[must_use]
    pub fn build_drop_ddls(&self) -> Vec<String> {
        self.suggestions
            .iter()
            .filter(|s| s.estimated_benefit == 0.0)
            .map(|s| s.to_drop_ddl())
            .collect()
    }

    /// 统计建议数量
    #[must_use]
    pub fn suggestion_count(&self) -> usize {
        self.suggestions.len()
    }

    /// 统计使用统计数量
    #[must_use]
    pub fn usage_stats_count(&self) -> usize {
        self.usage_stats.len()
    }

    /// 统计缺失索引数量
    #[must_use]
    pub fn missing_index_count(&self) -> usize {
        self.missing_indexes.len()
    }

    /// 生成缺失索引查询 SQL
    #[must_use]
    pub fn missing_index_query_sql(&self) -> String {
        "SELECT \
         OBJECT_NAME(s.object_id) AS table_name, \
         s.equality_columns, s.inequality_columns, s.included_columns, \
         s.user_seeks, s.avg_user_impact \
         FROM sys.dm_db_missing_index_details d \
         JOIN sys.dm_db_missing_index_groups g ON d.index_handle = g.index_handle \
         JOIN sys.dm_db_missing_index_group_stats s ON g.index_group_handle = s.group_handle"
            .to_string()
    }

    /// 生成索引使用统计查询 SQL
    #[must_use]
    pub fn usage_stats_query_sql(&self) -> String {
        "SELECT \
         i.name AS index_name, OBJECT_NAME(i.object_id) AS table_name, \
         s.user_seeks, s.user_scans, s.user_lookups, s.user_updates \
         FROM sys.indexes i \
         JOIN sys.dm_db_index_usage_stats s ON i.object_id = s.object_id AND i.index_id = s.index_id"
            .to_string()
    }
}

impl fmt::Display for IndexOptimizationAdvisor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "IndexOptimizationAdvisor(usage={}, missing={}, suggestions={})",
            self.usage_stats_count(),
            self.missing_index_count(),
            self.suggestion_count()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_type_as_sql() {
        assert_eq!(IndexType::Clustered.as_sql(), "CLUSTERED");
        assert_eq!(IndexType::NonClustered.as_sql(), "NONCLUSTERED");
        assert_eq!(IndexType::UniqueClustered.as_sql(), "UNIQUE CLUSTERED");
    }

    #[test]
    fn test_index_type_is_clustered() {
        assert!(IndexType::Clustered.is_clustered());
        assert!(!IndexType::NonClustered.is_clustered());
    }

    #[test]
    fn test_index_type_is_unique() {
        assert!(IndexType::UniqueClustered.is_unique());
        assert!(!IndexType::Clustered.is_unique());
    }

    #[test]
    fn test_index_type_is_columnstore() {
        assert!(IndexType::Columnstore.is_columnstore());
        assert!(!IndexType::Clustered.is_columnstore());
    }

    #[test]
    fn test_index_suggestion_create_ddl() {
        let s = IndexSuggestion::new("users", IndexType::NonClustered, &["email"]);
        let ddl = s.to_create_ddl();
        assert!(ddl.contains("CREATE NONCLUSTERED INDEX"));
        assert!(ddl.contains("ON dbo.users (email)"));
    }

    #[test]
    fn test_index_suggestion_with_included() {
        let s = IndexSuggestion::new("users", IndexType::NonClustered, &["email"])
            .with_included(&["name", "created_at"]);
        let ddl = s.to_create_ddl();
        assert!(ddl.contains("INCLUDE (name, created_at)"));
    }

    #[test]
    fn test_index_suggestion_with_filter() {
        let s = IndexSuggestion::new("orders", IndexType::NonClustered, &["status"])
            .with_filter("status = 'active'");
        let ddl = s.to_create_ddl();
        assert!(ddl.contains("WHERE status = 'active'"));
    }

    #[test]
    fn test_index_suggestion_with_name() {
        let s = IndexSuggestion::new("users", IndexType::NonClustered, &["email"])
            .with_name("IX_users_email_custom");
        let ddl = s.to_create_ddl();
        assert!(ddl.contains("IX_users_email_custom"));
    }

    #[test]
    fn test_index_suggestion_with_schema() {
        let s = IndexSuggestion::new("users", IndexType::NonClustered, &["id"]).with_schema("app");
        let ddl = s.to_create_ddl();
        assert!(ddl.contains("ON app.users"));
    }

    #[test]
    fn test_index_suggestion_drop_ddl() {
        let s = IndexSuggestion::new("users", IndexType::NonClustered, &["email"])
            .with_name("IX_users_email");
        let ddl = s.to_drop_ddl();
        assert_eq!(ddl, "DROP INDEX IX_users_email ON users;");
    }

    #[test]
    fn test_index_suggestion_display() {
        let s = IndexSuggestion::new("users", IndexType::NonClustered, &["id"]);
        let str = format!("{}", s);
        assert!(str.contains("CREATE NONCLUSTERED INDEX"));
    }

    #[test]
    fn test_index_usage_stats_total_reads() {
        let stats = IndexUsageStats {
            user_seeks: 10,
            user_scans: 5,
            user_lookups: 3,
            ..IndexUsageStats::new("ix", "t")
        };
        assert_eq!(stats.total_reads(), 18);
    }

    #[test]
    fn test_index_usage_stats_read_write_ratio() {
        let stats = IndexUsageStats {
            user_seeks: 100,
            user_updates: 10,
            ..IndexUsageStats::new("ix", "t")
        };
        assert!((stats.read_write_ratio() - 10.0).abs() < 1e-10);
    }

    #[test]
    fn test_index_usage_stats_is_unused() {
        let stats = IndexUsageStats::new("ix", "t");
        assert!(stats.is_unused());
    }

    #[test]
    fn test_index_usage_stats_usage_score() {
        let stats = IndexUsageStats {
            user_seeks: 80,
            user_updates: 20,
            ..IndexUsageStats::new("ix", "t")
        };
        assert!((stats.usage_score() - 0.8).abs() < 1e-10);
    }

    #[test]
    fn test_missing_index_to_suggestion() {
        let info = MissingIndexInfo {
            equality_columns: vec!["email".to_string()],
            included_columns: vec!["name".to_string()],
            user_impact: 95.0,
            user_seeks: 100,
            avg_user_cost: 0.5,
            ..MissingIndexInfo::new("users")
        };
        let s = info.to_suggestion();
        assert_eq!(s.table, "users");
        assert!(s.key_columns.contains(&"email".to_string()));
        assert!(s.included_columns.contains(&"name".to_string()));
    }

    #[test]
    fn test_missing_index_composite_score() {
        let info = MissingIndexInfo {
            user_impact: 95.0,
            user_seeks: 100,
            avg_user_cost: 0.5,
            ..MissingIndexInfo::new("users")
        };
        let score = info.composite_score();
        assert!((score - 95.0 * 100.0 * 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_advisor_analyze_missing() {
        let mut advisor = IndexOptimizationAdvisor::new();
        advisor.add_missing_index(MissingIndexInfo {
            equality_columns: vec!["email".to_string()],
            user_impact: 90.0,
            user_seeks: 50,
            avg_user_cost: 0.3,
            ..MissingIndexInfo::new("users")
        });
        advisor.analyze();
        assert_eq!(advisor.suggestion_count(), 1);
    }

    #[test]
    fn test_advisor_analyze_unused() {
        let mut advisor = IndexOptimizationAdvisor::new();
        advisor.add_usage_stats(IndexUsageStats::new("ix_unused", "users"));
        advisor.analyze();
        assert!(advisor.suggestion_count() >= 1);
    }

    #[test]
    fn test_advisor_build_create_ddls() {
        let mut advisor = IndexOptimizationAdvisor::new();
        advisor.add_missing_index(MissingIndexInfo {
            equality_columns: vec!["id".to_string()],
            user_impact: 80.0,
            user_seeks: 10,
            avg_user_cost: 0.1,
            ..MissingIndexInfo::new("t")
        });
        advisor.analyze();
        let ddls = advisor.build_create_ddls();
        assert!(!ddls.is_empty());
        assert!(ddls[0].contains("CREATE NONCLUSTERED INDEX"));
    }

    #[test]
    fn test_advisor_missing_index_query_sql() {
        let advisor = IndexOptimizationAdvisor::new();
        let sql = advisor.missing_index_query_sql();
        assert!(sql.contains("dm_db_missing_index_details"));
    }

    #[test]
    fn test_advisor_usage_stats_query_sql() {
        let advisor = IndexOptimizationAdvisor::new();
        let sql = advisor.usage_stats_query_sql();
        assert!(sql.contains("dm_db_index_usage_stats"));
    }

    #[test]
    fn test_advisor_display() {
        let advisor = IndexOptimizationAdvisor::new();
        let s = format!("{}", advisor);
        assert!(s.contains("IndexOptimizationAdvisor"));
    }

    #[test]
    fn test_advisor_counts() {
        let mut advisor = IndexOptimizationAdvisor::new();
        advisor.add_usage_stats(IndexUsageStats::new("ix", "t"));
        advisor.add_missing_index(MissingIndexInfo::new("t"));
        assert_eq!(advisor.usage_stats_count(), 1);
        assert_eq!(advisor.missing_index_count(), 1);
    }
}
