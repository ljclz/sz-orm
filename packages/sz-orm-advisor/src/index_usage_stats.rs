//! 索引使用统计器
//!
//! 提供 [`IndexUsageStats`] 跟踪索引的使用情况，识别未使用索引、
//! 热点索引、冗余索引等，为优化建议提供数据支撑。

use std::collections::HashMap;
use std::fmt;

/// 单个索引的使用统计
#[derive(Debug, Clone)]
pub struct IndexStats {
    /// 索引名
    pub index_name: String,
    /// 所属表
    pub table_name: String,
    /// 索引列
    pub columns: Vec<String>,
    /// 是否为唯一索引
    pub is_unique: bool,
    /// 是否为主键
    pub is_primary: bool,
    /// 读取次数（通过该索引访问的查询数）
    pub read_count: u64,
    /// 写入次数（因该索引存在导致的额外写入）
    pub write_count: u64,
    /// 最后使用时间戳（Unix 毫秒），0 表示从未使用
    pub last_used: u64,
    /// 索引大小（字节，估算）
    pub size_bytes: u64,
}

impl IndexStats {
    /// 创建新索引统计
    #[must_use]
    pub fn new(index_name: &str, table_name: &str, columns: Vec<String>) -> Self {
        Self {
            index_name: index_name.to_string(),
            table_name: table_name.to_string(),
            columns,
            is_unique: false,
            is_primary: false,
            read_count: 0,
            write_count: 0,
            last_used: 0,
            size_bytes: 0,
        }
    }

    /// 标记为唯一索引
    #[must_use]
    pub fn with_unique(mut self, is_unique: bool) -> Self {
        self.is_unique = is_unique;
        self
    }

    /// 标记为主键
    #[must_use]
    pub fn with_primary(mut self, is_primary: bool) -> Self {
        self.is_primary = is_primary;
        self
    }

    /// 设置索引大小
    #[must_use]
    pub fn with_size(mut self, size_bytes: u64) -> Self {
        self.size_bytes = size_bytes;
        self
    }

    /// 记算读写比
    #[must_use]
    pub fn read_write_ratio(&self) -> f64 {
        if self.write_count == 0 {
            return self.read_count as f64;
        }
        self.read_count as f64 / self.write_count as f64
    }

    /// 是否从未被使用
    #[must_use]
    pub fn is_unused(&self) -> bool {
        self.read_count == 0 && !self.is_primary
    }

    /// 是否为低使用率（读取次数低于阈值）
    #[must_use]
    pub fn is_low_usage(&self, threshold: u64) -> bool {
        self.read_count < threshold && !self.is_primary
    }

    /// 使用率评分（0.0~1.0）
    #[must_use]
    pub fn usage_score(&self, max_reads: u64) -> f64 {
        if max_reads == 0 {
            return 0.0;
        }
        (self.read_count as f64 / max_reads as f64).min(1.0)
    }

    /// 计算维护成本评分（0.0~1.0，越高越昂贵）
    #[must_use]
    pub fn maintenance_cost(&self) -> f64 {
        let base = self.write_count as f64;
        let col_factor = self.columns.len() as f64;
        let size_factor = (self.size_bytes as f64 / 1_048_576.0).min(10.0);
        (base * col_factor * (1.0 + size_factor)) / 1_000_000.0
    }
}

impl fmt::Display for IndexStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "IndexStats({}.{}, reads={}, writes={}, cols={})",
            self.table_name,
            self.index_name,
            self.read_count,
            self.write_count,
            self.columns.len()
        )
    }
}

/// 索引使用统计器
#[derive(Debug, Default)]
pub struct IndexUsageStats {
    /// 索引统计映射：key = "table.index"
    stats: HashMap<String, IndexStats>,
    /// 最大读取次数（用于评分归一化）
    max_reads: u64,
}

impl IndexUsageStats {
    /// 创建新的统计器
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 生成索引键
    fn make_key(table: &str, index: &str) -> String {
        format!("{}.{}", table, index)
    }

