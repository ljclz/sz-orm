//! L1/L2 多级缓存协同回填
//!
//! L1 miss → 查 L2 → L2 hit 回填 L1 → L2 miss 查 DB 回填 L1+L2
//! 通过依赖图精确失效 L1+L2。

use crate::dependency_graph::DependencyGraph;
use crate::l2_cache::{CacheKey, L2Cache};
use crate::value::Value;
use std::sync::Arc;

/// L1/L2 多级缓存协同回填
///
/// L1 miss → 查 L2 → L2 hit 回填 L1 → L2 miss 查 DB 回填 L1+L2
pub struct MultiLevelCache {
    l2: Option<Arc<L2Cache>>,
    dep_graph: DependencyGraph,
}

impl MultiLevelCache {
    /// 创建多级缓存协调器
    ///
    /// `l2` 为 None 时表示 L2 不可达，降级直查 DB。
    pub fn new(l2: Option<Arc<L2Cache>>, dep_graph: DependencyGraph) -> Self {
        Self { l2, dep_graph }
    }

    /// 多级查询 + 回填
    ///
    /// L2 hit → 返回；L2 miss → `db_loader` 查 DB → 回填 L2。
    /// L2 不可达时降级直查 DB，不阻塞业务。
    pub async fn get_or_load<F, Fut>(&self, key: &CacheKey, db_loader: F) -> Option<Value>
    where
        F: FnOnce(&CacheKey) -> Fut,
        Fut: std::future::Future<Output = Option<Value>>,
    {
        if let Some(ref l2) = self.l2 {
            if let Some(value) = l2.get(key) {
                return Some(value);
            }
        }

        let value = db_loader(key).await?;

        if let Some(ref l2) = self.l2 {
            l2.put(key, value.clone(), None);
        }

        Some(value)
    }

    /// 通过依赖图精确失效 L2
    ///
    /// 依赖图中无该表时降级为表级失效。
    pub fn invalidate_table(&self, table: &str) {
        let dependent_keys = self.dep_graph.dependent_keys(table);

        if dependent_keys.is_empty() {
            if let Some(ref l2) = self.l2 {
                l2.invalidate_table(table);
            }
            return;
        }

        if let Some(ref l2) = self.l2 {
            for key in &dependent_keys {
                l2.invalidate(key);
            }
            l2.invalidate_table(table);
        }
    }

    /// 记录缓存项依赖的表（自动从 SQL 解析或手动指定）
    pub fn add_dependency(&self, table: &str, cache_key: &CacheKey) {
        self.dep_graph.add_dependency(table, cache_key);
    }

    /// 从 SQL 自动解析依赖并记录
    pub fn add_dependency_from_sql(&self, sql: &str, cache_key: &CacheKey) {
        let tables = DependencyGraph::parse_dependencies(sql);
        for table in tables {
            self.dep_graph.add_dependency(&table, cache_key);
        }
    }

    /// 依赖图引用
    pub fn dependency_graph(&self) -> &DependencyGraph {
        &self.dep_graph
    }
}

/// 热点淘汰策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvictionStrategy {
    /// 传统 LRU 淘汰（默认，向后兼容）
    LRU,
    /// 热点优先保留：按访问频次降序，淘汰低频（冷门）key
    HotPriority,
}

/// 热点淘汰配置
#[derive(Debug, Clone)]
pub struct HotEvictionConfig {
    /// 热点阈值：访问频次 ≥ 此值视为热点
    pub hot_threshold: u64,
    /// 统计窗口大小（访问次数）
    pub stats_window: usize,
    /// 淘汰策略
    pub strategy: EvictionStrategy,
}

impl Default for HotEvictionConfig {
    fn default() -> Self {
        Self {
            hot_threshold: 10,
            stats_window: 1000,
            strategy: EvictionStrategy::LRU,
        }
    }
}

impl HotEvictionConfig {
    /// 创建默认 LRU 策略配置
    pub fn lru() -> Self {
        Self {
            strategy: EvictionStrategy::LRU,
            ..Default::default()
        }
    }

    /// 创建热点优先策略配置
    pub fn hot_priority(hot_threshold: u64) -> Self {
        Self {
            hot_threshold,
            strategy: EvictionStrategy::HotPriority,
            ..Default::default()
        }
    }
}

/// 热点优先淘汰器
///
/// 按访问频次降序排序，淘汰低频（冷门）key，保留高频（热点）key。
pub struct HotPriorityEviction {
    config: HotEvictionConfig,
}

