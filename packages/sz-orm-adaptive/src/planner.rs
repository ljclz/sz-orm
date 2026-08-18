//! 自适应查询计划器
//!
//! 根据查询统计与表元数据生成最优执行计划。

use std::collections::HashMap;

use crate::executor::{ExecutionPath, IndexSelectionStrategy, JoinOrderStrategy};
use crate::stats::QueryStats;

/// 查询计划
#[derive(Debug, Clone)]
pub struct QueryPlan {
    /// 查询键
    pub query_key: String,
    /// 执行路径
    pub execution_path: ExecutionPath,
    /// 索引选择策略
    pub index_strategy: IndexSelectionStrategy,
    /// 连接顺序策略
    pub join_order: JoinOrderStrategy,
    /// 建议的批大小
    pub batch_size: u64,
    /// 预计耗时（毫秒）
    pub estimated_cost_ms: u64,
    /// 计划原因
    pub reason: String,
}

/// 表元数据
#[derive(Debug, Clone)]
pub struct TableMetadata {
    /// 表名
    pub table_name: String,
    /// 估算行数
    pub estimated_rows: u64,
    /// 可用索引
    pub available_indexes: Vec<String>,
    /// 平均行大小（字节）
    pub avg_row_size: u64,
}

impl TableMetadata {
    /// 创建表元数据
    pub fn new(table_name: impl Into<String>, estimated_rows: u64) -> Self {
        Self {
            table_name: table_name.into(),
            estimated_rows,
            available_indexes: Vec::new(),
            avg_row_size: 100,
        }
    }

    /// 添加索引
    pub fn with_index(mut self, index: impl Into<String>) -> Self {
        self.available_indexes.push(index.into());
        self
    }

    /// 设置平均行大小
    pub fn with_row_size(mut self, size: u64) -> Self {
        self.avg_row_size = size;
        self
    }

    /// 是否有索引
    pub fn has_indexes(&self) -> bool {
        !self.available_indexes.is_empty()
    }

    /// 估算表大小（字节）
    pub fn estimated_size_bytes(&self) -> u64 {
        self.estimated_rows * self.avg_row_size
    }
}

/// 自适应查询计划器配置
#[derive(Debug, Clone)]
pub struct PlannerConfig {
    /// 大结果集阈值（行数）
    pub large_result_threshold: u64,
    /// 建议分页的批大小
    pub pagination_batch_size: u64,
    /// 小表阈值（行数 < 此值视为小表）
    pub small_table_threshold: u64,
    /// 是否偏好索引扫描
    pub prefer_index_scan: bool,
}

impl Default for PlannerConfig {
    fn default() -> Self {
        Self {
            large_result_threshold: 10_000,
            pagination_batch_size: 1000,
            small_table_threshold: 1000,
            prefer_index_scan: true,
        }
    }
}

/// 自适应查询计划器
///
/// 根据统计与元数据生成最优执行计划。
pub struct AdaptiveQueryPlanner {
    config: PlannerConfig,
    table_metadata: HashMap<String, TableMetadata>,
}

impl AdaptiveQueryPlanner {
    /// 创建计划器
    pub fn new(config: PlannerConfig) -> Self {
        Self {
            config,
            table_metadata: HashMap::new(),
        }
    }

    /// 使用默认配置创建
    pub fn with_defaults() -> Self {
        Self::new(PlannerConfig::default())
    }

    /// 注册表元数据
    pub fn register_table(&mut self, metadata: TableMetadata) {
        self.table_metadata
            .insert(metadata.table_name.clone(), metadata);
    }

    /// 获取表元数据
    pub fn table_metadata(&self, table_name: &str) -> Option<&TableMetadata> {
        self.table_metadata.get(table_name)
    }

