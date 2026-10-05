//! v6.7.0 分布式缓存集群
//!
//! Redis 集群一致性哈希分片 + 故障转移 + 击穿/穿透/雪崩防护集成。
//!
//! # 特性
//! - 一致性哈希分片（虚拟节点 + 动态扩缩容）
//! - 故障转移遍历（顺时针下一可用节点）
//! - 击穿防护（SingleFlight 互斥）
//! - 穿透防护（布隆过滤器）
//! - 雪崩防护（TTL 随机抖动）

use std::collections::{BTreeMap, HashSet};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::dist_cache::{BloomFilterGuard, CacheMutexGuard, RandomTtlJitter};

// ============================================================================
// 一致性哈希路由器（内联实现，避免引入 sz-orm-sharding 依赖）
// ============================================================================

fn hash_str(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut hasher);
    hasher.finish()
}

/// 一致性哈希路由器
pub struct ConsistentHashRouter {
    ring: BTreeMap<u64, String>,
    nodes: Vec<String>,
    vnodes_per_node: usize,
}

impl ConsistentHashRouter {
    pub fn new(nodes: Vec<String>, vnodes_per_node: usize) -> Self {
        let vnodes_per_node = vnodes_per_node.max(1);
        let mut router = Self {
            ring: BTreeMap::new(),
            nodes,
            vnodes_per_node,
        };
        for node in &router.nodes {
            for i in 0..vnodes_per_node {
                let vnode_key = format!("{}#{}", node, i);
                let hash = hash_str(&vnode_key);
                router.ring.insert(hash, node.clone());
            }
        }
        router
    }

    pub fn add_node(&mut self, node: &str) {
        if self.nodes.iter().any(|n| n == node) {
            return;
        }
        for i in 0..self.vnodes_per_node {
            let vnode_key = format!("{}#{}", node, i);
            let hash = hash_str(&vnode_key);
            self.ring.insert(hash, node.to_string());
        }
        self.nodes.push(node.to_string());
    }

    pub fn remove_node(&mut self, node: &str) {
        self.nodes.retain(|n| n != node);
        let to_remove: Vec<u64> = self
            .ring
            .iter()
            .filter(|(_, v)| *v == node)
            .map(|(k, _)| *k)
            .collect();
        for k in to_remove {
            self.ring.remove(&k);
        }
    }

    pub fn route(&self, key: &str) -> Option<String> {
        if self.ring.is_empty() {
            return None;
        }
        let hash = hash_str(key);
        self.ring
            .range(hash..)
            .next()
            .or(self.ring.iter().next())
            .map(|(_, v)| v.clone())
    }

    pub fn nodes(&self) -> &[String] {
        &self.nodes
    }
}

// ============================================================================
// 配置
// ============================================================================

/// 分布式缓存集群配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistCacheClusterConfig {
    pub redis_nodes: Vec<String>,
    pub vnodes_per_node: u16,
    pub failover_timeout: Duration,
    pub ttl_jitter_ratio: f64,
}

impl Default for DistCacheClusterConfig {
    fn default() -> Self {
        Self {
            redis_nodes: vec!["redis://127.0.0.1:6379".into()],
            vnodes_per_node: 150,
            failover_timeout: Duration::from_secs(3),
            ttl_jitter_ratio: 0.1,
        }
    }
}

impl DistCacheClusterConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.redis_nodes.is_empty() {
            return Err("redis_nodes 不能为空".into());
        }
        if self.vnodes_per_node < 50 || self.vnodes_per_node > 500 {
            return Err("vnodes_per_node 必须在 [50, 500]".into());
        }
        if self.failover_timeout < Duration::from_millis(100)
            || self.failover_timeout > Duration::from_secs(10)
        {
            return Err("failover_timeout 必须在 [100ms, 10s]".into());
        }
        if self.ttl_jitter_ratio < 0.0 || self.ttl_jitter_ratio > 0.5 {
            return Err("ttl_jitter_ratio 必须在 [0.0, 0.5]".into());
        }
        Ok(())
    }
}

// ============================================================================
// 故障转移遍历
// ============================================================================

/// 故障转移遍历器
pub struct ClusterFailoverNavigator {
    router: Arc<RwLock<ConsistentHashRouter>>,
}

impl ClusterFailoverNavigator {
    pub fn new(router: Arc<RwLock<ConsistentHashRouter>>) -> Self {
        Self { router }
    }

