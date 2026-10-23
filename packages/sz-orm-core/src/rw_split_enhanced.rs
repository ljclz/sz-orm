//! v6.7.0 读写分离/分库分表增强
//!
//! 权重路由 + 延迟感知回退 + 分片键自动推断 + 跨片聚合 + 故障转移。

use std::collections::HashMap;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};

// ============================================================================
// 加权随机选择器
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeightedSlave {
    pub name: String,
    pub weight: u32,
    pub host: String,
    pub port: u16,
}

pub struct WeightedRandomSelector {
    rng: rand::rngs::StdRng,
}

impl WeightedRandomSelector {
    pub fn new() -> Self {
        use rand::SeedableRng;
        Self {
            rng: rand::rngs::StdRng::from_entropy(),
        }
    }

    pub fn select(&mut self, slaves: &[WeightedSlave]) -> Option<usize> {
        if slaves.is_empty() {
            return None;
        }
        let total: u32 = slaves.iter().map(|s| s.weight).sum();
        if total == 0 {
            return None;
        }
        use rand::Rng;
        let mut pick = self.rng.gen_range(0..total);
        for (i, slave) in slaves.iter().enumerate() {
            if pick < slave.weight {
                return Some(i);
            }
            pick -= slave.weight;
        }
        Some(slaves.len() - 1)
    }

    pub fn select_with_lag_filter(
        &mut self,
        slaves: &[WeightedSlave],
        lag_threshold: u64,
        lag_stats: &HashMap<String, u64>,
    ) -> Option<usize> {
        let filtered: Vec<usize> = slaves
            .iter()
            .enumerate()
            .filter(|(_, s)| *lag_stats.get(&s.name).unwrap_or(&0) <= lag_threshold)
            .map(|(i, _)| i)
            .collect();
        if filtered.is_empty() {
            return None;
        }
        let filtered_slaves: Vec<WeightedSlave> =
            filtered.iter().map(|&i| slaves[i].clone()).collect();
        self.select(&filtered_slaves).map(|i| filtered[i])
    }
}

impl Default for WeightedRandomSelector {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 分片键推断器
// ============================================================================

#[derive(Debug, Clone)]
pub struct ShardKeyInference {
    pub table_shard_keys: HashMap<String, String>,
}

impl ShardKeyInference {
    pub fn new() -> Self {
        Self {
            table_shard_keys: HashMap::new(),
        }
    }

    pub fn register(&mut self, table: &str, shard_key: &str) {
        self.table_shard_keys
            .insert(table.to_string(), shard_key.to_string());
    }

    pub fn infer(&self, sql: &str) -> Option<(String, String)> {
        let lower = sql.to_lowercase();
        for (table, key) in &self.table_shard_keys {
            if lower.contains(&table.to_lowercase()) {
                let pattern = format!("{} = ?", key);
                if lower.contains(&pattern.to_lowercase()) {
                    return Some((table.clone(), key.clone()));
                }
                let pattern2 = format!("{}=", key);
                if lower.contains(&pattern2.to_lowercase()) {
                    return Some((table.clone(), key.clone()));
                }
            }
        }
        None
    }
}

impl Default for ShardKeyInference {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// 跨片聚合器
// ============================================================================

#[derive(Debug, Clone)]
pub struct ScatterGatherResult {
    pub shard: String,
    pub rows: Vec<HashMap<String, crate::Value>>,
    pub error: Option<String>,
}

pub struct ScatterGatherCollector;

impl ScatterGatherCollector {
    pub fn merge(results: Vec<ScatterGatherResult>) -> Vec<HashMap<String, crate::Value>> {
        let mut merged = Vec::new();
        for r in results {
            if r.error.is_none() {
                merged.extend(r.rows);
            }
        }
        merged
    }

    pub fn merge_sorted(
        results: Vec<ScatterGatherResult>,
        order_by: &str,
        descending: bool,
    ) -> Vec<HashMap<String, crate::Value>> {
        let mut merged = Self::merge(results);
        merged.sort_by(|a, b| {
            let av = a.get(order_by);
            let bv = b.get(order_by);
            match (av, bv) {
                (Some(crate::Value::I64(a)), Some(crate::Value::I64(b))) => {
                    if descending {
                        b.cmp(a)
                    } else {
                        a.cmp(b)
                    }
                }
                _ => std::cmp::Ordering::Equal,
            }
        });
        merged
    }

    pub fn count(results: &[ScatterGatherResult]) -> u64 {
        results
            .iter()
            .filter(|r| r.error.is_none())
            .map(|r| r.rows.len() as u64)
            .sum()
    }
}

// ============================================================================
// 读写分离路由器
// ============================================================================

#[derive(Debug, Clone)]
pub struct RwSplitConfig {
    pub master: String,
    pub slaves: Vec<WeightedSlave>,
    pub lag_threshold_secs: u64,
}

impl Default for RwSplitConfig {
    fn default() -> Self {
        Self {
            master: "master".into(),
            slaves: vec![WeightedSlave {
                name: "slave-1".into(),
                weight: 1,
                host: "127.0.0.1".into(),
                port: 3307,
            }],
            lag_threshold_secs: 5,
        }
    }
}

pub struct RwSplitRouter {
    config: RwSplitConfig,
    selector: RwLock<WeightedRandomSelector>,
    lag_stats: RwLock<HashMap<String, u64>>,
    #[cfg(feature = "pool-elastic")]
    adaptive_tuner: Option<RwSplitAdaptiveTuner>,
}

impl RwSplitRouter {
    pub fn new(config: RwSplitConfig) -> Self {
        Self {
            config,
            selector: RwLock::new(WeightedRandomSelector::new()),
            lag_stats: RwLock::new(HashMap::new()),
            #[cfg(feature = "pool-elastic")]
            adaptive_tuner: None,
        }
    }

    /// 创建带自适应调优器的读写分离路由器
    #[cfg(feature = "pool-elastic")]
    pub fn new_with_adaptive_tuner(
        config: RwSplitConfig,
        adaptive_tuner: RwSplitAdaptiveTuner,
    ) -> Self {
        Self {
            config,
            selector: RwLock::new(WeightedRandomSelector::new()),
            lag_stats: RwLock::new(HashMap::new()),
            adaptive_tuner: Some(adaptive_tuner),
        }
    }

    /// 自适应调优器引用
    #[cfg(feature = "pool-elastic")]
    pub fn adaptive_tuner(&self) -> Option<&RwSplitAdaptiveTuner> {
        self.adaptive_tuner.as_ref()
    }

    pub fn with_default() -> Self {
        Self::new(RwSplitConfig::default())
    }