    /// 为查询生成计划
    pub fn plan(&self, query_key: &str, stats: &QueryStats) -> QueryPlan {
        let avg_rows = stats.avg_rows() as u64;
        let avg_time_ms = stats.avg_time_ms() as u64;

        let execution_path = self.decide_execution_path(avg_rows);
        let index_strategy = self.select_index_strategy(avg_rows);
        let join_order = self.select_join_order(avg_rows);
        let batch_size = self.decide_batch_size(avg_rows, execution_path);
        let estimated_cost = self.estimate_cost(avg_rows, avg_time_ms);
        let reason = self.plan_reason(avg_rows, execution_path);

        QueryPlan {
            query_key: query_key.to_string(),
            execution_path,
            index_strategy,
            join_order,
            batch_size,
            estimated_cost_ms: estimated_cost,
            reason,
        }
    }

    fn decide_execution_path(&self, avg_rows: u64) -> ExecutionPath {
        if avg_rows > self.config.large_result_threshold {
            ExecutionPath::Paginated
        } else {
            ExecutionPath::Normal
        }
    }

    fn select_index_strategy(&self, avg_rows: u64) -> IndexSelectionStrategy {
        if avg_rows < self.config.small_table_threshold {
            IndexSelectionStrategy::FullScan
        } else if self.config.prefer_index_scan {
            IndexSelectionStrategy::IndexScan("auto_selected".to_string())
        } else {
            IndexSelectionStrategy::FullScan
        }
    }

    fn select_join_order(&self, avg_rows: u64) -> JoinOrderStrategy {
        if avg_rows > self.config.large_result_threshold {
            JoinOrderStrategy::Bushy
        } else if avg_rows > self.config.small_table_threshold {
            JoinOrderStrategy::RightDeep
        } else {
            JoinOrderStrategy::LeftDeep
        }
    }

    fn decide_batch_size(&self, avg_rows: u64, path: ExecutionPath) -> u64 {
        match path {
            ExecutionPath::Paginated => self.config.pagination_batch_size,
            ExecutionPath::Cached => avg_rows.min(self.config.pagination_batch_size),
            ExecutionPath::Normal => avg_rows.max(1),
        }
    }

    fn estimate_cost(&self, avg_rows: u64, avg_time_ms: u64) -> u64 {
        if avg_time_ms > 0 {
            avg_time_ms
        } else {
            // 估算：行数 * 0.01ms
            (avg_rows / 100).max(1)
        }
    }

    fn plan_reason(&self, avg_rows: u64, path: ExecutionPath) -> String {
        match path {
            ExecutionPath::Paginated => {
                format!(
                    "avg rows {} > threshold {}, suggest pagination",
                    avg_rows, self.config.large_result_threshold
                )
            }
            ExecutionPath::Cached => "hot query, suggest cache".to_string(),
            ExecutionPath::Normal => "normal execution".to_string(),
        }
    }

    /// 配置引用
    pub fn config(&self) -> &PlannerConfig {
        &self.config
    }

    /// 已注册表数
    pub fn table_count(&self) -> usize {
        self.table_metadata.len()
    }
}

impl Default for AdaptiveQueryPlanner {
    fn default() -> Self {
        Self::with_defaults()
    }
}

/// 计划缓存项
#[derive(Debug, Clone)]
pub struct CachedPlan {
    /// 查询计划
    pub plan: QueryPlan,
    /// 创建时间戳（毫秒）
    pub created_at_ms: u64,
    /// 命中次数
    pub hit_count: u64,
}

/// 执行计划缓存
///
/// 缓存查询计划避免重复规划，支持 TTL 过期。
pub struct ExecutionPlanCache {
    cache: HashMap<String, CachedPlan>,
    ttl_ms: u64,
    current_time_ms: u64,
}

impl ExecutionPlanCache {
    /// 创建计划缓存
    pub fn new(ttl_ms: u64) -> Self {
        Self {
            cache: HashMap::new(),
            ttl_ms,
            current_time_ms: 0,
        }
    }