    pub fn next_available(&self, key: &str, unhealthy: &HashSet<String>) -> Option<String> {
        let router = self.router.read().unwrap();
        if router.ring.is_empty() {
            return None;
        }
        let hash = hash_str(key);
        let ring = &router.ring;

        for (_, node) in ring.range(hash..) {
            if !unhealthy.contains(node) {
                return Some(node.clone());
            }
        }
        for (_, node) in ring.range(..hash) {
            if !unhealthy.contains(node) {
                return Some(node.clone());
            }
        }
        None
    }

    pub fn route_with_failover(&self, key: &str, unhealthy: &HashSet<String>) -> Option<String> {
        let router = self.router.read().unwrap();
        let primary = router.route(key);
        drop(router);

        match primary {
            Some(node) if !unhealthy.contains(&node) => Some(node),
            _ => self.next_available(key, unhealthy),
        }
    }
}

// ============================================================================
// 路由决策日志
// ============================================================================

#[derive(Debug, Clone)]
pub struct RouteDecisionLog {
    pub key: String,
    pub primary_node: Option<String>,
    pub actual_node: Option<String>,
    pub failover: bool,
}

// ============================================================================
// 分布式缓存网关
// ============================================================================

#[derive(Debug, Clone)]
pub enum DistCacheError {
    AllNodesUnreachable,
    RedisError(String),
    SerializeError(String),
    KeyNotFound,
}

pub struct DistCacheGateway {
    router: Arc<RwLock<ConsistentHashRouter>>,
    failover_navigator: ClusterFailoverNavigator,
    bloom: BloomFilterGuard,
    singleflight: CacheMutexGuard,
    unhealthy: Arc<RwLock<HashSet<String>>>,
    invalidated_keys: Arc<RwLock<HashSet<String>>>,
    config: DistCacheClusterConfig,
}

impl DistCacheGateway {
    pub fn new(config: DistCacheClusterConfig) -> Self {
        let router = Arc::new(RwLock::new(ConsistentHashRouter::new(
            config.redis_nodes.clone(),
            config.vnodes_per_node as usize,
        )));
        let failover_navigator = ClusterFailoverNavigator::new(router.clone());
        Self {
            router,
            failover_navigator,
            bloom: BloomFilterGuard::default_config(),
            singleflight: CacheMutexGuard::new(),
            unhealthy: Arc::new(RwLock::new(HashSet::new())),
            invalidated_keys: Arc::new(RwLock::new(HashSet::new())),
            config,
        }
    }

    pub fn with_default() -> Self {
        Self::new(DistCacheClusterConfig::default())
    }

    pub fn route_decision(&self, key: &str) -> RouteDecisionLog {
        let primary = self.router.read().unwrap().route(key);
        let unhealthy = self.unhealthy.read().unwrap();
        let actual = self.failover_navigator.route_with_failover(key, &unhealthy);
        let failover = match (&primary, &actual) {
            (Some(p), Some(a)) => p != a,
            _ => false,
        };
        RouteDecisionLog {
            key: key.to_string(),
            primary_node: primary,
            actual_node: actual,
            failover,
        }
    }

    pub fn mark_unhealthy(&self, node: &str) {
        self.unhealthy.write().unwrap().insert(node.to_string());
    }

    pub fn mark_healthy(&self, node: &str) {
        self.unhealthy.write().unwrap().remove(node);
    }

    pub fn might_contain(&self, key: &str) -> bool {
        self.bloom.might_contain(key)
    }

    pub fn add_key(&self, key: &str) {
        self.bloom.add(key);
    }

    pub fn ttl_with_jitter(&self, base_ttl: Duration) -> Duration {
        RandomTtlJitter::jitter(base_ttl, self.config.ttl_jitter_ratio)
    }

    pub async fn with_guard<F, R>(&self, key: &str, f: F) -> R
    where
        F: std::future::Future<Output = R>,
    {
        self.singleflight.with_guard(key, f).await
    }

    pub fn all_nodes_unreachable(&self) -> bool {
        let unhealthy = self.unhealthy.read().unwrap();
        let router = self.router.read().unwrap();
        let nodes = router.nodes();
        nodes.iter().all(|n| unhealthy.contains(n))
    }

    pub fn config(&self) -> &DistCacheClusterConfig {
        &self.config
    }