    /// 注册索引
    pub fn register_index(&mut self, stats: IndexStats) {
        let key = Self::make_key(&stats.table_name, &stats.index_name);
        self.max_reads = self.max_reads.max(stats.read_count);
        self.stats.insert(key, stats);
    }

    /// 记数记录索引读取
    pub fn record_read(&mut self, table: &str, index: &str, timestamp: u64) {
        let key = Self::make_key(table, index);
        if let Some(stats) = self.stats.get_mut(&key) {
            stats.read_count += 1;
            stats.last_used = stats.last_used.max(timestamp);
            self.max_reads = self.max_reads.max(stats.read_count);
        }
    }

    /// 批量记录读取
    pub fn record_reads(&mut self, table: &str, index: &str, count: u64, timestamp: u64) {
        let key = Self::make_key(table, index);
        if let Some(stats) = self.stats.get_mut(&key) {
            stats.read_count += count;
            stats.last_used = stats.last_used.max(timestamp);
            self.max_reads = self.max_reads.max(stats.read_count);
        }
    }

    /// 记数记录索引写入
    pub fn record_write(&mut self, table: &str, index: &str) {
        let key = Self::make_key(table, index);
        if let Some(stats) = self.stats.get_mut(&key) {
            stats.write_count += 1;
        }
    }

    /// 批量记录写入
    pub fn record_writes(&mut self, table: &str, index: &str, count: u64) {
        let key = Self::make_key(table, index);
        if let Some(stats) = self.stats.get_mut(&key) {
            stats.write_count += count;
        }
    }

    /// 获取索引统计
    #[must_use]
    pub fn get(&self, table: &str, index: &str) -> Option<&IndexStats> {
        let key = Self::make_key(table, index);
        self.stats.get(&key)
    }

    /// 获取所有索引统计
    #[must_use]
    pub fn all(&self) -> Vec<&IndexStats> {
        self.stats.values().collect()
    }

    /// 获取表的索引
    #[must_use]
    pub fn indexes_for_table(&self, table: &str) -> Vec<&IndexStats> {
        self.stats
            .values()
            .filter(|s| s.table_name == table)
            .collect()
    }

    /// 获取未使用索引
    #[must_use]
    pub fn unused_indexes(&self) -> Vec<&IndexStats> {
        self.stats.values().filter(|s| s.is_unused()).collect()
    }

    /// 获取低使用率索引
    #[must_use]
    pub fn low_usage_indexes(&self, threshold: u64) -> Vec<&IndexStats> {
        self.stats
            .values()
            .filter(|s| s.is_low_usage(threshold))
            .collect()
    }

    /// 获取热点索引（按读取次数排序）
    #[must_use]
    pub fn hot_indexes(&self, limit: usize) -> Vec<&IndexStats> {
        let mut indexes: Vec<&IndexStats> = self.stats.values().collect();
        indexes.sort_by_key(|i| std::cmp::Reverse(i.read_count));
        indexes.into_iter().take(limit).collect()
    }

    /// 检测冗余索引（列前缀重叠）
    #[must_use]
    pub fn redundant_indexes(&self) -> Vec<RedundantIndexPair> {
        let mut result = Vec::new();
        let mut by_table: HashMap<String, Vec<&IndexStats>> = HashMap::new();
        for stats in self.stats.values() {
            by_table
                .entry(stats.table_name.clone())
                .or_default()
                .push(stats);
        }
        for (table, indexes) in &by_table {
            for i in 0..indexes.len() {
                for j in (i + 1)..indexes.len() {
                    let a = indexes[i];
                    let b = indexes[j];
                    if a.is_primary || b.is_primary {
                        continue;
                    }
                    if Self::is_prefix_overlap(&a.columns, &b.columns) {
                        let (redundant, kept) = if a.columns.len() >= b.columns.len() {
                            (a, b)
                        } else {
                            (b, a)
                        };
                        result.push(RedundantIndexPair {
                            table: table.clone(),
                            redundant_index: redundant.index_name.clone(),
                            kept_index: kept.index_name.clone(),
                            reason: "column prefix overlap".to_string(),
                        });
                    }
                }
            }
        }
        result
    }

