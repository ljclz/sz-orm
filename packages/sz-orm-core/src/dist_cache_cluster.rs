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
        let min = Duration::from_millis((60_000 as f64 * 0.9) as u64);
        let max = Duration::from_millis((60_000 as f64 * 1.1) as u64);
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
}