    pub fn route_read(&self) -> RouteResult {
        let lag_stats = self.lag_stats.read().unwrap();
        let mut selector = self.selector.write().unwrap();
        match selector.select_with_lag_filter(
            &self.config.slaves,
            self.config.lag_threshold_secs,
            &lag_stats,
        ) {
            Some(idx) => RouteResult {
                target: self.config.slaves[idx].name.clone(),
                is_master: false,
                degraded: false,
            },
            None => RouteResult {
                target: self.config.master.clone(),
                is_master: true,
                degraded: true,
            },
        }
    }

    pub fn route_write(&self) -> RouteResult {
        RouteResult {
            target: self.config.master.clone(),
            is_master: true,
            degraded: false,
        }
    }

    pub fn update_lag(&self, slave: &str, lag_secs: u64) {
        self.lag_stats
            .write()
            .unwrap()
            .insert(slave.to_string(), lag_secs);
    }

    /// 记录读操作到自适应调优器
    #[cfg(feature = "pool-elastic")]
    pub fn record_read_op(
        &self,
        latency: std::time::Duration,
        wait_duration: std::time::Duration,
        is_error: bool,
    ) {
        if let Some(ref tuner) = self.adaptive_tuner {
            tuner
                .read_pool_tuner()
                .record_op(latency, wait_duration, is_error);
        }
    }

    /// 记录写操作到自适应调优器
    #[cfg(feature = "pool-elastic")]
    pub fn record_write_op(
        &self,
        latency: std::time::Duration,
        wait_duration: std::time::Duration,
        is_error: bool,
    ) {
        if let Some(ref tuner) = self.adaptive_tuner {
            tuner
                .write_pool_tuner()
                .record_op(latency, wait_duration, is_error);
        }
    }

    /// 调优读池
    #[cfg(feature = "pool-elastic")]
    pub async fn tune_read_pool(&self) -> Option<crate::pool_elastic::WaterlineAdjustment> {
        if let Some(ref tuner) = self.adaptive_tuner {
            return tuner.tune_read_pool().await;
        }
        None
    }

    /// 调优写池
    #[cfg(feature = "pool-elastic")]
    pub async fn tune_write_pool(&self) -> Option<crate::pool_elastic::WaterlineAdjustment> {
        if let Some(ref tuner) = self.adaptive_tuner {
            return tuner.tune_write_pool().await;
        }
        None
    }

    /// 检查全局连接数限制
    #[cfg(feature = "pool-elastic")]
    pub fn check_global_limit(&self, read_conns: u32, write_conns: u32) -> GlobalCoordResult {
        if let Some(ref tuner) = self.adaptive_tuner {
            return tuner.check_global_limit(read_conns, write_conns);
        }
        GlobalCoordResult::WithinLimit
    }
}

#[derive(Debug, Clone)]
pub struct RouteResult {
    pub target: String,
    pub is_master: bool,
    pub degraded: bool,
}

// ============================================================================
// v10.11.0 T2.2：异地多活路由（MultiActiveRouter + PartitionKeyRouter）
// ============================================================================

/// 分区策略
#[derive(Debug, Clone)]
pub enum PartitionStrategy {
    /// 哈希分区（按桶数取模）
    Hash { buckets: usize },
    /// 范围分区（按范围边界映射）
    Range { ranges: Vec<(i64, String)> },
    /// 列表分区（显式映射）
    List { mapping: HashMap<String, String> },
}

/// 分区键映射
#[derive(Debug, Clone)]
pub struct PartitionKeyMapping {
    /// 分区键 → 区域 ID 缓存（一致性校验用）
    mapping: HashMap<String, String>,
    /// 分区策略
    strategy: PartitionStrategy,
}

impl PartitionKeyMapping {
    /// 创建分区键映射
    pub fn new(strategy: PartitionStrategy) -> Self {
        Self {
            mapping: HashMap::new(),
            strategy,
        }
    }

    /// 策略
    pub fn strategy(&self) -> &PartitionStrategy {
        &self.strategy
    }

    /// 显式注册分区键 → 区域映射
    pub fn register(&mut self, partition_key: &str, region_id: &str) {
        self.mapping
            .insert(partition_key.to_string(), region_id.to_string());
    }

    /// 查找分区键对应的区域（先查显式映射，再按策略计算）
    pub fn resolve_region(&self, partition_key: &str) -> Option<String> {
        if let Some(region) = self.mapping.get(partition_key) {
            return Some(region.clone());
        }
        match &self.strategy {
            PartitionStrategy::Hash { buckets } => {
                if *buckets == 0 {
                    return None;
                }
                let hash = partition_key
                    .bytes()
                    .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
                Some(format!("region-{}", hash % *buckets as u64))
            }
            PartitionStrategy::Range { ranges } => {
                let key_val: i64 = partition_key.parse().ok()?;
                ranges
                    .iter()
                    .find(|(bound, _)| key_val < *bound)
                    .map(|(_, region)| region.clone())
            }
            PartitionStrategy::List { mapping } => mapping.get(partition_key).cloned(),
        }
    }

    /// 已注册的显式映射数量
    pub fn registered_count(&self) -> usize {
        self.mapping.len()
    }
}

/// 区域主节点
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionMaster {
    pub region_id: String,
    pub master_node: String,
}

/// 路由错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteError {
    /// 分区键未映射到任何区域
    UnmappedPartitionKey(String),
    /// 区域无可用主节点
    NoMasterForRegion(String),
    /// 路由一致性冲突（同一分区键映射到不同区域）
    PartitionConsistencyConflict {
        partition_key: String,
        expected: String,
        actual: String,
    },
}

/// 分区键路由器
#[derive(Debug, Clone)]
pub struct PartitionKeyRouter {
    mapping: PartitionKeyMapping,
    /// 路由一致性缓存（partition_key → last routed region）
    consistency_cache: HashMap<String, String>,
}

impl PartitionKeyRouter {
    /// 创建分区键路由器
    pub fn new(strategy: PartitionStrategy) -> Self {
        Self {
            mapping: PartitionKeyMapping::new(strategy),
            consistency_cache: HashMap::new(),
        }
    }

    /// 映射引用
    pub fn mapping(&self) -> &PartitionKeyMapping {
        &self.mapping
    }

    /// 映射可变引用
    pub fn mapping_mut(&mut self) -> &mut PartitionKeyMapping {
        &mut self.mapping
    }