    /// 判断两个列列表是否前缀重叠
    fn is_prefix_overlap(a: &[String], b: &[String]) -> bool {
        if a.is_empty() || b.is_empty() {
            return false;
        }
        let min_len = a.len().min(b.len());
        (0..min_len).all(|i| a[i] == b[i])
    }

    /// 计算总索引大小
    #[must_use]
    pub fn total_size_bytes(&self) -> u64 {
        self.stats.values().map(|s| s.size_bytes).sum()
    }

    /// 计算总读取次数
    #[must_use]
    pub fn total_reads(&self) -> u64 {
        self.stats.values().map(|s| s.read_count).sum()
    }

    /// 计算总写入次数
    #[must_use]
    pub fn total_writes(&self) -> u64 {
        self.stats.values().map(|s| s.write_count).sum()
    }

    /// 索引数量
    #[must_use]
    pub fn index_count(&self) -> usize {
        self.stats.len()
    }

    /// 生成索引使用报告
    #[must_use]
    pub fn report(&self) -> IndexUsageReport {
        let unused = self.unused_indexes();
        let redundant = self.redundant_indexes();
        IndexUsageReport {
            total_indexes: self.index_count(),
            unused_count: unused.len(),
            redundant_count: redundant.len(),
            total_reads: self.total_reads(),
            total_writes: self.total_writes(),
            total_size_bytes: self.total_size_bytes(),
            unused_indexes: unused.iter().map(|s| s.index_name.clone()).collect(),
            redundant_pairs: redundant,
        }
    }
}

impl fmt::Display for IndexUsageStats {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "IndexUsageStats(indexes={}, reads={}, writes={})",
            self.index_count(),
            self.total_reads(),
            self.total_writes()
        )
    }
}

/// 冗余索引对
#[derive(Debug, Clone)]
pub struct RedundantIndexPair {
    /// 表名
    pub table: String,
    /// 冗余索引名（建议删除）
    pub redundant_index: String,
    /// 保留索引名
    pub kept_index: String,
    /// 原因
    pub reason: String,
}

/// 索引使用报告
#[derive(Debug, Clone)]
pub struct IndexUsageReport {
    /// 总索引数
    pub total_indexes: usize,
    /// 未使用索引数
    pub unused_count: usize,
    /// 冗余索引对数
    pub redundant_count: usize,
    /// 总读取次数
    pub total_reads: u64,
    /// 总写入次数
    pub total_writes: u64,
    /// 总索引大小
    pub total_size_bytes: u64,
    /// 未使用索引名列表
    pub unused_indexes: Vec<String>,
    /// 冗余索引对
    pub redundant_pairs: Vec<RedundantIndexPair>,
}

impl IndexUsageReport {
    /// 是否有优化建议
    #[must_use]
    pub fn has_suggestions(&self) -> bool {
        self.unused_count > 0 || self.redundant_count > 0
    }

    /// 估算可释放空间（未使用索引大小）
    #[must_use]
    pub fn estimated_savings_bytes(&self, stats: &IndexUsageStats) -> u64 {
        self.unused_indexes
            .iter()
            .filter_map(|name| {
                stats
                    .stats
                    .values()
                    .find(|s| s.index_name == *name)
                    .map(|s| s.size_bytes)
            })
            .sum()
    }
}