impl HotPriorityEviction {
    /// 创建热点优先淘汰器
    pub fn new(config: HotEvictionConfig) -> Self {
        Self { config }
    }

    /// 从候选 key 中选择应淘汰的 key（低频优先淘汰）
    ///
    /// `access_counts`: key → 访问频次
    /// `candidates`: 待淘汰候选 key 列表
    /// `evict_count`: 需要淘汰的数量
    /// 返回应淘汰的 key 列表
    pub fn select_for_eviction(
        &self,
        access_counts: &HashMap<CacheKey, u64>,
        candidates: &[CacheKey],
        evict_count: usize,
    ) -> Vec<CacheKey> {
        if evict_count == 0 || candidates.is_empty() {
            return Vec::new();
        }

        let mut scored: Vec<(CacheKey, u64)> = candidates
            .iter()
            .map(|k| {
                let count = access_counts.get(k).copied().unwrap_or(0);
                (k.clone(), count)
            })
            .collect();

        scored.sort_by_key(|(_, count)| *count);

        scored
            .into_iter()
            .take(evict_count)
            .map(|(k, _)| k)
            .collect()
    }

    /// 判断 key 是否为热点
    pub fn is_hot(&self, access_count: u64) -> bool {
        access_count >= self.config.hot_threshold
    }

    /// 配置引用
    pub fn config(&self) -> &HotEvictionConfig {
        &self.config
    }
}