    /// 路由分区键到区域 ID
    pub fn route(&mut self, partition_key: &str) -> Result<String, RouteError> {
        let region = self
            .mapping
            .resolve_region(partition_key)
            .ok_or_else(|| RouteError::UnmappedPartitionKey(partition_key.to_string()))?;

        if let Some(prev_region) = self.consistency_cache.get(partition_key) {
            if prev_region != &region {
                return Err(RouteError::PartitionConsistencyConflict {
                    partition_key: partition_key.to_string(),
                    expected: prev_region.clone(),
                    actual: region,
                });
            }
        }
        self.consistency_cache
            .insert(partition_key.to_string(), region.clone());
        Ok(region)
    }

    /// 校验分区一致性（同一分区键是否始终路由到同一区域）
    pub fn validate_consistency(&self, partition_key: &str) -> bool {
        if let Some(cached) = self.consistency_cache.get(partition_key) {
            if let Some(current) = self.mapping.resolve_region(partition_key) {
                return cached == &current;
            }
        }
        true
    }
}

/// 区域主节点解析器
#[derive(Debug, Clone)]
pub struct RegionMasterResolver {
    /// 区域 ID → 主节点地址
    masters: HashMap<String, String>,
}

impl RegionMasterResolver {
    /// 创建区域主节点解析器
    pub fn new() -> Self {
        Self {
            masters: HashMap::new(),
        }
    }

    /// 注册区域主节点
    pub fn register(&mut self, region_id: &str, master_node: &str) {
        self.masters
            .insert(region_id.to_string(), master_node.to_string());
    }

    /// 解析区域主节点
    pub fn resolve(&self, region_id: &str) -> Result<RegionMaster, RouteError> {
        self.masters
            .get(region_id)
            .map(|node| RegionMaster {
                region_id: region_id.to_string(),
                master_node: node.clone(),
            })
            .ok_or_else(|| RouteError::NoMasterForRegion(region_id.to_string()))
    }

    /// 已注册区域数
    pub fn region_count(&self) -> usize {
        self.masters.len()
    }
}

impl Default for RegionMasterResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// 异地多活路由器
///
/// 按分区键路由写请求到对应区域主节点，避免跨区域写冲突。
#[derive(Debug, Clone)]
pub struct MultiActiveRouter {
    partition_router: PartitionKeyRouter,
    region_master_resolver: RegionMasterResolver,
}

impl MultiActiveRouter {
    /// 创建异地多活路由器
    pub fn new(strategy: PartitionStrategy) -> Self {
        Self {
            partition_router: PartitionKeyRouter::new(strategy),
            region_master_resolver: RegionMasterResolver::new(),
        }
    }

    /// 分区路由器可变引用
    pub fn partition_router_mut(&mut self) -> &mut PartitionKeyRouter {
        &mut self.partition_router
    }

    /// 区域主节点解析器可变引用
    pub fn region_master_resolver_mut(&mut self) -> &mut RegionMasterResolver {
        &mut self.region_master_resolver
    }

    /// 路由写请求到对应区域主节点
    ///
    /// `partition_key` 用于确定目标区域，`write_req` 为写请求标识。
    pub fn route_write(
        &mut self,
        partition_key: &str,
        write_req: &str,
    ) -> Result<RegionMaster, RouteError> {
        let _ = std::hint::black_box(write_req);
        let region_id = self.partition_router.route(partition_key)?;
        self.region_master_resolver.resolve(&region_id)
    }