impl fmt::Display for IndexUsageReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "IndexUsageReport(total={}, unused={}, redundant={}, reads={}, writes={})",
            self.total_indexes,
            self.unused_count,
            self.redundant_count,
            self.total_reads,
            self.total_writes
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_stats(name: &str, table: &str, cols: Vec<String>) -> IndexStats {
        IndexStats::new(name, table, cols)
    }

    #[test]
    fn test_index_stats_new() {
        let s = make_stats("idx_a", "t", vec!["a".into()]);
        assert_eq!(s.index_name, "idx_a");
        assert_eq!(s.table_name, "t");
        assert_eq!(s.read_count, 0);
    }

    #[test]
    fn test_index_stats_with_unique() {
        let s = make_stats("idx_a", "t", vec!["a".into()]).with_unique(true);
        assert!(s.is_unique);
    }

    #[test]
    fn test_index_stats_with_primary() {
        let s = make_stats("pk", "t", vec!["id".into()]).with_primary(true);
        assert!(s.is_primary);
    }

    #[test]
    fn test_index_stats_with_size() {
        let s = make_stats("idx", "t", vec!["a".into()]).with_size(1024);
        assert_eq!(s.size_bytes, 1024);
    }

    #[test]
    fn test_read_write_ratio_no_writes() {
        let mut s = make_stats("idx", "t", vec!["a".into()]);
        s.read_count = 10;
        assert!((s.read_write_ratio() - 10.0).abs() < 1e-10);
    }

    #[test]
    fn test_read_write_ratio_with_writes() {
        let mut s = make_stats("idx", "t", vec!["a".into()]);
        s.read_count = 20;
        s.write_count = 4;
        assert!((s.read_write_ratio() - 5.0).abs() < 1e-10);
    }

    #[test]
    fn test_is_unused() {
        let s = make_stats("idx", "t", vec!["a".into()]);
        assert!(s.is_unused());
    }

    #[test]
    fn test_is_unused_primary_never() {
        let s = make_stats("pk", "t", vec!["id".into()]).with_primary(true);
        assert!(!s.is_unused());
    }

    #[test]
    fn test_is_low_usage() {
        let mut s = make_stats("idx", "t", vec!["a".into()]);
        s.read_count = 5;
        assert!(s.is_low_usage(10));
        assert!(!s.is_low_usage(3));
    }

    #[test]
    fn test_usage_score() {
        let mut s = make_stats("idx", "t", vec!["a".into()]);
        s.read_count = 50;
        assert!((s.usage_score(100) - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_usage_score_zero_max() {
        let s = make_stats("idx", "t", vec!["a".into()]);
        assert!((s.usage_score(0) - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_maintenance_cost() {
        let mut s = make_stats("idx", "t", vec!["a".into(), "b".into()]);
        s.write_count = 100;
        let cost = s.maintenance_cost();
        assert!(cost > 0.0);
    }

    #[test]
    fn test_index_stats_display() {
        let s = make_stats("idx", "t", vec!["a".into()]);
        let str = format!("{}", s);
        assert!(str.contains("IndexStats"));
    }

    #[test]
    fn test_index_usage_stats_new() {
        let stats = IndexUsageStats::new();
        assert_eq!(stats.index_count(), 0);
    }

    #[test]
    fn test_register_index() {
        let mut stats = IndexUsageStats::new();
        stats.register_index(make_stats("idx_a", "t", vec!["a".into()]));
        assert_eq!(stats.index_count(), 1);
    }

    #[test]
    fn test_record_read() {
        let mut stats = IndexUsageStats::new();
        stats.register_index(make_stats("idx_a", "t", vec!["a".into()]));
        stats.record_read("t", "idx_a", 1000);
        stats.record_read("t", "idx_a", 2000);
        let s = stats.get("t", "idx_a").unwrap();
        assert_eq!(s.read_count, 2);
        assert_eq!(s.last_used, 2000);
    }

    #[test]
    fn test_record_reads_batch() {
        let mut stats = IndexUsageStats::new();
        stats.register_index(make_stats("idx_a", "t", vec!["a".into()]));
        stats.record_reads("t", "idx_a", 10, 1000);
        let s = stats.get("t", "idx_a").unwrap();
        assert_eq!(s.read_count, 10);
    }

    #[test]
    fn test_record_write() {
        let mut stats = IndexUsageStats::new();
        stats.register_index(make_stats("idx_a", "t", vec!["a".into()]));
        stats.record_write("t", "idx_a");
        stats.record_write("t", "idx_a");
        let s = stats.get("t", "idx_a").unwrap();
        assert_eq!(s.write_count, 2);
    }

    #[test]
    fn test_record_writes_batch() {
        let mut stats = IndexUsageStats::new();
        stats.register_index(make_stats("idx_a", "t", vec!["a".into()]));
        stats.record_writes("t", "idx_a", 5);
        let s = stats.get("t", "idx_a").unwrap();
        assert_eq!(s.write_count, 5);
    }

    #[test]
    fn test_get_nonexistent() {
        let stats = IndexUsageStats::new();
        assert!(stats.get("t", "idx").is_none());
    }

    #[test]
    fn test_indexes_for_table() {
        let mut stats = IndexUsageStats::new();
        stats.register_index(make_stats("idx_a", "t1", vec!["a".into()]));
        stats.register_index(make_stats("idx_b", "t1", vec!["b".into()]));
        stats.register_index(make_stats("idx_c", "t2", vec!["c".into()]));
        assert_eq!(stats.indexes_for_table("t1").len(), 2);
        assert_eq!(stats.indexes_for_table("t2").len(), 1);
    }

    #[test]
    fn test_unused_indexes() {
        let mut stats = IndexUsageStats::new();
        stats.register_index(make_stats("idx_a", "t", vec!["a".into()]));
        stats.register_index(make_stats("pk", "t", vec!["id".into()]).with_primary(true));
        let unused = stats.unused_indexes();
        assert_eq!(unused.len(), 1);
        assert_eq!(unused[0].index_name, "idx_a");
    }

    #[test]
    fn test_low_usage_indexes() {
        let mut stats = IndexUsageStats::new();
        stats.register_index(make_stats("idx_a", "t", vec!["a".into()]));
        stats.register_index(make_stats("idx_b", "t", vec!["b".into()]));
        stats.record_reads("t", "idx_b", 100, 1000);
        let low = stats.low_usage_indexes(10);
        assert_eq!(low.len(), 1);
        assert_eq!(low[0].index_name, "idx_a");
    }

    #[test]
    fn test_hot_indexes() {
        let mut stats = IndexUsageStats::new();
        stats.register_index(make_stats("idx_a", "t", vec!["a".into()]));
        stats.register_index(make_stats("idx_b", "t", vec!["b".into()]));
        stats.record_reads("t", "idx_a", 10, 1000);
        stats.record_reads("t", "idx_b", 50, 1000);
        let hot = stats.hot_indexes(1);
        assert_eq!(hot[0].index_name, "idx_b");
    }

    #[test]
    fn test_redundant_indexes() {
        let mut stats = IndexUsageStats::new();
        stats.register_index(make_stats("idx_ab", "t", vec!["a".into(), "b".into()]));
        stats.register_index(make_stats("idx_a", "t", vec!["a".into()]));
        let redundant = stats.redundant_indexes();
        assert_eq!(redundant.len(), 1);
        assert_eq!(redundant[0].redundant_index, "idx_ab");
    }

    #[test]
    fn test_redundant_indexes_with_primary() {
        let mut stats = IndexUsageStats::new();
        stats.register_index(make_stats("pk", "t", vec!["id".into()]).with_primary(true));
        stats.register_index(make_stats("idx_id", "t", vec!["id".into()]));
        let redundant = stats.redundant_indexes();
        assert_eq!(redundant.len(), 0);
    }

    #[test]
    fn test_no_redundant_indexes_disjoint() {
        let mut stats = IndexUsageStats::new();
        stats.register_index(make_stats("idx_a", "t", vec!["a".into()]));
        stats.register_index(make_stats("idx_b", "t", vec!["b".into()]));
        let redundant = stats.redundant_indexes();
        assert_eq!(redundant.len(), 0);
    }

    #[test]
    fn test_total_size_bytes() {
        let mut stats = IndexUsageStats::new();
        stats.register_index(make_stats("idx_a", "t", vec!["a".into()]).with_size(100));
        stats.register_index(make_stats("idx_b", "t", vec!["b".into()]).with_size(200));
        assert_eq!(stats.total_size_bytes(), 300);
    }

    #[test]
    fn test_total_reads() {
        let mut stats = IndexUsageStats::new();
        stats.register_index(make_stats("idx_a", "t", vec!["a".into()]));
        stats.register_index(make_stats("idx_b", "t", vec!["b".into()]));
        stats.record_reads("t", "idx_a", 10, 1000);
        stats.record_reads("t", "idx_b", 20, 1000);
        assert_eq!(stats.total_reads(), 30);
    }

    #[test]
    fn test_total_writes() {
        let mut stats = IndexUsageStats::new();
        stats.register_index(make_stats("idx_a", "t", vec!["a".into()]));
        stats.register_index(make_stats("idx_b", "t", vec!["b".into()]));
        stats.record_writes("t", "idx_a", 5);
        stats.record_writes("t", "idx_b", 3);
        assert_eq!(stats.total_writes(), 8);
    }

    #[test]
    fn test_report() {
        let mut stats = IndexUsageStats::new();
        stats.register_index(make_stats("idx_a", "t", vec!["a".into()]));
        stats.register_index(make_stats("idx_b", "t", vec!["b".into()]));
        stats.record_reads("t", "idx_a", 10, 1000);
        let report = stats.report();
        assert_eq!(report.total_indexes, 2);
        assert_eq!(report.unused_count, 1);
    }

    #[test]
    fn test_report_has_suggestions() {
        let mut stats = IndexUsageStats::new();
        stats.register_index(make_stats("idx_a", "t", vec!["a".into()]));
        let report = stats.report();
        assert!(report.has_suggestions());
    }

    #[test]
    fn test_report_no_suggestions() {
        let mut stats = IndexUsageStats::new();
        stats.register_index(make_stats("pk", "t", vec!["id".into()]).with_primary(true));
        let report = stats.report();
        assert!(!report.has_suggestions());
    }

    #[test]
    fn test_estimated_savings() {
        let mut stats = IndexUsageStats::new();
        stats.register_index(make_stats("idx_a", "t", vec!["a".into()]).with_size(500));
        let report = stats.report();
        assert_eq!(report.estimated_savings_bytes(&stats), 500);
    }

    #[test]
    fn test_index_usage_stats_display() {
        let stats = IndexUsageStats::new();
        let s = format!("{}", stats);
        assert!(s.contains("IndexUsageStats"));
    }

    #[test]
    fn test_report_display() {
        let stats = IndexUsageStats::new();
        let report = stats.report();
        let s = format!("{}", report);
        assert!(s.contains("IndexUsageReport"));
    }

    #[test]
    fn test_all_indexes() {
        let mut stats = IndexUsageStats::new();
        stats.register_index(make_stats("idx_a", "t", vec!["a".into()]));
        stats.register_index(make_stats("idx_b", "t", vec!["b".into()]));
        assert_eq!(stats.all().len(), 2);
    }

    #[test]
    fn test_is_prefix_overlap_same() {
        let a = vec!["a".to_string(), "b".to_string()];
        let b = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert!(IndexUsageStats::is_prefix_overlap(&a, &b));
    }

    #[test]
    fn test_is_prefix_overlap_disjoint() {
        let a = vec!["a".to_string()];
        let b = vec!["b".to_string()];
        assert!(!IndexUsageStats::is_prefix_overlap(&a, &b));
    }

    #[test]
    fn test_is_prefix_overlap_empty() {
        let a: Vec<String> = vec![];
        let b = vec!["a".to_string()];
        assert!(!IndexUsageStats::is_prefix_overlap(&a, &b));
    }
}