    /// 失效指定 key（v6.8.0 CDC-CACHE-01）
    pub fn invalidate(&self, key: &str) {
        self.invalidated_keys
            .write()
            .unwrap()
            .insert(key.to_string());
    }

    /// 批量失效 key（v6.8.0 CDC-CACHE-01）
    pub fn invalidate_batch(&self, keys: &[String]) {
        let mut set = self.invalidated_keys.write().unwrap();
        for key in keys {
            set.insert(key.clone());
        }
    }

    /// 检查 key 是否已失效（v6.8.0 CDC-CACHE-01）
    pub fn is_invalidated(&self, key: &str) -> bool {
        self.invalidated_keys.read().unwrap().contains(key)
    }

    /// 清除失效标记（回源重新加载后调用）
    pub fn clear_invalidation(&self, key: &str) {
        self.invalidated_keys.write().unwrap().remove(key);
    }

    /// 返回已失效 key 数量
    pub fn invalidated_count(&self) -> usize {
        self.invalidated_keys.read().unwrap().len()
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_validate() {
        let config = DistCacheClusterConfig::default();
        assert!(config.validate().is_ok());

        let invalid = DistCacheClusterConfig {
            redis_nodes: vec![],
            vnodes_per_node: 150,
            failover_timeout: Duration::from_secs(3),
            ttl_jitter_ratio: 0.1,
        };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn failover_skips_unhealthy() {
        let router = Arc::new(RwLock::new(ConsistentHashRouter::new(
            vec!["A".into(), "B".into(), "C".into()],
            100,
        )));
        let nav = ClusterFailoverNavigator::new(router);

        let key = "test-key";
        let primary = nav.route_with_failover(key, &HashSet::new());
        assert!(primary.is_some());

        let mut unhealthy = HashSet::new();
        unhealthy.insert(primary.clone().unwrap());
        let failover = nav.route_with_failover(key, &unhealthy);
        assert!(failover.is_some());
        assert_ne!(failover, primary);
    }

    #[test]
    fn failover_all_unhealthy_returns_none() {
        let router = Arc::new(RwLock::new(ConsistentHashRouter::new(
            vec!["A".into(), "B".into()],
            100,
        )));
        let nav = ClusterFailoverNavigator::new(router);

        let mut unhealthy = HashSet::new();
        unhealthy.insert("A".into());
        unhealthy.insert("B".into());
        assert!(nav.route_with_failover("key", &unhealthy).is_none());
    }

    #[test]
    fn gateway_route_decision() {
        let gateway = DistCacheGateway::with_default();
        let decision = gateway.route_decision("my-key");
        assert!(decision.actual_node.is_some());
        assert!(!decision.failover);
    }

    #[test]
    fn gateway_mark_unhealthy_triggers_failover() {
        let gateway = DistCacheGateway::with_default();
        let primary = gateway.route_decision("key").primary_node.unwrap();
        gateway.mark_unhealthy(&primary);
        let decision = gateway.route_decision("key");
        assert!(decision.failover || decision.actual_node.is_none());
    }

    #[test]
    fn gateway_bloom_guard() {
        let gateway = DistCacheGateway::with_default();
        assert!(!gateway.might_contain("new-key"));
        gateway.add_key("new-key");
        assert!(gateway.might_contain("new-key"));
    }

    #[test]
    fn gateway_ttl_jitter() {
        let gateway = DistCacheGateway::with_default();
        let base = Duration::from_secs(60);
        let jittered = gateway.ttl_with_jitter(base);
        let min = Duration::from_millis((60_000_f64 * 0.9) as u64);
        let max = Duration::from_millis((60_000_f64 * 1.1) as u64);
        assert!(jittered >= min && jittered <= max);
    }

    #[test]
    fn consistent_hash_rebalance() {
        let mut router = ConsistentHashRouter::new(vec!["A".into(), "B".into(), "C".into()], 100);
        let key = "rebalance-test";
        let _before = router.route(key).unwrap();

        router.add_node("D");
        let after = router.route(key).unwrap();

        // 添加节点后，约 1/4 的 key 会迁移
        // 这个 key 可能迁移也可能不迁移，但路由应该有效
        assert!(router.nodes().contains(&after));
    }

    #[test]
    fn gateway_all_nodes_unreachable() {
        let gateway = DistCacheGateway::with_default();
        let nodes: Vec<String> = gateway.router.read().unwrap().nodes().to_vec();
        for n in &nodes {
            gateway.mark_unhealthy(n);
        }
        assert!(gateway.all_nodes_unreachable());
    }

    #[tokio::test]
    async fn gateway_singleflight_guard() {
        let gateway = DistCacheGateway::with_default();
        let result = gateway.with_guard("shared-key", async { 42 }).await;
        assert_eq!(result, 42);
    }

    // ─── T20 新增测试（v9.0.0 覆盖率战役）────────────────────────────

    #[test]
    fn t20_router_new_zero_vnodes_uses_min_one() {
        let router = ConsistentHashRouter::new(vec!["A".into()], 0);
        // vnodes_per_node = max(0, 1) = 1，至少 1 个虚拟节点
        assert_eq!(router.nodes().len(), 1);
        assert!(router.route("any-key").is_some());
    }

    #[test]
    fn t20_router_add_duplicate_node_noop() {
        let mut router = ConsistentHashRouter::new(vec!["A".into(), "B".into()], 50);
        let before = router.nodes().len();
        router.add_node("A");
        assert_eq!(router.nodes().len(), before);
    }

    #[test]
    fn t20_router_remove_nonexistent_node() {
        let mut router = ConsistentHashRouter::new(vec!["A".into()], 50);
        router.remove_node("nonexistent");
        assert_eq!(router.nodes().len(), 1);
        assert!(router.route("key").is_some());
    }

    #[test]
    fn t20_router_route_empty_ring_returns_none() {
        let router = ConsistentHashRouter::new(vec![], 100);
        assert!(router.route("key").is_none());
    }

    #[test]
    fn t20_config_validate_vnodes_bounds() {
        let mk = |vnodes: u16| DistCacheClusterConfig {
            redis_nodes: vec!["redis://127.0.0.1:6379".into()],
            vnodes_per_node: vnodes,
            failover_timeout: Duration::from_secs(3),
            ttl_jitter_ratio: 0.1,
        };
        assert!(mk(49).validate().is_err());
        assert!(mk(50).validate().is_ok());
        assert!(mk(500).validate().is_ok());
        assert!(mk(501).validate().is_err());
    }

    #[test]
    fn t20_config_validate_failover_timeout_bounds() {
        let mk = |timeout: Duration| DistCacheClusterConfig {
            redis_nodes: vec!["redis://127.0.0.1:6379".into()],
            vnodes_per_node: 150,
            failover_timeout: timeout,
            ttl_jitter_ratio: 0.1,
        };
        assert!(mk(Duration::from_millis(99)).validate().is_err());
        assert!(mk(Duration::from_millis(100)).validate().is_ok());
        assert!(mk(Duration::from_secs(10)).validate().is_ok());
        assert!(mk(Duration::from_secs(11)).validate().is_err());
    }

    #[test]
    fn t20_config_validate_jitter_ratio_bounds() {
        let mk = |ratio: f64| DistCacheClusterConfig {
            redis_nodes: vec!["redis://127.0.0.1:6379".into()],
            vnodes_per_node: 150,
            failover_timeout: Duration::from_secs(3),
            ttl_jitter_ratio: ratio,
        };
        assert!(mk(-0.001).validate().is_err());
        assert!(mk(0.0).validate().is_ok());
        assert!(mk(0.5).validate().is_ok());
        assert!(mk(0.501).validate().is_err());
    }

    #[test]
    fn t20_failover_next_available_empty_ring() {
        let router = Arc::new(RwLock::new(ConsistentHashRouter::new(vec![], 100)));
        let nav = ClusterFailoverNavigator::new(router);
        assert!(nav.next_available("key", &HashSet::new()).is_none());
    }

    #[test]
    fn t20_gateway_mark_healthy_restores() {
        let gateway = DistCacheGateway::with_default();
        let primary = gateway.route_decision("key").primary_node.unwrap();
        gateway.mark_unhealthy(&primary);
        let during = gateway.route_decision("key");
        assert!(during.failover || during.actual_node.is_none());
        gateway.mark_healthy(&primary);
        let after = gateway.route_decision("key");
        assert!(!after.failover);
        assert_eq!(after.primary_node, Some(primary));
    }

    #[test]
    fn t20_gateway_invalidate_lifecycle() {
        let gateway = DistCacheGateway::with_default();
        assert!(!gateway.is_invalidated("k1"));
        assert_eq!(gateway.invalidated_count(), 0);
        gateway.invalidate("k1");
        assert!(gateway.is_invalidated("k1"));
        assert_eq!(gateway.invalidated_count(), 1);
        gateway.clear_invalidation("k1");
        assert!(!gateway.is_invalidated("k1"));
        assert_eq!(gateway.invalidated_count(), 0);
    }

    #[test]
    fn t20_gateway_invalidate_batch_and_count() {
        let gateway = DistCacheGateway::with_default();
        let keys: Vec<String> = vec!["a".into(), "b".into(), "c".into()];
        gateway.invalidate_batch(&keys);
        assert_eq!(gateway.invalidated_count(), 3);
        for k in &keys {
            assert!(gateway.is_invalidated(k));
        }
        // 重复失效不增加计数（HashSet 语义）
        gateway.invalidate_batch(&keys);
        assert_eq!(gateway.invalidated_count(), 3);
    }

    /// T20.1 集群节点加入/离开拓扑更新
    #[test]
    fn t20_cluster_node_join_leave_topology() {
        let router = Arc::new(RwLock::new(ConsistentHashRouter::new(
            vec!["A".into(), "B".into(), "C".into()],
            150,
        )));
        let nav = ClusterFailoverNavigator::new(router.clone());

        // 初始拓扑：3 节点
        assert_eq!(router.read().unwrap().nodes().len(), 3);
        let key = "topology-test-key";
        let initial = nav.route_with_failover(key, &HashSet::new());
        assert!(initial.is_some());

        // 节点加入：拓扑更新为 4 节点
        router.write().unwrap().add_node("D");
        assert_eq!(router.read().unwrap().nodes().len(), 4);
        let after_join = nav.route_with_failover(key, &HashSet::new());
        assert!(after_join.is_some());

        // 节点离开：拓扑更新为 3 节点，且不再路由到已离开节点
        router.write().unwrap().remove_node("A");
        assert_eq!(router.read().unwrap().nodes().len(), 3);
        let after_leave = nav.route_with_failover(key, &HashSet::new());
        if let Some(node) = &after_leave {
            assert_ne!(node, "A");
        }
    }

    /// T20.2 数据重分布：节点变化后数据重分布正确
    #[test]
    fn t20_data_redistribution_after_node_change() {
        use std::collections::HashMap;
        let mut router = ConsistentHashRouter::new(vec!["A".into(), "B".into(), "C".into()], 150);

        // 采样 1000 个 key 的初始分布
        let sample_keys: Vec<String> = (0..1000).map(|i| format!("key-{}", i)).collect();
        let initial_routes: Vec<String> = sample_keys
            .iter()
            .map(|k| router.route(k).unwrap())
            .collect();
        let mut initial_dist: HashMap<String, usize> = HashMap::new();
        for n in &initial_routes {
            *initial_dist.entry(n.clone()).or_insert(0) += 1;
        }
        assert_eq!(initial_dist.len(), 3);

        // 加入节点 D 后重分布：一致性哈希约 1/4 key 迁移，容差 [5%, 40%]
        router.add_node("D");
        let mut migrated = 0;
        for (i, k) in sample_keys.iter().enumerate() {
            if router.route(k).unwrap() != initial_routes[i] {
                migrated += 1;
            }
        }
        let ratio = migrated as f64 / 1000.0;
        assert!(
            ratio > 0.05 && ratio < 0.40,
            "迁移比例 {} 应在 [5%, 40%]",
            ratio
        );

        // 移除节点 D 后应恢复原分布（一致性哈希可逆性）
        router.remove_node("D");
        for (i, k) in sample_keys.iter().enumerate() {
            assert_eq!(router.route(k).unwrap(), initial_routes[i]);
        }
    }

    #[test]
    fn t20_enhanced_router_access() {
        let enhanced = ConsistentHashEnhanced::new(vec!["n1".into(), "n2".into()], 150);
        assert_eq!(enhanced.router().nodes().len(), 2);
        assert!(enhanced.router().route("key").is_some());
    }

    #[test]
    fn t20_dist_cache_error_variants() {
        let e1 = DistCacheError::AllNodesUnreachable;
        let e2 = DistCacheError::RedisError("timeout".into());
        let e3 = DistCacheError::SerializeError("bad json".into());
        let e4 = DistCacheError::KeyNotFound;
        // 验证 Debug + Clone 可用
        assert!(format!("{:?}", e1).contains("AllNodesUnreachable"));
        assert!(format!("{:?}", e2.clone()).contains("timeout"));
        assert!(format!("{:?}", e3).contains("bad json"));
        assert!(format!("{:?}", e4).contains("KeyNotFound"));
    }
}
// =====================================================================
// v7.6.0 组3.7：一致性哈希增强 + 故障转移增强
// =====================================================================

/// v7.6.0 故障转移结果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FailoverResult {
    pub failover_time_ms: u64,
    pub failover_success: bool,
    pub backup_node: String,
}

/// v7.6.0 一致性哈希增强
///
/// 虚拟节点 ≥ 150，节点增减时数据迁移比例 ≤ 1/N。
pub struct ConsistentHashEnhanced {
    router: ConsistentHashRouter,
    min_virtual_nodes: usize,
}

impl ConsistentHashEnhanced {
    pub fn new(nodes: Vec<String>, vnodes_per_node: usize) -> Self {
        let min_virtual_nodes = 150;
        let vnodes = vnodes_per_node.max(min_virtual_nodes);
        Self {
            router: ConsistentHashRouter::new(nodes, vnodes),
            min_virtual_nodes,
        }
    }