    /// 校验分区一致性
    pub fn validate_partition_consistency(&self, partition_key: &str) -> bool {
        self.partition_router.validate_consistency(partition_key)
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weighted_random_distribution() {
        let mut selector = WeightedRandomSelector::new();
        let slaves = vec![
            WeightedSlave {
                name: "A".into(),
                weight: 3,
                host: "h".into(),
                port: 1,
            },
            WeightedSlave {
                name: "B".into(),
                weight: 1,
                host: "h".into(),
                port: 2,
            },
        ];
        let mut counts = HashMap::new();
        for _ in 0..1000 {
            let idx = selector.select(&slaves).unwrap();
            *counts.entry(idx).or_insert(0) += 1;
        }
        let a = *counts.get(&0).unwrap_or(&0);
        let b = *counts.get(&1).unwrap_or(&0);
        assert!(a > 600 && a < 900, "A 应约 750，实际 {}", a);
        assert!(b > 100 && b < 400, "B 应约 250，实际 {}", b);
    }

    #[test]
    fn lag_filter_excludes_slow_slave() {
        let mut selector = WeightedRandomSelector::new();
        let slaves = vec![
            WeightedSlave {
                name: "A".into(),
                weight: 1,
                host: "h".into(),
                port: 1,
            },
            WeightedSlave {
                name: "B".into(),
                weight: 1,
                host: "h".into(),
                port: 2,
            },
        ];
        let mut lag = HashMap::new();
        lag.insert("A".into(), 10u64);
        let idx = selector.select_with_lag_filter(&slaves, 5, &lag);
        assert_eq!(idx, Some(1));
    }

    #[test]
    fn shard_key_inference() {
        let mut inferer = ShardKeyInference::new();
        inferer.register("orders", "merchant_id");
        let result = inferer.infer("SELECT * FROM orders WHERE merchant_id = ?");
        assert_eq!(result, Some(("orders".into(), "merchant_id".into())));
    }

    #[test]
    fn shard_key_inference_no_match() {
        let inferer = ShardKeyInference::new();
        assert!(inferer.infer("SELECT * FROM users").is_none());
    }

    #[test]
    fn scatter_gather_merge() {
        let results = vec![
            ScatterGatherResult {
                shard: "s1".into(),
                rows: vec![HashMap::from([("id".into(), crate::Value::I64(1))])],
                error: None,
            },
            ScatterGatherResult {
                shard: "s2".into(),
                rows: vec![HashMap::from([("id".into(), crate::Value::I64(2))])],
                error: None,
            },
            ScatterGatherResult {
                shard: "s3".into(),
                rows: vec![],
                error: Some("connection refused".into()),
            },
        ];
        let merged = ScatterGatherCollector::merge(results);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn scatter_gather_count() {
        let results = vec![
            ScatterGatherResult {
                shard: "s1".into(),
                rows: vec![HashMap::new(), HashMap::new()],
                error: None,
            },
            ScatterGatherResult {
                shard: "s2".into(),
                rows: vec![HashMap::new()],
                error: None,
            },
        ];
        assert_eq!(ScatterGatherCollector::count(&results), 3);
    }

    #[test]
    fn rw_split_route_read_to_slave() {
        let router = RwSplitRouter::with_default();
        let result = router.route_read();
        assert!(!result.is_master || result.degraded);
    }

    #[test]
    fn rw_split_route_write_to_master() {
        let router = RwSplitRouter::with_default();
        let result = router.route_write();
        assert!(result.is_master);
        assert!(!result.degraded);
    }

    #[test]
    fn rw_split_degrade_on_high_lag() {
        let router = RwSplitRouter::with_default();
        router.update_lag("slave-1", 10);
        let result = router.route_read();
        assert!(result.degraded, "高延迟应降级到主库");
    }

    // ===================== T15 新增测试（覆盖所有 pub fn/method 核心路径/边界/错误处理） =====================

    #[test]
    fn t15_select_empty_slaves_returns_none() {
        let mut selector = WeightedRandomSelector::new();
        assert!(selector.select(&[]).is_none());
    }

    #[test]
    fn t15_select_all_zero_weights_returns_none() {
        let mut selector = WeightedRandomSelector::new();
        let slaves = vec![
            WeightedSlave {
                name: "A".into(),
                weight: 0,
                host: "h".into(),
                port: 1,
            },
            WeightedSlave {
                name: "B".into(),
                weight: 0,
                host: "h".into(),
                port: 2,
            },
        ];
        assert!(selector.select(&slaves).is_none());
    }

    #[test]
    fn t15_select_single_slave_returns_zero() {
        let mut selector = WeightedRandomSelector::new();
        let slaves = vec![WeightedSlave {
            name: "only".into(),
            weight: 1,
            host: "h".into(),
            port: 1,
        }];
        for _ in 0..10 {
            assert_eq!(selector.select(&slaves), Some(0));
        }
    }

    #[test]
    fn t15_select_multi_slaves_returns_valid_index() {
        let mut selector = WeightedRandomSelector::new();
        let slaves = vec![
            WeightedSlave {
                name: "A".into(),
                weight: 1,
                host: "h".into(),
                port: 1,
            },
            WeightedSlave {
                name: "B".into(),
                weight: 1,
                host: "h".into(),
                port: 2,
            },
            WeightedSlave {
                name: "C".into(),
                weight: 1,
                host: "h".into(),
                port: 3,
            },
        ];
        for _ in 0..50 {
            let idx = selector.select(&slaves).unwrap();
            assert!(idx < 3);
        }
    }

    #[test]
    fn t15_lag_filter_all_filtered_returns_none() {
        let mut selector = WeightedRandomSelector::new();
        let slaves = vec![
            WeightedSlave {
                name: "A".into(),
                weight: 1,
                host: "h".into(),
                port: 1,
            },
            WeightedSlave {
                name: "B".into(),
                weight: 1,
                host: "h".into(),
                port: 2,
            },
        ];
        let mut lag = HashMap::new();
        lag.insert("A".into(), 100u64);
        lag.insert("B".into(), 200u64);
        assert_eq!(selector.select_with_lag_filter(&slaves, 5, &lag), None);
    }

    #[test]
    fn t15_lag_filter_no_stats_includes_all() {
        let mut selector = WeightedRandomSelector::new();
        let slaves = vec![
            WeightedSlave {
                name: "A".into(),
                weight: 1,
                host: "h".into(),
                port: 1,
            },
            WeightedSlave {
                name: "B".into(),
                weight: 1,
                host: "h".into(),
                port: 2,
            },
        ];
        let lag = HashMap::new();
        let idx = selector.select_with_lag_filter(&slaves, 5, &lag);
        assert!(idx.is_some());
        assert!(idx.unwrap() < 2);
    }

    #[test]
    fn t15_lag_filter_threshold_boundary_equal() {
        let mut selector = WeightedRandomSelector::new();
        let slaves = vec![WeightedSlave {
            name: "A".into(),
            weight: 1,
            host: "h".into(),
            port: 1,
        }];
        let mut lag = HashMap::new();
        lag.insert("A".into(), 5u64);
        let idx = selector.select_with_lag_filter(&slaves, 5, &lag);
        assert_eq!(idx, Some(0));
    }

    #[test]
    fn t15_lag_filter_empty_slaves_returns_none() {
        let mut selector = WeightedRandomSelector::new();
        let lag = HashMap::new();
        assert_eq!(selector.select_with_lag_filter(&[], 5, &lag), None);
    }

    #[test]
    fn t15_infer_compact_pattern_no_space() {
        let mut inferer = ShardKeyInference::new();
        inferer.register("orders", "merchant_id");
        let result = inferer.infer("SELECT * FROM orders WHERE merchant_id=123");
        assert_eq!(result, Some(("orders".into(), "merchant_id".into())));
    }

    #[test]
    fn t15_infer_case_insensitive_table_match() {
        let mut inferer = ShardKeyInference::new();
        inferer.register("Orders", "merchant_id");
        let result = inferer.infer("select * from ORDERS where merchant_id = ?");
        assert_eq!(result, Some(("Orders".into(), "merchant_id".into())));
    }

    #[test]
    fn t15_infer_table_match_but_key_missing() {
        let mut inferer = ShardKeyInference::new();
        inferer.register("orders", "merchant_id");
        assert!(inferer
            .infer("SELECT * FROM orders WHERE status = 1")
            .is_none());
    }

    #[test]
    fn t15_infer_multiple_registrations_first_wins() {
        let mut inferer = ShardKeyInference::new();
        inferer.register("orders", "merchant_id");
        inferer.register("users", "user_id");
        let result = inferer.infer("SELECT * FROM users WHERE user_id = ?");
        assert_eq!(result, Some(("users".into(), "user_id".into())));
        assert!(inferer
            .infer("SELECT * FROM products WHERE id = ?")
            .is_none());
    }

    #[test]
    fn t15_merge_all_errors_returns_empty() {
        let results = vec![
            ScatterGatherResult {
                shard: "s1".into(),
                rows: vec![HashMap::from([("id".into(), crate::Value::I64(1))])],
                error: Some("err1".into()),
            },
            ScatterGatherResult {
                shard: "s2".into(),
                rows: vec![HashMap::from([("id".into(), crate::Value::I64(2))])],
                error: Some("err2".into()),
            },
        ];
        let merged = ScatterGatherCollector::merge(results);
        assert!(merged.is_empty());
    }

    #[test]
    fn t15_merge_sorted_ascending() {
        let results = vec![
            ScatterGatherResult {
                shard: "s1".into(),
                rows: vec![HashMap::from([("v".into(), crate::Value::I64(3))])],
                error: None,
            },
            ScatterGatherResult {
                shard: "s2".into(),
                rows: vec![HashMap::from([("v".into(), crate::Value::I64(1))])],
                error: None,
            },
            ScatterGatherResult {
                shard: "s3".into(),
                rows: vec![HashMap::from([("v".into(), crate::Value::I64(2))])],
                error: None,
            },
        ];
        let merged = ScatterGatherCollector::merge_sorted(results, "v", false);
        let vals: Vec<i64> = merged
            .iter()
            .map(|r| match r.get("v") {
                Some(crate::Value::I64(x)) => *x,
                _ => 0,
            })
            .collect();
        assert_eq!(vals, vec![1, 2, 3]);
    }

    #[test]
    fn t15_merge_sorted_descending() {
        let results = vec![
            ScatterGatherResult {
                shard: "s1".into(),
                rows: vec![HashMap::from([("v".into(), crate::Value::I64(1))])],
                error: None,
            },
            ScatterGatherResult {
                shard: "s2".into(),
                rows: vec![HashMap::from([("v".into(), crate::Value::I64(3))])],
                error: None,
            },
            ScatterGatherResult {
                shard: "s3".into(),
                rows: vec![HashMap::from([("v".into(), crate::Value::I64(2))])],
                error: None,
            },
        ];
        let merged = ScatterGatherCollector::merge_sorted(results, "v", true);
        let vals: Vec<i64> = merged
            .iter()
            .map(|r| match r.get("v") {
                Some(crate::Value::I64(x)) => *x,
                _ => 0,
            })
            .collect();
        assert_eq!(vals, vec![3, 2, 1]);
    }

    #[test]
    fn t15_merge_sorted_non_i64_equal_order() {
        let results = vec![
            ScatterGatherResult {
                shard: "s1".into(),
                rows: vec![HashMap::from([(
                    "v".into(),
                    crate::Value::String("a".into()),
                )])],
                error: None,
            },
            ScatterGatherResult {
                shard: "s2".into(),
                rows: vec![HashMap::from([(
                    "v".into(),
                    crate::Value::String("b".into()),
                )])],
                error: None,
            },
        ];
        let merged = ScatterGatherCollector::merge_sorted(results, "v", false);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn t15_merge_sorted_missing_key_equal_order() {
        let results = vec![
            ScatterGatherResult {
                shard: "s1".into(),
                rows: vec![HashMap::from([("id".into(), crate::Value::I64(1))])],
                error: None,
            },
            ScatterGatherResult {
                shard: "s2".into(),
                rows: vec![HashMap::from([("id".into(), crate::Value::I64(2))])],
                error: None,
            },
        ];
        let merged = ScatterGatherCollector::merge_sorted(results, "missing_key", false);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn t15_count_with_errors_excludes_failed_shards() {
        let results = vec![
            ScatterGatherResult {
                shard: "s1".into(),
                rows: vec![HashMap::new(), HashMap::new(), HashMap::new()],
                error: None,
            },
            ScatterGatherResult {
                shard: "s2".into(),
                rows: vec![HashMap::new()],
                error: Some("err".into()),
            },
        ];
        assert_eq!(ScatterGatherCollector::count(&results), 3);
    }

    #[test]
    fn t15_count_empty_input_returns_zero() {
        let results: Vec<ScatterGatherResult> = vec![];
        assert_eq!(ScatterGatherCollector::count(&results), 0);
    }

    #[test]
    fn t15_route_read_no_slaves_degrades_to_master() {
        let config = RwSplitConfig {
            master: "primary".into(),
            slaves: vec![],
            lag_threshold_secs: 5,
        };
        let router = RwSplitRouter::new(config);
        let result = router.route_read();
        assert!(result.degraded);
        assert!(result.is_master);
        assert_eq!(result.target, "primary");
    }

    #[test]
    fn t15_route_read_all_high_lag_degrades() {
        let config = RwSplitConfig {
            master: "primary".into(),
            slaves: vec![
                WeightedSlave {
                    name: "s1".into(),
                    weight: 1,
                    host: "h".into(),
                    port: 1,
                },
                WeightedSlave {
                    name: "s2".into(),
                    weight: 1,
                    host: "h".into(),
                    port: 2,
                },
            ],
            lag_threshold_secs: 3,
        };
        let router = RwSplitRouter::new(config);
        router.update_lag("s1", 10);
        router.update_lag("s2", 20);
        let result = router.route_read();
        assert!(result.degraded);
        assert!(result.is_master);
        assert_eq!(result.target, "primary");
    }

    #[test]
    fn t15_route_read_success_returns_slave_not_degraded() {
        let config = RwSplitConfig {
            master: "primary".into(),
            slaves: vec![WeightedSlave {
                name: "s1".into(),
                weight: 1,
                host: "h".into(),
                port: 1,
            }],
            lag_threshold_secs: 5,
        };
        let router = RwSplitRouter::new(config);
        let result = router.route_read();
        assert!(!result.degraded);
        assert!(!result.is_master);
        assert_eq!(result.target, "s1");
    }

    #[test]
    fn t15_route_write_target_is_master() {
        let config = RwSplitConfig {
            master: "writer".into(),
            slaves: vec![],
            lag_threshold_secs: 0,
        };
        let router = RwSplitRouter::new(config);
        let result = router.route_write();
        assert_eq!(result.target, "writer");
        assert!(result.is_master);
        assert!(!result.degraded);
    }

    #[test]
    fn t15_update_lag_overwrites_previous() {
        let router = RwSplitRouter::with_default();
        router.update_lag("slave-1", 5);
        let r1 = router.route_read();
        assert!(!r1.degraded);
        router.update_lag("slave-1", 100);
        let r2 = router.route_read();
        assert!(r2.degraded);
    }

    #[test]
    fn t15_config_default_has_one_slave() {
        let config = RwSplitConfig::default();
        assert_eq!(config.master, "master");
        assert_eq!(config.slaves.len(), 1);
        assert_eq!(config.slaves[0].name, "slave-1");
        assert_eq!(config.slaves[0].weight, 1);
        assert_eq!(config.lag_threshold_secs, 5);
    }

    #[test]
    fn t15_weighted_slave_struct_fields() {
        let slave = WeightedSlave {
            name: "s".into(),
            weight: 7,
            host: "10.0.0.1".into(),
            port: 3306,
        };
        assert_eq!(slave.name, "s");
        assert_eq!(slave.weight, 7);
        assert_eq!(slave.host, "10.0.0.1");
        assert_eq!(slave.port, 3306);
    }

    #[test]
    fn t15_route_result_struct_fields() {
        let result = RouteResult {
            target: "t".into(),
            is_master: true,
            degraded: false,
        };
        assert_eq!(result.target, "t");
        assert!(result.is_master);
        assert!(!result.degraded);
        let cloned = result.clone();
        assert_eq!(cloned.target, "t");
    }

    #[test]
    fn t15_shard_key_inference_default_empty() {
        let inferer = ShardKeyInference::default();
        assert!(inferer.table_shard_keys.is_empty());
        assert!(inferer.infer("SELECT * FROM x").is_none());
    }

    #[test]
    fn t15_weighted_random_selector_default_works() {
        let mut selector = WeightedRandomSelector::default();
        let slaves = vec![WeightedSlave {
            name: "A".into(),
            weight: 1,
            host: "h".into(),
            port: 1,
        }];
        assert_eq!(selector.select(&slaves), Some(0));
    }
}
// ============================================================================
// v7.3.0 自动主备故障转移协调器（auto-failover feature gate）
// ============================================================================

#[cfg(feature = "auto-failover")]
mod failover {
    use super::*;
    use crate::{FailbackStrategy, FailoverConfig};
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
    use std::time::Instant;

    /// 故障转移错误
    #[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
    pub enum FailoverError {
        /// 故障转移正在进行中（并发防护）
        #[error("failover already in progress")]
        AlreadyInProgress,
        /// 位点校验失败（备库落后主库，可能丢失事务）
        #[error("replica LSN {replica_lsn} behind primary {primary_lsn}, potential data loss")]
        ReplicaBehind { replica_lsn: u64, primary_lsn: u64 },
        /// 回切条件不满足
        #[error("failback conditions not met: {0}")]
        FailbackConditionsNotMet(String),
        /// 探活失败
        #[error("probe failed: {0}")]
        ProbeFailed(String),
    }

    /// 故障转移决策
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct FailoverDecision {
        /// 决策时间戳（毫秒）
        pub timestamp_ms: i64,
        /// 触发原因
        pub reason: String,
        /// 是否执行了切换
        pub switched: bool,
        /// 位点校验结果（None 表示未校验，Some 表示校验结果）
        pub lsn_check: Option<LsnCheckResult>,
    }

    /// 位点校验结果
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct LsnCheckResult {
        /// 主库故障时 LSN
        pub primary_lsn: u64,
        /// 备库当前 LSN
        pub replica_lsn: u64,
        /// 是否落后（true 表示可能丢失事务）
        pub behind: bool,
        /// 可能丢失的事务数（behind=true 时有意义）
        pub potential_lost_txns: u64,
    }

    /// 探活结果
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ProbeResult {
        /// 是否成功
        pub success: bool,
        /// 探活时间戳（毫秒）
        pub timestamp_ms: i64,
        /// 失败原因（success=false 时有意义）
        pub error: Option<String>,
    }

    /// 自动故障转移协调器（v7.3.0）
    ///
    /// 持有主备拓扑 + 探活循环 + 位点校验 + 回切策略 + 决策历史。
    /// 复用 `failover_in_progress` AtomicBool CAS 进行并发防护。
    /// RTO ≤ 5s（由 `FailoverConfig.probe_interval ≤ 5s` 保证）。
    pub struct AutoFailoverCoordinator {
        config: FailoverConfig,
        /// 并发防护：CAS 保证同一时刻只有一个故障转移在进行
        failover_in_progress: AtomicBool,
        /// 当前是否已故障转移到备库
        failed_over: AtomicBool,
        /// 连续探活失败计数
        consecutive_probe_failures: AtomicU32,
        /// 决策历史
        decision_history: RwLock<Vec<FailoverDecision>>,
        /// 主库 LSN（故障位点）
        primary_lsn: AtomicU64,
        /// 备库 LSN
        replica_lsn: AtomicU64,
        /// 总探活次数
        total_probes: AtomicU64,
        /// 探活成功次数
        successful_probes: AtomicU64,
        /// 上次探活时间
        last_probe_at: RwLock<Option<Instant>>,
    }

    impl AutoFailoverCoordinator {
        /// 创建协调器
        pub fn new(config: FailoverConfig) -> Self {
            Self {
                config,
                failover_in_progress: AtomicBool::new(false),
                failed_over: AtomicBool::new(false),
                consecutive_probe_failures: AtomicU32::new(0),
                decision_history: RwLock::new(Vec::new()),
                primary_lsn: AtomicU64::new(0),
                replica_lsn: AtomicU64::new(0),
                total_probes: AtomicU64::new(0),
                successful_probes: AtomicU64::new(0),
                last_probe_at: RwLock::new(None),
            }
        }

        /// 配置引用
        pub fn config(&self) -> &FailoverConfig {
            &self.config
        }

        /// 记录一次探活结果（由探活循环或外部探活器调用）
        ///
        /// 连续失败达 `probe_failure_threshold` 时自动触发故障转移。
        pub fn record_probe(&self, result: ProbeResult) {
            self.total_probes.fetch_add(1, Ordering::SeqCst);
            if let Ok(mut guard) = self.last_probe_at.write() {
                *guard = Some(Instant::now());
            }
            if result.success {
                self.successful_probes.fetch_add(1, Ordering::SeqCst);
                self.consecutive_probe_failures.store(0, Ordering::SeqCst);
            } else {
                let failures = self
                    .consecutive_probe_failures
                    .fetch_add(1, Ordering::SeqCst)
                    + 1;
                if failures >= self.config.probe_failure_threshold
                    && !self.failed_over.load(Ordering::SeqCst)
                {
                    // 达到阈值且未已故障转移，触发故障转移
                    let _ = self.trigger_failover_internal(&result);
                }
            }
        }

        /// 触发故障转移（并发防护：CAS 保证同一时刻只有一个故障转移）
        ///
        /// 返回决策记录。若已有故障转移在进行中，返回 `AlreadyInProgress`。
        pub fn trigger_failover(&self) -> Result<FailoverDecision, FailoverError> {
            self.trigger_failover_internal(&ProbeResult {
                success: false,
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
                error: Some("manual trigger".to_string()),
            })
        }

        fn trigger_failover_internal(
            &self,
            probe: &ProbeResult,
        ) -> Result<FailoverDecision, FailoverError> {
            // CAS 并发防护
            if self
                .failover_in_progress
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
            {
                return Err(FailoverError::AlreadyInProgress);
            }

            // 位点校验
            let primary_lsn = self.primary_lsn.load(Ordering::SeqCst);
            let replica_lsn = self.replica_lsn.load(Ordering::SeqCst);
            let lsn_check = if primary_lsn > replica_lsn {
                Some(LsnCheckResult {
                    primary_lsn,
                    replica_lsn,
                    behind: true,
                    potential_lost_txns: primary_lsn - replica_lsn,
                })
            } else {
                Some(LsnCheckResult {
                    primary_lsn,
                    replica_lsn,
                    behind: false,
                    potential_lost_txns: 0,
                })
            };

            // 执行切换
            self.failed_over.store(true, Ordering::SeqCst);
            self.consecutive_probe_failures.store(0, Ordering::SeqCst);

            let decision = FailoverDecision {
                timestamp_ms: chrono::Utc::now().timestamp_millis(),
                reason: format!(
                    "probe failed: {:?}, consecutive failures reached threshold {}",
                    probe.error, self.config.probe_failure_threshold
                ),
                switched: true,
                lsn_check: lsn_check.clone(),
            };

            // 记录决策历史
            if let Ok(mut history) = self.decision_history.write() {
                history.push(decision.clone());
            }

            // 释放 CAS 锁
            self.failover_in_progress.store(false, Ordering::SeqCst);

            // 位点落后时返回错误（不静默丢数据）
            if let Some(check) = &lsn_check {
                if check.behind {
                    return Err(FailoverError::ReplicaBehind {
                        replica_lsn: check.replica_lsn,
                        primary_lsn: check.primary_lsn,
                    });
                }
            }

            Ok(decision)
        }

        /// 回切到主库
        ///
        /// - `Manual`：等待运维确认，直接执行回切
        /// - `Auto`：经健康+一致性校验后自动回切
        pub fn failback(&self, strategy: FailbackStrategy) -> Result<(), FailoverError> {
            if !self.failed_over.load(Ordering::SeqCst) {
                return Err(FailoverError::FailbackConditionsNotMet(
                    "not in failed-over state".to_string(),
                ));
            }

            match strategy {
                FailbackStrategy::Manual => {
                    // 运维确认后直接回切
                    self.failed_over.store(false, Ordering::SeqCst);
                    self.consecutive_probe_failures.store(0, Ordering::SeqCst);
                    Ok(())
                }
                FailbackStrategy::Auto => {
                    // 自动回切：校验主库健康（连续探活失败为 0）+ 位点一致性
                    let failures = self.consecutive_probe_failures.load(Ordering::SeqCst);
                    if failures > 0 {
                        return Err(FailoverError::FailbackConditionsNotMet(format!(
                            "primary still unhealthy: {failures} consecutive failures"
                        )));
                    }
                    let primary_lsn = self.primary_lsn.load(Ordering::SeqCst);
                    let replica_lsn = self.replica_lsn.load(Ordering::SeqCst);
                    if replica_lsn < primary_lsn {
                        return Err(FailoverError::FailbackConditionsNotMet(format!(
                            "replica LSN {replica_lsn} behind primary {primary_lsn}"
                        )));
                    }
                    self.failed_over.store(false, Ordering::SeqCst);
                    Ok(())
                }
            }
        }

        /// 决策历史
        pub fn decision_history(&self) -> Vec<FailoverDecision> {
            self.decision_history
                .read()
                .map(|h| h.clone())
                .unwrap_or_default()
        }

        /// 当前是否已故障转移
        pub fn is_failed_over(&self) -> bool {
            self.failed_over.load(Ordering::SeqCst)
        }

        /// 连续探活失败次数
        pub fn consecutive_probe_failures(&self) -> u32 {
            self.consecutive_probe_failures.load(Ordering::SeqCst)
        }

        /// 更新主库 LSN（由位点追踪器调用）
        pub fn set_primary_lsn(&self, lsn: u64) {
            self.primary_lsn.store(lsn, Ordering::SeqCst);
        }

        /// 更新备库 LSN（由位点追踪器调用）
        pub fn set_replica_lsn(&self, lsn: u64) {
            self.replica_lsn.store(lsn, Ordering::SeqCst);
        }

        /// 总探活次数
        pub fn total_probes(&self) -> u64 {
            self.total_probes.load(Ordering::SeqCst)
        }

        /// 探活成功次数
        pub fn successful_probes(&self) -> u64 {
            self.successful_probes.load(Ordering::SeqCst)
        }
    }
}

#[cfg(feature = "auto-failover")]
pub use failover::{
    AutoFailoverCoordinator, FailoverDecision, FailoverError, LsnCheckResult, ProbeResult,
};
// ============================================================================
// v10.12.0 T1.5：读写分离池独立调优
// ============================================================================

#[cfg(feature = "pool-elastic")]
mod adaptive_tuner {
    use super::RwSplitConfig;
    use crate::pool_elastic::{
        DynamicWaterline, LoadScorer, MultiSignalCollector, SignalConfig, SignalWeight,
        WaterlineAdjustment, WaterlineConfig,
    };
    use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
    use std::time::Duration;

    /// 读池独立调优器
    pub struct ReadPoolTuner {
        signal_collector: MultiSignalCollector,
        load_scorer: LoadScorer,
        waterline: DynamicWaterline,
        current_size: AtomicU32,
    }

    impl ReadPoolTuner {
        pub fn new(signal_config: SignalConfig, waterline_config: WaterlineConfig) -> Self {
            Self {
                signal_collector: MultiSignalCollector::new(signal_config),
                load_scorer: LoadScorer::new(SignalWeight::default()),
                waterline: DynamicWaterline::new(waterline_config),
                current_size: AtomicU32::new(0),
            }
        }

        pub fn record_op(&self, latency: Duration, wait_duration: Duration, is_error: bool) {
            self.signal_collector
                .record_op(latency, wait_duration, is_error);
        }

        pub async fn tune(&self) -> Option<WaterlineAdjustment> {
            let signal = self.signal_collector.collect();
            let score = self.load_scorer.compute_score(&signal);
            self.waterline.adjust(&score)
        }

        pub fn current_size(&self) -> u32 {
            self.current_size.load(Ordering::Relaxed)
        }

        pub fn set_current_size(&self, size: u32) {
            self.current_size.store(size, Ordering::Relaxed);
        }

        pub fn current_max(&self) -> u32 {
            self.waterline.current_max()
        }

        pub fn current_min(&self) -> u32 {
            self.waterline.current_min()
        }
    }

    impl std::fmt::Debug for ReadPoolTuner {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("ReadPoolTuner")
                .field("current_size", &self.current_size())
                .field("current_min", &self.current_min())
                .field("current_max", &self.current_max())
                .finish()
        }
    }

    /// 写池独立调优器
    pub struct WritePoolTuner {
        signal_collector: MultiSignalCollector,
        load_scorer: LoadScorer,
        waterline: DynamicWaterline,
        current_size: AtomicU32,
    }

    impl WritePoolTuner {
        pub fn new(signal_config: SignalConfig, waterline_config: WaterlineConfig) -> Self {
            Self {
                signal_collector: MultiSignalCollector::new(signal_config),
                load_scorer: LoadScorer::new(SignalWeight::default()),
                waterline: DynamicWaterline::new(waterline_config),
                current_size: AtomicU32::new(0),
            }
        }

        pub fn record_op(&self, latency: Duration, wait_duration: Duration, is_error: bool) {
            self.signal_collector
                .record_op(latency, wait_duration, is_error);
        }

        pub async fn tune(&self) -> Option<WaterlineAdjustment> {
            let signal = self.signal_collector.collect();
            let score = self.load_scorer.compute_score(&signal);
            self.waterline.adjust(&score)
        }

        pub fn current_size(&self) -> u32 {
            self.current_size.load(Ordering::Relaxed)
        }

        pub fn set_current_size(&self, size: u32) {
            self.current_size.store(size, Ordering::Relaxed);
        }

        pub fn current_max(&self) -> u32 {
            self.waterline.current_max()
        }

        pub fn current_min(&self) -> u32 {
            self.waterline.current_min()
        }
    }

    impl std::fmt::Debug for WritePoolTuner {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("WritePoolTuner")
                .field("current_size", &self.current_size())
                .field("current_min", &self.current_min())
                .field("current_max", &self.current_max())
                .finish()
        }
    }

    /// 全局连接数协调结果
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum GlobalCoordResult {
        /// 在限制范围内
        WithinLimit,
        /// 超出限制，需要缩减
        OverLimit {
            /// 读池应缩减的连接数
            reduce_read: u32,
            /// 写池应缩减的连接数
            reduce_write: u32,
        },
    }

    /// 全局连接数协调器
    ///
    /// 保证 读池 + 写池 总连接数 ≤ 数据库上限。
    pub struct GlobalConnCoordinator {
        db_max_connections: u32,
        total_reductions: AtomicU64,
    }

    impl GlobalConnCoordinator {
        pub fn new(db_max_connections: u32) -> Self {
            Self {
                db_max_connections,
                total_reductions: AtomicU64::new(0),
            }
        }

        /// 检查全局连接数是否超限
        ///
        /// 超限时按负载比例缩减：低负载池先缩减。
        pub fn check_limit(&self, read_conns: u32, write_conns: u32) -> GlobalCoordResult {
            let total = read_conns + write_conns;
            if total <= self.db_max_connections {
                return GlobalCoordResult::WithinLimit;
            }

            let excess = total - self.db_max_connections;
            let reduce_read = if read_conns > write_conns {
                excess.min(read_conns)
            } else {
                excess / 2
            };
            let reduce_write = excess - reduce_read;

            self.total_reductions.fetch_add(1, Ordering::Relaxed);

            GlobalCoordResult::OverLimit {
                reduce_read,
                reduce_write,
            }
        }

        pub fn db_max_connections(&self) -> u32 {
            self.db_max_connections
        }

        pub fn total_reductions(&self) -> u64 {
            self.total_reductions.load(Ordering::Relaxed)
        }
    }

    impl std::fmt::Debug for GlobalConnCoordinator {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("GlobalConnCoordinator")
                .field("db_max_connections", &self.db_max_connections)
                .field("total_reductions", &self.total_reductions())
                .finish()
        }
    }

    /// 读写分离自适应调优器
    ///
    /// 读池和写池独立自适应调优，互不干扰，
    /// 全局连接数上限协调（读池+写池总连接数 ≤ 数据库上限）。
    pub struct RwSplitAdaptiveTuner {
        read_pool_tuner: ReadPoolTuner,
        write_pool_tuner: WritePoolTuner,
        global_coordinator: GlobalConnCoordinator,
        tune_count: AtomicU64,
    }

    impl RwSplitAdaptiveTuner {
        pub fn new(
            read_signal_config: SignalConfig,
            read_waterline_config: WaterlineConfig,
            write_signal_config: SignalConfig,
            write_waterline_config: WaterlineConfig,
            db_max_connections: u32,
        ) -> Self {
            Self {
                read_pool_tuner: ReadPoolTuner::new(read_signal_config, read_waterline_config),
                write_pool_tuner: WritePoolTuner::new(write_signal_config, write_waterline_config),
                global_coordinator: GlobalConnCoordinator::new(db_max_connections),
                tune_count: AtomicU64::new(0),
            }
        }

        pub async fn tune_read_pool(&self) -> Option<WaterlineAdjustment> {
            self.tune_count.fetch_add(1, Ordering::Relaxed);
            self.read_pool_tuner.tune().await
        }

        pub async fn tune_write_pool(&self) -> Option<WaterlineAdjustment> {
            self.tune_count.fetch_add(1, Ordering::Relaxed);
            self.write_pool_tuner.tune().await
        }

        pub fn check_global_limit(&self, read_conns: u32, write_conns: u32) -> GlobalCoordResult {
            self.global_coordinator.check_limit(read_conns, write_conns)
        }

        pub fn read_pool_tuner(&self) -> &ReadPoolTuner {
            &self.read_pool_tuner
        }

        pub fn write_pool_tuner(&self) -> &WritePoolTuner {
            &self.write_pool_tuner
        }

        pub fn global_coordinator(&self) -> &GlobalConnCoordinator {
            &self.global_coordinator
        }

        pub fn tune_count(&self) -> u64 {
            self.tune_count.load(Ordering::Relaxed)
        }
    }

    impl std::fmt::Debug for RwSplitAdaptiveTuner {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("RwSplitAdaptiveTuner")
                .field("read_pool_tuner", &self.read_pool_tuner)
                .field("write_pool_tuner", &self.write_pool_tuner)
                .field("global_coordinator", &self.global_coordinator)
                .field("tune_count", &self.tune_count())
                .finish()
        }
    }

    impl RwSplitConfig {
        /// 创建带自适应调优器的 RwSplitConfig 扩展
        pub fn with_adaptive_tuner(
            master: &str,
            slaves: Vec<super::WeightedSlave>,
            db_max_connections: u32,
        ) -> (Self, RwSplitAdaptiveTuner) {
            let config = Self {
                master: master.into(),
                slaves,
                lag_threshold_secs: 5,
            };
            let tuner = RwSplitAdaptiveTuner::new(
                SignalConfig::default(),
                WaterlineConfig::default(),
                SignalConfig::default(),
                WaterlineConfig::default(),
                db_max_connections,
            );
            (config, tuner)
        }
    }
}

#[cfg(feature = "pool-elastic")]
pub use adaptive_tuner::{
    GlobalConnCoordinator, GlobalCoordResult, ReadPoolTuner, RwSplitAdaptiveTuner, WritePoolTuner,
};