    /// 推进时间
    pub fn advance_time(&mut self, ms: u64) {
        self.current_time_ms += ms;
    }

    /// 设置当前时间
    pub fn set_time(&mut self, ms: u64) {
        self.current_time_ms = ms;
    }

    /// 缓存计划
    pub fn put(&mut self, plan: QueryPlan) {
        let key = plan.query_key.clone();
        self.cache.insert(
            key,
            CachedPlan {
                plan,
                created_at_ms: self.current_time_ms,
                hit_count: 0,
            },
        );
    }

    /// 获取计划
    pub fn get(&mut self, query_key: &str) -> Option<&QueryPlan> {
        let entry = self.cache.get_mut(query_key)?;
        if self.current_time_ms > entry.created_at_ms + self.ttl_ms {
            return None;
        }
        entry.hit_count += 1;
        Some(&entry.plan)
    }

    /// 移除计划
    pub fn remove(&mut self, query_key: &str) -> bool {
        self.cache.remove(query_key).is_some()
    }

    /// 清空缓存
    pub fn clear(&mut self) {
        self.cache.clear();
    }

    /// 缓存大小
    pub fn len(&self) -> usize {
        self.cache.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// TTL（毫秒）
    pub fn ttl_ms(&self) -> u64 {
        self.ttl_ms
    }

    /// 清理过期项
    pub fn evict_expired(&mut self) -> usize {
        let expired: Vec<String> = self
            .cache
            .iter()
            .filter(|(_, entry)| self.current_time_ms > entry.created_at_ms + self.ttl_ms)
            .map(|(k, _)| k.clone())
            .collect();
        let count = expired.len();
        for key in expired {
            self.cache.remove(&key);
        }
        count
    }

    /// 命中次数
    pub fn hit_count(&self, query_key: &str) -> Option<u64> {
        self.cache.get(query_key).map(|e| e.hit_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats_with(rows: u64, time_ms: u64) -> QueryStats {
        let s = QueryStats::new();
        s.record(rows, time_ms * 1000);
        s
    }

    // --- TableMetadata tests ---

    #[test]
    fn table_metadata_new() {
        let m = TableMetadata::new("users", 1000);
        assert_eq!(m.table_name, "users");
        assert_eq!(m.estimated_rows, 1000);
        assert!(!m.has_indexes());
    }

    #[test]
    fn table_metadata_with_index() {
        let m = TableMetadata::new("users", 1000)
            .with_index("idx_email")
            .with_index("idx_name");
        assert_eq!(m.available_indexes.len(), 2);
        assert!(m.has_indexes());
    }

    #[test]
    fn table_metadata_with_row_size() {
        let m = TableMetadata::new("users", 1000).with_row_size(200);
        assert_eq!(m.avg_row_size, 200);
    }

    #[test]
    fn table_metadata_estimated_size() {
        let m = TableMetadata::new("users", 1000).with_row_size(200);
        assert_eq!(m.estimated_size_bytes(), 200_000);
    }

    #[test]
    fn table_metadata_no_indexes() {
        let m = TableMetadata::new("users", 1000);
        assert!(!m.has_indexes());
    }

    // --- PlannerConfig tests ---

    #[test]
    fn config_default() {
        let c = PlannerConfig::default();
        assert_eq!(c.large_result_threshold, 10_000);
        assert_eq!(c.pagination_batch_size, 1000);
        assert_eq!(c.small_table_threshold, 1000);
        assert!(c.prefer_index_scan);
    }

    // --- AdaptiveQueryPlanner tests ---

    #[test]
    fn planner_empty() {
        let p = AdaptiveQueryPlanner::with_defaults();
        assert_eq!(p.table_count(), 0);
    }

    #[test]
    fn planner_register_table() {
        let mut p = AdaptiveQueryPlanner::with_defaults();
        p.register_table(TableMetadata::new("users", 1000));
        assert_eq!(p.table_count(), 1);
        assert!(p.table_metadata("users").is_some());
    }

    #[test]
    fn planner_normal_execution() {
        let p = AdaptiveQueryPlanner::with_defaults();
        let stats = stats_with(100, 10);
        let plan = p.plan("q1", &stats);
        assert_eq!(plan.execution_path, ExecutionPath::Normal);
    }

    #[test]
    fn planner_paginated_execution() {
        let p = AdaptiveQueryPlanner::with_defaults();
        let stats = stats_with(20_000, 10);
        let plan = p.plan("q1", &stats);
        assert_eq!(plan.execution_path, ExecutionPath::Paginated);
    }

    #[test]
    fn planner_small_table_full_scan() {
        let p = AdaptiveQueryPlanner::with_defaults();
        let stats = stats_with(100, 10);
        let plan = p.plan("q1", &stats);
        assert_eq!(plan.index_strategy, IndexSelectionStrategy::FullScan);
    }

    #[test]
    fn planner_large_table_index_scan() {
        let p = AdaptiveQueryPlanner::with_defaults();
        let stats = stats_with(5000, 10);
        let plan = p.plan("q1", &stats);
        assert!(plan.index_strategy.is_index_used());
    }

    #[test]
    fn planner_join_order_small() {
        let p = AdaptiveQueryPlanner::with_defaults();
        let stats = stats_with(100, 10);
        let plan = p.plan("q1", &stats);
        assert_eq!(plan.join_order, JoinOrderStrategy::LeftDeep);
    }

    #[test]
    fn planner_join_order_large() {
        let p = AdaptiveQueryPlanner::with_defaults();
        let stats = stats_with(20_000, 10);
        let plan = p.plan("q1", &stats);
        assert_eq!(plan.join_order, JoinOrderStrategy::Bushy);
    }

    #[test]
    fn planner_batch_size_paginated() {
        let p = AdaptiveQueryPlanner::with_defaults();
        let stats = stats_with(20_000, 10);
        let plan = p.plan("q1", &stats);
        assert_eq!(plan.batch_size, 1000);
    }

    #[test]
    fn planner_estimated_cost_from_stats() {
        let p = AdaptiveQueryPlanner::with_defaults();
        let stats = stats_with(100, 50);
        let plan = p.plan("q1", &stats);
        assert_eq!(plan.estimated_cost_ms, 50);
    }

    #[test]
    fn planner_estimated_cost_from_rows() {
        let p = AdaptiveQueryPlanner::with_defaults();
        let stats = QueryStats::new(); // 无统计
        let plan = p.plan("q1", &stats);
        assert!(plan.estimated_cost_ms >= 1);
    }

    #[test]
    fn planner_reason_populated() {
        let p = AdaptiveQueryPlanner::with_defaults();
        let stats = stats_with(100, 10);
        let plan = p.plan("q1", &stats);
        assert!(!plan.reason.is_empty());
    }

    #[test]
    fn planner_default() {
        let p = AdaptiveQueryPlanner::default();
        assert_eq!(p.table_count(), 0);
    }

    #[test]
    fn planner_config_ref() {
        let p = AdaptiveQueryPlanner::with_defaults();
        assert_eq!(p.config().large_result_threshold, 10_000);
    }

    // --- ExecutionPlanCache tests ---

    #[test]
    fn cache_empty() {
        let c = ExecutionPlanCache::new(1000);
        assert!(c.is_empty());
        assert_eq!(c.len(), 0);
    }

    #[test]
    fn cache_put_and_get() {
        let mut c = ExecutionPlanCache::new(1000);
        let plan = QueryPlan {
            query_key: "q1".to_string(),
            execution_path: ExecutionPath::Normal,
            index_strategy: IndexSelectionStrategy::FullScan,
            join_order: JoinOrderStrategy::LeftDeep,
            batch_size: 100,
            estimated_cost_ms: 10,
            reason: "test".to_string(),
        };
        c.put(plan);
        assert_eq!(c.len(), 1);
        assert!(c.get("q1").is_some());
    }

    #[test]
    fn cache_miss() {
        let mut c = ExecutionPlanCache::new(1000);
        assert!(c.get("nonexistent").is_none());
    }

    #[test]
    fn cache_ttl_expiry() {
        let mut c = ExecutionPlanCache::new(1000);
        let plan = QueryPlan {
            query_key: "q1".to_string(),
            execution_path: ExecutionPath::Normal,
            index_strategy: IndexSelectionStrategy::FullScan,
            join_order: JoinOrderStrategy::LeftDeep,
            batch_size: 100,
            estimated_cost_ms: 10,
            reason: "test".to_string(),
        };
        c.put(plan);
        c.advance_time(1001);
        assert!(c.get("q1").is_none());
    }

    #[test]
    fn cache_remove() {
        let mut c = ExecutionPlanCache::new(1000);
        let plan = QueryPlan {
            query_key: "q1".to_string(),
            execution_path: ExecutionPath::Normal,
            index_strategy: IndexSelectionStrategy::FullScan,
            join_order: JoinOrderStrategy::LeftDeep,
            batch_size: 100,
            estimated_cost_ms: 10,
            reason: "test".to_string(),
        };
        c.put(plan);
        assert!(c.remove("q1"));
        assert!(c.is_empty());
    }

    #[test]
    fn cache_clear() {
        let mut c = ExecutionPlanCache::new(1000);
        let plan = QueryPlan {
            query_key: "q1".to_string(),
            execution_path: ExecutionPath::Normal,
            index_strategy: IndexSelectionStrategy::FullScan,
            join_order: JoinOrderStrategy::LeftDeep,
            batch_size: 100,
            estimated_cost_ms: 10,
            reason: "test".to_string(),
        };
        c.put(plan);
        c.clear();
        assert!(c.is_empty());
    }

    #[test]
    fn cache_hit_count() {
        let mut c = ExecutionPlanCache::new(1000);
        let plan = QueryPlan {
            query_key: "q1".to_string(),
            execution_path: ExecutionPath::Normal,
            index_strategy: IndexSelectionStrategy::FullScan,
            join_order: JoinOrderStrategy::LeftDeep,
            batch_size: 100,
            estimated_cost_ms: 10,
            reason: "test".to_string(),
        };
        c.put(plan);
        c.get("q1");
        c.get("q1");
        assert_eq!(c.hit_count("q1"), Some(2));
    }

    #[test]
    fn cache_evict_expired() {
        let mut c = ExecutionPlanCache::new(1000);
        c.put(QueryPlan {
            query_key: "q1".to_string(),
            execution_path: ExecutionPath::Normal,
            index_strategy: IndexSelectionStrategy::FullScan,
            join_order: JoinOrderStrategy::LeftDeep,
            batch_size: 100,
            estimated_cost_ms: 10,
            reason: "test".to_string(),
        });
        c.advance_time(1001);
        let evicted = c.evict_expired();
        assert_eq!(evicted, 1);
        assert!(c.is_empty());
    }

    #[test]
    fn cache_ttl_getter() {
        let c = ExecutionPlanCache::new(5000);
        assert_eq!(c.ttl_ms(), 5000);
    }

    #[test]
    fn cache_set_time() {
        let mut c = ExecutionPlanCache::new(1000);
        c.set_time(500);
        c.put(QueryPlan {
            query_key: "q1".to_string(),
            execution_path: ExecutionPath::Normal,
            index_strategy: IndexSelectionStrategy::FullScan,
            join_order: JoinOrderStrategy::LeftDeep,
            batch_size: 100,
            estimated_cost_ms: 10,
            reason: "test".to_string(),
        });
        c.set_time(1501);
        assert!(c.get("q1").is_none());
    }
}