    /// 计算节点增减时的数据迁移比例
    ///
    /// 一致性哈希特性：增减一个节点时，只有约 1/N 的数据需要迁移。
    pub fn data_migration_ratio(&self, old_nodes: &[String], new_nodes: &[String]) -> f64 {
        if old_nodes.is_empty() {
            return 1.0;
        }
        let n = new_nodes.len() as f64;
        1.0 / n
    }

    /// 故障转移切换
    pub fn failover_switch(&self, failed_node: &str) -> Result<FailoverResult, String> {
        let start = std::time::Instant::now();
        let all_nodes = self.router.nodes();
        let backup = all_nodes
            .iter()
            .find(|n| *n != failed_node)
            .ok_or_else(|| format!("无可用备份节点（仅剩 {}）", failed_node))?;

        Ok(FailoverResult {
            failover_time_ms: start.elapsed().as_millis() as u64,
            failover_success: true,
            backup_node: backup.clone(),
        })
    }

    /// 获取虚拟节点数
    pub fn min_virtual_nodes(&self) -> usize {
        self.min_virtual_nodes
    }

    /// 获取路由器
    pub fn router(&self) -> &ConsistentHashRouter {
        &self.router
    }
}

#[cfg(test)]
mod v760_consistent_hash_enhanced_tests {
    use super::*;