use std::collections::HashMap;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::l2_cache::CacheKeyKind;

    fn make_key(table: &str, id: &str) -> CacheKey {
        CacheKey {
            table: table.to_string(),
            kind: CacheKeyKind::ByQuery,
            identifier: id.to_string(),
        }
    }

    #[tokio::test]
    async fn test_multi_level_cache_l2_hit() {
        let l2 = Arc::new(L2Cache::new());
        let key = make_key("users", "q1");
        l2.put(&key, Value::I32(42), None);

        let dg = DependencyGraph::new(1000);
        let mlc = MultiLevelCache::new(Some(l2), dg);

        let result = mlc
            .get_or_load(&key, |_| async { panic!("不应调用 db_loader") })
            .await;
        assert_eq!(result, Some(Value::I32(42)));
    }

    #[tokio::test]
    async fn test_multi_level_cache_l2_miss_db_backfill() {
        let l2 = Arc::new(L2Cache::new());
        let key = make_key("users", "q1");

        let dg = DependencyGraph::new(1000);
        let mlc = MultiLevelCache::new(Some(l2.clone()), dg);

        let result = mlc
            .get_or_load(&key, |_| async { Some(Value::I32(99)) })
            .await;
        assert_eq!(result, Some(Value::I32(99)));

        assert_eq!(l2.get(&key), Some(Value::I32(99)), "L2 应被回填");
    }

    #[tokio::test]
    async fn test_multi_level_cache_l2_unreachable_degrade() {
        let dg = DependencyGraph::new(1000);
        let mlc = MultiLevelCache::new(None, dg);

        let key = make_key("users", "q1");
        let result = mlc
            .get_or_load(&key, |_| async { Some(Value::I32(77)) })
            .await;
        assert_eq!(result, Some(Value::I32(77)), "L2 不可达时应降级直查 DB");
    }

    #[tokio::test]
    async fn test_multi_level_cache_db_miss_returns_none() {
        let l2 = Arc::new(L2Cache::new());
        let dg = DependencyGraph::new(1000);
        let mlc = MultiLevelCache::new(Some(l2), dg);

        let key = make_key("users", "q1");
        let result = mlc.get_or_load(&key, |_| async { None }).await;
        assert!(result.is_none());
    }

    #[test]
    fn test_multi_level_cache_invalidate_via_dependency_graph() {
        let l2 = Arc::new(L2Cache::new());
        let key1 = make_key("orders", "q1");
        let key2 = make_key("items", "q2");

        l2.put(&key1, Value::I32(1), None);
        l2.put(&key2, Value::I32(2), None);

        let dg = DependencyGraph::new(1000);
        dg.add_dependency("orders", &key1);
        dg.add_dependency("items", &key2);

        let mlc = MultiLevelCache::new(Some(l2.clone()), dg);

        mlc.invalidate_table("orders");

        assert!(l2.get(&key1).is_none(), "key1 应被失效");
        assert!(l2.get(&key2).is_some(), "key2 不应被失效（不依赖 orders）");
    }

    #[test]
    fn test_multi_level_cache_invalidate_degrade_no_dependency() {
        let l2 = Arc::new(L2Cache::new());
        let key = make_key("unknown", "q1");
        l2.put(&key, Value::I32(1), None);

        let dg = DependencyGraph::new(1000);
        let mlc = MultiLevelCache::new(Some(l2.clone()), dg);

        mlc.invalidate_table("unknown");

        assert!(l2.get(&key).is_none(), "无依赖图记录时应降级为表级失效");
    }

    #[test]
    fn test_multi_level_cache_add_dependency_from_sql() {
        let l2 = Arc::new(L2Cache::new());
        let dg = DependencyGraph::new(1000);
        let mlc = MultiLevelCache::new(Some(l2), dg);

        let key = make_key("orders", "q1");
        mlc.add_dependency_from_sql(
            "SELECT * FROM orders o JOIN items i ON o.id = i.order_id",
            &key,
        );

        let keys_orders = mlc.dependency_graph().dependent_keys("orders");
        let keys_items = mlc.dependency_graph().dependent_keys("items");
        assert!(keys_orders.contains(&key));
        assert!(keys_items.contains(&key));
    }

    #[test]
    fn test_eviction_strategy_default() {
        let config = HotEvictionConfig::default();
        assert_eq!(config.strategy, EvictionStrategy::LRU);
        assert_eq!(config.hot_threshold, 10);
    }

    #[test]
    fn test_eviction_strategy_hot_priority() {
        let config = HotEvictionConfig::hot_priority(5);
        assert_eq!(config.strategy, EvictionStrategy::HotPriority);
        assert_eq!(config.hot_threshold, 5);
    }

    #[test]
    fn test_hot_priority_eviction_select() {
        let evictor = HotPriorityEviction::new(HotEvictionConfig::hot_priority(10));

        let k1 = make_key("t", "k1");
        let k2 = make_key("t", "k2");
        let k3 = make_key("t", "k3");

        let mut access_counts = HashMap::new();
        access_counts.insert(k1.clone(), 100);
        access_counts.insert(k2.clone(), 1);
        access_counts.insert(k3.clone(), 50);

        let candidates = vec![k1.clone(), k2.clone(), k3.clone()];
        let evicted = evictor.select_for_eviction(&access_counts, &candidates, 1);

        assert_eq!(evicted.len(), 1);
        assert_eq!(evicted[0], k2, "应淘汰访问频次最低的 k2");
    }

    #[test]
    fn test_hot_priority_eviction_preserve_hot() {
        let evictor = HotPriorityEviction::new(HotEvictionConfig::hot_priority(10));

        let mut access_counts = HashMap::new();
        let mut candidates = Vec::new();

        for i in 0..50 {
            let k = make_key("t", &format!("hot_{}", i));
            access_counts.insert(k.clone(), 100);
            candidates.push(k);
        }
        for i in 0..100 {
            let k = make_key("t", &format!("cold_{}", i));
            access_counts.insert(k.clone(), 1);
            candidates.push(k);
        }

        let evicted = evictor.select_for_eviction(&access_counts, &candidates, 100);

        assert_eq!(evicted.len(), 100);

        let hot_evicted = evicted
            .iter()
            .filter(|k| k.identifier.starts_with("hot_"))
            .count();
        let cold_evicted = evicted
            .iter()
            .filter(|k| k.identifier.starts_with("cold_"))
            .count();

        assert_eq!(hot_evicted, 0, "热点 key 不应被淘汰");
        assert_eq!(cold_evicted, 100, "冷门 key 应全部被淘汰");
    }

    #[test]
    fn test_hot_priority_is_hot() {
        let evictor = HotPriorityEviction::new(HotEvictionConfig::hot_priority(10));
        assert!(!evictor.is_hot(9));
        assert!(evictor.is_hot(10));
        assert!(evictor.is_hot(100));
    }

    #[test]
    fn test_hot_priority_eviction_empty_candidates() {
        let evictor = HotPriorityEviction::new(HotEvictionConfig::hot_priority(10));
        let access_counts = HashMap::new();
        let evicted = evictor.select_for_eviction(&access_counts, &[], 10);
        assert!(evicted.is_empty());
    }

    #[test]
    fn test_hot_priority_eviction_zero_count() {
        let evictor = HotPriorityEviction::new(HotEvictionConfig::hot_priority(10));
        let access_counts = HashMap::new();
        let k1 = make_key("t", "k1");
        let evicted = evictor.select_for_eviction(&access_counts, &[k1], 0);
        assert!(evicted.is_empty());
    }
}