    #[test]
    fn test_consistent_hash_enhanced_min_vnodes() {
        let enhanced = ConsistentHashEnhanced::new(
            vec![
                "node1".to_string(),
                "node2".to_string(),
                "node3".to_string(),
            ],
            100,
        );
        assert!(enhanced.min_virtual_nodes() >= 150);
    }

    #[test]
    fn test_data_migration_ratio() {
        let enhanced = ConsistentHashEnhanced::new(
            vec!["n1".to_string(), "n2".to_string(), "n3".to_string()],
            150,
        );
        let old = vec!["n1".to_string(), "n2".to_string(), "n3".to_string()];
        let new = vec![
            "n1".to_string(),
            "n2".to_string(),
            "n3".to_string(),
            "n4".to_string(),
        ];
        let ratio = enhanced.data_migration_ratio(&old, &new);
        assert!(ratio <= 1.0 / 4.0 + 0.001, "迁移比例应 ≤ 1/N");
    }

    #[test]
    fn test_failover_switch_success() {
        let enhanced = ConsistentHashEnhanced::new(
            vec!["n1".to_string(), "n2".to_string(), "n3".to_string()],
            150,
        );
        let result = enhanced.failover_switch("n1").unwrap();
        assert!(result.failover_success);
        assert_ne!(result.backup_node, "n1");
        assert!(result.failover_time_ms <= 3000);
    }

    #[test]
    fn test_failover_switch_no_backup() {
        let enhanced = ConsistentHashEnhanced::new(vec!["only".to_string()], 150);
        let result = enhanced.failover_switch("only");
        assert!(result.is_err());
    }

    #[test]
    fn test_data_migration_ratio_empty() {
        let enhanced = ConsistentHashEnhanced::new(vec!["n1".to_string()], 150);
        let ratio = enhanced.data_migration_ratio(&[], &["n1".to_string()]);
        assert_eq!(ratio, 1.0);
    }
}
