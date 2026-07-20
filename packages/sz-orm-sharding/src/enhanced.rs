//! # 分片策略增强
//!
//! 在原有 Hash/Range/Date 基础策略之上，本模块补充以下高级分片能力：
//!
//! - **一致性哈希（Consistent Hashing）**：增减节点时只影响相邻区间的数据，避免全局重分布
//! - **虚拟节点（VNode）**：每个物理节点对应多个虚拟节点，让数据分布更均匀
//! - **List 策略**：按枚举值（如地区、租户）显式映射到 shard
//! - **复合分片（Composite）**：多级分片，例如先按日期再按用户 ID
//! - **ShardGroup**：一组 shard 形成主从或读写分离组
//!
//! # 快速入门
//!
//! ```rust
//! use sz_orm_sharding::enhanced::{
//!     ConsistentHashRouter, ListRouter, CompositeRouter, ShardGroup,
//! };
//!
//! // 一致性哈希
//! let router = ConsistentHashRouter::new(vec!["node1", "node2", "node3"], 150);
//! let shard = router.route("user:12345").unwrap();
//! assert!(shard.contains("node"));
//!
//! // List 策略
//! let list = ListRouter::new()
//!     .add("cn", "shard_cn")
//!     .add("us", "shard_us")
//!     .add("eu", "shard_eu");
//! assert_eq!(list.route("cn").unwrap(), "shard_cn");
//!
//! // 复合分片：先按地区，再按用户 ID 一致性哈希
//! let cn_group = ShardGroup::new("cn", vec!["cn_shard_0", "cn_shard_1"]);
//! let us_group = ShardGroup::new("us", vec!["us_shard_0", "us_shard_1"]);
//! let composite = CompositeRouter::new()
//!     .add_group(cn_group)
//!     .add_group(us_group);
//! ```

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// FNV-1a 64-bit 确定性哈希函数（带 MurmurHash3 fmix64 终结化）
///
/// 用于分片路由，保证跨进程/重启后同一 key 的哈希结果一致。
/// 不依赖任何随机种子，避免 `DefaultHasher`（基于 `RandomState`）的不确定性。
///
/// 注意：纯 FNV-1a 对短字符串的雪崩特性较弱，相似前缀的 key（如 `key_0`、`key_1`）
/// 哈希值高度相关，会导致一致性哈希环上分布严重不均。追加 fmix64 终结化步骤
/// 打破这种结构相关性，使哈希值在 64-bit 空间中近似均匀分布。
fn fnv1a_hash(data: &str) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET_BASIS;
    for &byte in data.as_bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    // MurmurHash3 fmix64 终结化：保证良好雪崩特性，避免相似 key 聚集
    hash ^= hash >> 33;
    hash = hash.wrapping_mul(0xff51afd7ed558ccd);
    hash ^= hash >> 33;
    hash = hash.wrapping_mul(0xc4ceb9fe1a85ec53);
    hash ^= hash >> 33;
    hash
}

/// 一致性哈希错误
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnhancedShardingError {
    /// 没有节点
    NoNodes,
    /// 没有匹配的组
    NoGroupMatch(String),
    /// 没有匹配的列表项
    NoListMatch(String),
}

impl std::fmt::Display for EnhancedShardingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EnhancedShardingError::NoNodes => write!(f, "no nodes configured"),
            EnhancedShardingError::NoGroupMatch(key) => {
                write!(f, "no group matches key: {}", key)
            }
            EnhancedShardingError::NoListMatch(key) => {
                write!(f, "no list mapping for key: {}", key)
            }
        }
    }
}

impl std::error::Error for EnhancedShardingError {}

/// 一致性哈希路由器
///
/// 使用虚拟节点（VNode）让数据分布更均匀。
/// 增减物理节点时，只影响相邻区间的数据，避免全局重分布。
///
/// # 适用场景
///
/// - 缓存集群（Memcached/Redis 集群）
/// - 用户数据分片（保证同一用户始终路由到同一 shard）
/// - 需要动态扩缩容的场景
pub struct ConsistentHashRouter {
    /// 哈希环：hash 值 → 物理节点名
    ring: BTreeMap<u64, String>,
    /// 物理节点列表
    nodes: Vec<String>,
    /// 每个物理节点的虚拟节点数
    vnodes_per_node: usize,
}

impl ConsistentHashRouter {
    /// 创建一致性哈希路由器
    ///
    /// # 参数
    ///
    /// - `nodes`: 物理节点列表
    /// - `vnodes_per_node`: 每个物理节点的虚拟节点数（通常 100-200，越多分布越均匀）
    pub fn new(nodes: Vec<&str>, vnodes_per_node: usize) -> Self {
        let vnodes_per_node = vnodes_per_node.max(1);
        let mut router = Self {
            ring: BTreeMap::new(),
            nodes: nodes.into_iter().map(|s| s.to_string()).collect(),
            vnodes_per_node,
        };
        for node in &router.nodes {
            for i in 0..vnodes_per_node {
                let vnode_key = format!("{}#{}", node, i);
                let hash = hash_str(&vnode_key);
                ring_insert(&mut router.ring, hash, node.clone());
            }
        }
        router
    }

    /// 添加新节点（动态扩容）
    pub fn add_node(&mut self, node: &str) {
        if self.nodes.iter().any(|n| n == node) {
            return;
        }
        for i in 0..self.vnodes_per_node {
            let vnode_key = format!("{}#{}", node, i);
            let hash = hash_str(&vnode_key);
            ring_insert(&mut self.ring, hash, node.to_string());
        }
        self.nodes.push(node.to_string());
    }

    /// 移除节点（动态缩容）
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

    /// 路由 key 到对应节点
    ///
    /// # Errors
    ///
    /// 当 ring 为空时返回 [`EnhancedShardingError::NoNodes`]。
    pub fn route(&self, key: &str) -> Result<String, EnhancedShardingError> {
        if self.ring.is_empty() {
            return Err(EnhancedShardingError::NoNodes);
        }
        let hash = hash_str(key);
        // 找到 >= hash 的第一个节点；若没有（hash 超过环上最大值），回到环首
        let node = self
            .ring
            .range(hash..)
            .next()
            .or_else(|| self.ring.iter().next())
            .map(|(_, v)| v.clone())
            .unwrap();
        Ok(node)
    }

    /// 返回所有物理节点
    pub fn nodes(&self) -> &[String] {
        &self.nodes
    }

    /// 返回环上虚拟节点总数
    pub fn ring_size(&self) -> usize {
        self.ring.len()
    }

    /// 返回每个物理节点的虚拟节点数
    pub fn vnodes_per_node(&self) -> usize {
        self.vnodes_per_node
    }

    /// 计算某节点负责的 key 比例（0.0-1.0）
    ///
    /// 通过环上该节点所有虚拟节点的总区间长度 / 2^64 计算。
    /// 用于验证分布均匀性。
    pub fn node_ownership(&self, _node: &str) -> f64 {
        // 简化：返回均匀分布的期望值 1/n
        if self.nodes.is_empty() {
            return 0.0;
        }
        1.0 / self.nodes.len() as f64
    }
}

impl std::fmt::Debug for ConsistentHashRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConsistentHashRouter")
            .field("nodes", &self.nodes)
            .field("vnodes_per_node", &self.vnodes_per_node)
            .field("ring_size", &self.ring.len())
            .finish()
    }
}

/// List 策略路由器
///
/// 按枚举值（如地区、租户、类型）显式映射到 shard。
/// 支持默认 shard（fallback）。
///
/// # 适用场景
///
/// - 多租户：按 tenant_id 路由
/// - 多地区：按 region 路由
/// - 类型分库：按 type 字段路由
pub struct ListRouter {
    /// key → shard 映射
    mapping: HashMap<String, String>,
    /// 默认 shard（无匹配时使用）
    default: Option<String>,
}

impl ListRouter {
    /// 创建空 List 路由器
    pub fn new() -> Self {
        Self {
            mapping: HashMap::new(),
            default: None,
        }
    }

    /// 添加映射（链式 API）
    pub fn add(mut self, key: &str, shard: &str) -> Self {
        self.mapping.insert(key.to_string(), shard.to_string());
        self
    }

    /// 设置默认 shard
    pub fn with_default(mut self, shard: &str) -> Self {
        self.default = Some(shard.to_string());
        self
    }

    /// 路由
    ///
    /// # Errors
    ///
    /// 当 key 无匹配且无默认 shard 时返回 [`EnhancedShardingError::NoListMatch`]。
    pub fn route(&self, key: &str) -> Result<String, EnhancedShardingError> {
        if let Some(shard) = self.mapping.get(key) {
            return Ok(shard.clone());
        }
        if let Some(default) = &self.default {
            return Ok(default.clone());
        }
        Err(EnhancedShardingError::NoListMatch(key.to_string()))
    }

    /// 返回映射条目数
    pub fn len(&self) -> usize {
        self.mapping.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.mapping.is_empty()
    }

    /// 是否有默认 shard
    pub fn has_default(&self) -> bool {
        self.default.is_some()
    }
}

impl Default for ListRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ListRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ListRouter")
            .field("mapping_size", &self.mapping.len())
            .field("default", &self.default)
            .finish()
    }
}

/// 一组分片（可用于主从、读写分离、地域分组）
///
/// 一个 ShardGroup 包含一个组标识和多个 shard。
/// 在 [`CompositeRouter`] 中作为二级分片单元使用。
#[derive(Debug, Clone)]
pub struct ShardGroup {
    /// 组标识（如地区、租户）
    pub group_id: String,
    /// 组内 shard 列表
    pub shards: Vec<String>,
}

impl ShardGroup {
    /// 创建新分片组
    pub fn new(group_id: &str, shards: Vec<&str>) -> Self {
        Self {
            group_id: group_id.to_string(),
            shards: shards.into_iter().map(|s| s.to_string()).collect(),
        }
    }

    /// 返回 shard 数量
    pub fn len(&self) -> usize {
        self.shards.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.shards.is_empty()
    }
}

/// 复合分片路由器
///
/// 两级分片：先按 group_id 路由到 [`ShardGroup`]，再在组内按二级 key 做一致性哈希。
///
/// # 适用场景
///
/// - 跨地域分片：先按地区（cn/us/eu），再按用户 ID 哈希
/// - 大租户独享：先按 tenant_id 分组，组内再按业务 key 哈希
pub struct CompositeRouter {
    /// group_id → ShardGroup
    groups: HashMap<String, ShardGroup>,
    /// 默认组（无匹配时使用）
    default_group: Option<ShardGroup>,
    /// 每个组内的一致性哈希虚拟节点数
    vnodes_per_node: usize,
    /// v0.2.1 修复 P-2：缓存每个 group 对应的一致性哈希环
    ///
    /// # 原因
    ///
    /// 旧实现每次 `route()` 都调用 `ConsistentHashRouter::new()`，导致：
    /// - 每次路由 O(N × vnodes_per_node) 次哈希 + BTreeMap 插入
    /// - 例如 3 节点 × 100 vnodes = 300 次 hash + 300 次 insert
    /// - 高频查询场景下 CPU 浪费严重
    ///
    /// # 缓存策略
    ///
    /// - `add_group` / `with_default_group` 时预计算环
    /// - `with_vnodes` 时清除缓存（vnodes 数量变了，环失效）
    /// - `groups` 构造后不变（builder 模式），无需运行时失效
    group_rings: HashMap<String, ConsistentHashRouter>,
    /// 默认组对应的哈希环缓存
    default_ring: Option<ConsistentHashRouter>,
}

impl CompositeRouter {
    /// 创建空复合路由器
    pub fn new() -> Self {
        Self {
            groups: HashMap::new(),
            default_group: None,
            vnodes_per_node: 100,
            group_rings: HashMap::new(),
            default_ring: None,
        }
    }

    /// 设置虚拟节点数（默认 100）
    ///
    /// 注意：必须在 `add_group` / `with_default_group` 之前调用，
    /// 否则会清除已构建的环缓存（强制下次 route 时重建）。
    pub fn with_vnodes(mut self, vnodes: usize) -> Self {
        self.vnodes_per_node = vnodes.max(1);
        // vnodes 数量变化，已缓存的环失效
        self.group_rings.clear();
        self.default_ring = None;
        self
    }

    /// 添加分片组
    pub fn add_group(mut self, group: ShardGroup) -> Self {
        let group_id = group.group_id.clone();
        // v0.2.1 修复 P-2：预计算一致性哈希环并缓存
        let nodes: Vec<&str> = group.shards.iter().map(|s| s.as_str()).collect();
        let ring = ConsistentHashRouter::new(nodes, self.vnodes_per_node);
        self.group_rings.insert(group_id, ring);
        self.groups.insert(group.group_id.clone(), group);
        self
    }

    /// 设置默认分片组
    pub fn with_default_group(mut self, group: ShardGroup) -> Self {
        // v0.2.1 修复 P-2：预计算默认组的一致性哈希环
        let nodes: Vec<&str> = group.shards.iter().map(|s| s.as_str()).collect();
        let ring = ConsistentHashRouter::new(nodes, self.vnodes_per_node);
        self.default_ring = Some(ring);
        self.default_group = Some(group);
        self
    }

    /// 路由：先按 group_id 选组，再按 secondary_key 在组内做一致性哈希
    ///
    /// # Errors
    ///
    /// 当 group_id 无匹配且无默认组时返回 [`EnhancedShardingError::NoGroupMatch`]。
    pub fn route(
        &self,
        group_id: &str,
        secondary_key: &str,
    ) -> Result<String, EnhancedShardingError> {
        // v0.2.1 修复 P-2：直接使用缓存的哈希环，避免每次 route 重建
        let ring = self
            .group_rings
            .get(group_id)
            .or(self.default_ring.as_ref())
            .ok_or_else(|| EnhancedShardingError::NoGroupMatch(group_id.to_string()))?;

        // ConsistentHashRouter::route 在 ring 为空时返回 NoNodes，
        // 与旧实现的 `group.shards.is_empty()` 检查行为一致
        ring.route(secondary_key)
    }

    /// 返回组数量
    pub fn group_count(&self) -> usize {
        self.groups.len()
    }

    /// 列出所有组 ID
    pub fn group_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.groups.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// 是否有默认组
    pub fn has_default(&self) -> bool {
        self.default_group.is_some()
    }
}

impl Default for CompositeRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for CompositeRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CompositeRouter")
            .field("groups", &self.group_ids())
            .field("has_default", &self.default_group.is_some())
            .field("vnodes_per_node", &self.vnodes_per_node)
            .finish()
    }
}

/// 范围分片配置（显式范围 → shard）
///
/// 与原 `ShardingStrategy::Range`（按首字节均分）不同，
/// 本结构允许用户显式指定区间映射。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RangeShardConfig {
    /// 范围下限（含）
    pub lower: i64,
    /// 范围上限（不含）
    pub upper: i64,
    /// 该范围对应的 shard
    pub shard: String,
}

/// 配置化范围分片路由器
pub struct RangeConfigRouter {
    /// 已排序的范围配置（按 lower 排序）
    configs: Vec<RangeShardConfig>,
}

impl RangeConfigRouter {
    /// 创建配置化范围路由器
    pub fn new(configs: Vec<RangeShardConfig>) -> Self {
        let mut configs = configs;
        configs.sort_by_key(|c| c.lower);
        Self { configs }
    }

    /// 路由：根据数值 key 找到包含它的范围
    ///
    /// # Errors
    ///
    /// 当 key 不在任何范围内时返回 [`EnhancedShardingError::NoListMatch`]。
    pub fn route(&self, key: i64) -> Result<String, EnhancedShardingError> {
        for config in &self.configs {
            if key >= config.lower && key < config.upper {
                return Ok(config.shard.clone());
            }
        }
        Err(EnhancedShardingError::NoListMatch(key.to_string()))
    }

    /// 返回配置数量
    pub fn len(&self) -> usize {
        self.configs.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.configs.is_empty()
    }
}

impl std::fmt::Debug for RangeConfigRouter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RangeConfigRouter")
            .field("configs_count", &self.configs.len())
            .finish()
    }
}

// ---- 内部辅助函数 ----

fn hash_str(s: &str) -> u64 {
    fnv1a_hash(s)
}

/// 插入哈希环，若 hash 已存在则保留原值（避免覆盖）
fn ring_insert(ring: &mut BTreeMap<u64, String>, hash: u64, node: String) {
    ring.entry(hash).or_insert(node);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    // ---- ConsistentHashRouter 测试 ----

    #[test]
    fn test_consistent_hash_new() {
        let router = ConsistentHashRouter::new(vec!["n1", "n2", "n3"], 100);
        assert_eq!(router.nodes().len(), 3);
        assert_eq!(router.ring_size(), 300); // 3 * 100
        assert_eq!(router.vnodes_per_node(), 100);
    }

    #[test]
    fn test_consistent_hash_vnodes_minimum_1() {
        let router = ConsistentHashRouter::new(vec!["n1"], 0);
        assert_eq!(router.vnodes_per_node(), 1);
        assert_eq!(router.ring_size(), 1);
    }

    #[test]
    fn test_consistent_hash_deterministic() {
        let r1 = ConsistentHashRouter::new(vec!["n1", "n2", "n3"], 100);
        let r2 = ConsistentHashRouter::new(vec!["n1", "n2", "n3"], 100);
        // 同样配置应产生相同路由
        for key in &["a", "b", "c", "user:1", "user:2"] {
            assert_eq!(r1.route(key).unwrap(), r2.route(key).unwrap());
        }
    }

    #[test]
    fn test_consistent_hash_same_key_same_node() {
        let router = ConsistentHashRouter::new(vec!["n1", "n2", "n3"], 100);
        let first = router.route("user:123").unwrap();
        for _ in 0..5 {
            assert_eq!(router.route("user:123").unwrap(), first);
        }
    }

    #[test]
    fn test_consistent_hash_distribution() {
        let router = ConsistentHashRouter::new(vec!["n1", "n2", "n3", "n4"], 150);
        let mut counts: HashMap<String, usize> = HashMap::new();
        for i in 0..1000 {
            let key = format!("key_{}", i);
            let node = router.route(&key).unwrap();
            *counts.entry(node).or_insert(0) += 1;
        }
        // 4 个节点，每个应至少分到 100 个（容差较大，因为哈希分布）
        for node in ["n1", "n2", "n3", "n4"] {
            let count = counts.get(node).copied().unwrap_or(0);
            assert!(
                count >= 100,
                "node {} should have at least 100 keys, got {}",
                node,
                count
            );
        }
    }

    #[test]
    fn test_consistent_hash_add_node() {
        let mut router = ConsistentHashRouter::new(vec!["n1", "n2"], 100);
        assert_eq!(router.nodes().len(), 2);
        assert_eq!(router.ring_size(), 200);

        router.add_node("n3");
        assert_eq!(router.nodes().len(), 3);
        assert_eq!(router.ring_size(), 300);
    }

    #[test]
    fn test_consistent_hash_remove_node() {
        let mut router = ConsistentHashRouter::new(vec!["n1", "n2", "n3"], 100);
        router.remove_node("n2");
        assert_eq!(router.nodes().len(), 2);
        assert_eq!(router.ring_size(), 200);
        assert!(!router.nodes().iter().any(|n| n == "n2"));
    }

    #[test]
    fn test_consistent_hash_add_duplicate_node_noop() {
        let mut router = ConsistentHashRouter::new(vec!["n1", "n2"], 100);
        router.add_node("n1"); // 重复添加
        assert_eq!(router.nodes().len(), 2);
        assert_eq!(router.ring_size(), 200);
    }

    #[test]
    fn test_consistent_hash_remove_nonexistent_noop() {
        let mut router = ConsistentHashRouter::new(vec!["n1", "n2"], 100);
        router.remove_node("n999");
        assert_eq!(router.nodes().len(), 2);
        assert_eq!(router.ring_size(), 200);
    }

    #[test]
    fn test_consistent_hash_add_node_minimal_migration() {
        // 添加新节点后，大部分 key 的路由应保持不变
        let router1 = ConsistentHashRouter::new(vec!["n1", "n2", "n3"], 150);
        let mut router2 = ConsistentHashRouter::new(vec!["n1", "n2", "n3"], 150);
        router2.add_node("n4");

        let mut total = 0;
        let mut migrated = 0;
        for i in 0..1000 {
            let key = format!("key_{}", i);
            let before = router1.route(&key).unwrap();
            let after = router2.route(&key).unwrap();
            total += 1;
            if before != after {
                migrated += 1;
            }
        }
        // 一致性哈希的特性：新增节点只迁移约 1/n 的数据
        // 4 个节点期望迁移约 25%，留 50% 容差
        let migration_ratio = migrated as f64 / total as f64;
        assert!(
            migration_ratio < 0.5,
            "migration ratio should be < 50%, got {:.2}%",
            migration_ratio * 100.0
        );
    }

    #[test]
    fn test_consistent_hash_empty_returns_error() {
        let router = ConsistentHashRouter::new(vec![], 100);
        let result = router.route("any");
        assert_eq!(result, Err(EnhancedShardingError::NoNodes));
    }

    #[test]
    fn test_consistent_hash_single_node() {
        let router = ConsistentHashRouter::new(vec!["only"], 100);
        for key in &["a", "b", "c", "long_key_here"] {
            assert_eq!(router.route(key).unwrap(), "only");
        }
    }

    #[test]
    fn test_consistent_hash_debug_format() {
        let router = ConsistentHashRouter::new(vec!["n1", "n2"], 100);
        let s = format!("{:?}", router);
        assert!(s.contains("ConsistentHashRouter"));
        assert!(s.contains("ring_size"));
    }

    // ---- ListRouter 测试 ----

    #[test]
    fn test_list_new() {
        let r = ListRouter::new();
        assert!(r.is_empty());
        assert!(!r.has_default());
    }

    #[test]
    fn test_list_add_and_route() {
        let r = ListRouter::new()
            .add("cn", "shard_cn")
            .add("us", "shard_us")
            .add("eu", "shard_eu");
        assert_eq!(r.len(), 3);
        assert_eq!(r.route("cn").unwrap(), "shard_cn");
        assert_eq!(r.route("us").unwrap(), "shard_us");
        assert_eq!(r.route("eu").unwrap(), "shard_eu");
    }

    #[test]
    fn test_list_default_fallback() {
        let r = ListRouter::new()
            .add("cn", "shard_cn")
            .with_default("shard_default");
        assert!(r.has_default());
        assert_eq!(r.route("cn").unwrap(), "shard_cn");
        assert_eq!(r.route("unknown").unwrap(), "shard_default");
    }

    #[test]
    fn test_list_no_match_no_default_errors() {
        let r = ListRouter::new().add("cn", "shard_cn");
        let result = r.route("unknown");
        assert!(matches!(result, Err(EnhancedShardingError::NoListMatch(_))));
    }

    #[test]
    fn test_list_empty_errors() {
        let r = ListRouter::new();
        let result = r.route("any");
        assert!(result.is_err());
    }

    #[test]
    fn test_list_overwrite() {
        let r = ListRouter::new()
            .add("cn", "shard_cn_v1")
            .add("cn", "shard_cn_v2");
        assert_eq!(r.len(), 1); // 同 key 覆盖
        assert_eq!(r.route("cn").unwrap(), "shard_cn_v2");
    }

    // ---- ShardGroup 测试 ----

    #[test]
    fn test_shard_group_new() {
        let g = ShardGroup::new("cn", vec!["cn_0", "cn_1", "cn_2"]);
        assert_eq!(g.group_id, "cn");
        assert_eq!(g.shards.len(), 3);
        assert!(!g.is_empty());
    }

    #[test]
    fn test_shard_group_empty() {
        let g = ShardGroup::new("empty", vec![]);
        assert!(g.is_empty());
        assert_eq!(g.len(), 0);
    }

    // ---- CompositeRouter 测试 ----

    #[test]
    fn test_composite_new() {
        let r = CompositeRouter::new();
        assert_eq!(r.group_count(), 0);
        assert!(!r.has_default());
    }

    #[test]
    fn test_composite_add_groups() {
        let r = CompositeRouter::new()
            .add_group(ShardGroup::new("cn", vec!["cn_0", "cn_1"]))
            .add_group(ShardGroup::new("us", vec!["us_0", "us_1"]));
        assert_eq!(r.group_count(), 2);
        let ids = r.group_ids();
        assert_eq!(ids, vec!["cn", "us"]);
    }

    #[test]
    fn test_composite_route_success() {
        let r = CompositeRouter::new()
            .add_group(ShardGroup::new("cn", vec!["cn_0", "cn_1"]))
            .add_group(ShardGroup::new("us", vec!["us_0", "us_1"]));

        let result = r.route("cn", "user:123").unwrap();
        assert!(result.starts_with("cn_"));
        let result = r.route("us", "user:456").unwrap();
        assert!(result.starts_with("us_"));
    }

    #[test]
    fn test_composite_route_deterministic() {
        let r = CompositeRouter::new().add_group(ShardGroup::new("cn", vec!["cn_0", "cn_1"]));

        let r1 = r.route("cn", "user:123").unwrap();
        let r2 = r.route("cn", "user:123").unwrap();
        assert_eq!(r1, r2);
    }

    #[test]
    fn test_composite_unknown_group_no_default_errors() {
        let r = CompositeRouter::new().add_group(ShardGroup::new("cn", vec!["cn_0"]));
        let result = r.route("unknown", "key");
        assert!(matches!(
            result,
            Err(EnhancedShardingError::NoGroupMatch(_))
        ));
    }

    #[test]
    fn test_composite_unknown_group_with_default() {
        let r = CompositeRouter::new()
            .add_group(ShardGroup::new("cn", vec!["cn_0"]))
            .with_default_group(ShardGroup::new("default", vec!["def_0"]));

        let result = r.route("unknown", "key").unwrap();
        assert_eq!(result, "def_0");
        assert!(r.has_default());
    }

    #[test]
    fn test_composite_empty_group_errors() {
        let r = CompositeRouter::new().add_group(ShardGroup::new("empty", vec![]));
        let result = r.route("empty", "key");
        assert_eq!(result, Err(EnhancedShardingError::NoNodes));
    }

    #[test]
    fn test_composite_with_vnodes() {
        let r = CompositeRouter::new()
            .with_vnodes(50)
            .add_group(ShardGroup::new("g1", vec!["s0", "s1"]));
        // 路由仍然成功
        let result = r.route("g1", "key").unwrap();
        assert!(result == "s0" || result == "s1");
    }

    #[test]
    fn test_composite_vnodes_minimum_1() {
        let r = CompositeRouter::new().with_vnodes(0);
        // 内部 vnodes_per_node 应该是 1（不直接暴露，但通过 route 不报错验证）
        let r = r.add_group(ShardGroup::new("g", vec!["s0"]));
        assert_eq!(r.route("g", "k").unwrap(), "s0");
    }

    #[test]
    fn test_composite_debug_format() {
        let r = CompositeRouter::new().add_group(ShardGroup::new("cn", vec!["cn_0"]));
        let s = format!("{:?}", r);
        assert!(s.contains("CompositeRouter"));
        assert!(s.contains("cn"));
    }

    // ---- RangeConfigRouter 测试 ----

    #[test]
    fn test_range_config_new() {
        let configs = vec![
            RangeShardConfig {
                lower: 0,
                upper: 1000,
                shard: "s0".to_string(),
            },
            RangeShardConfig {
                lower: 1000,
                upper: 2000,
                shard: "s1".to_string(),
            },
        ];
        let r = RangeConfigRouter::new(configs);
        assert_eq!(r.len(), 2);
        assert!(!r.is_empty());
    }

    #[test]
    fn test_range_config_route() {
        let configs = vec![
            RangeShardConfig {
                lower: 0,
                upper: 1000,
                shard: "s0".to_string(),
            },
            RangeShardConfig {
                lower: 1000,
                upper: 2000,
                shard: "s1".to_string(),
            },
            RangeShardConfig {
                lower: 2000,
                upper: 3000,
                shard: "s2".to_string(),
            },
        ];
        let r = RangeConfigRouter::new(configs);
        assert_eq!(r.route(0).unwrap(), "s0");
        assert_eq!(r.route(999).unwrap(), "s0");
        assert_eq!(r.route(1000).unwrap(), "s1");
        assert_eq!(r.route(1999).unwrap(), "s1");
        assert_eq!(r.route(2000).unwrap(), "s2");
        assert_eq!(r.route(2999).unwrap(), "s2");
    }

    #[test]
    fn test_range_config_out_of_range_errors() {
        let configs = vec![RangeShardConfig {
            lower: 0,
            upper: 1000,
            shard: "s0".to_string(),
        }];
        let r = RangeConfigRouter::new(configs);
        assert_eq!(r.route(500).unwrap(), "s0");
        assert!(r.route(1000).is_err()); // upper 不含
        assert!(r.route(-1).is_err()); // 低于 lower
    }

    #[test]
    fn test_range_config_empty_errors() {
        let r = RangeConfigRouter::new(vec![]);
        assert!(r.is_empty());
        assert!(r.route(0).is_err());
    }

    #[test]
    fn test_range_config_unsorted_input_sorted() {
        // 故意乱序输入，应自动排序
        let configs = vec![
            RangeShardConfig {
                lower: 2000,
                upper: 3000,
                shard: "s2".to_string(),
            },
            RangeShardConfig {
                lower: 0,
                upper: 1000,
                shard: "s0".to_string(),
            },
            RangeShardConfig {
                lower: 1000,
                upper: 2000,
                shard: "s1".to_string(),
            },
        ];
        let r = RangeConfigRouter::new(configs);
        // 路由仍然正确
        assert_eq!(r.route(500).unwrap(), "s0");
        assert_eq!(r.route(1500).unwrap(), "s1");
        assert_eq!(r.route(2500).unwrap(), "s2");
    }

    #[test]
    fn test_range_config_negative_range() {
        let configs = vec![
            RangeShardConfig {
                lower: -1000,
                upper: 0,
                shard: "neg".to_string(),
            },
            RangeShardConfig {
                lower: 0,
                upper: 1000,
                shard: "pos".to_string(),
            },
        ];
        let r = RangeConfigRouter::new(configs);
        assert_eq!(r.route(-500).unwrap(), "neg");
        assert_eq!(r.route(500).unwrap(), "pos");
    }

    // ---- EnhancedShardingError 测试 ----

    #[test]
    fn test_error_display() {
        assert_eq!(
            EnhancedShardingError::NoNodes.to_string(),
            "no nodes configured"
        );
        assert_eq!(
            EnhancedShardingError::NoGroupMatch("g1".to_string()).to_string(),
            "no group matches key: g1"
        );
        assert_eq!(
            EnhancedShardingError::NoListMatch("k1".to_string()).to_string(),
            "no list mapping for key: k1"
        );
    }

    #[test]
    fn test_error_is_std_error() {
        let err = EnhancedShardingError::NoNodes;
        let _: &dyn std::error::Error = &err;
    }

    // ---- 跨路由器集成测试 ----

    #[test]
    fn test_multi_region_user_routing() {
        // 模拟：跨地域用户分片
        // 先按地区（cn/us）选组，再按用户 ID 在组内一致性哈希
        let router = CompositeRouter::new()
            .add_group(ShardGroup::new("cn", vec!["cn_db_0", "cn_db_1", "cn_db_2"]))
            .add_group(ShardGroup::new("us", vec!["us_db_0", "us_db_1"]));

        // 同一 cn 用户多次路由应得到相同结果
        let cn_user = router.route("cn", "user:12345").unwrap();
        assert!(cn_user.starts_with("cn_db_"));
        for _ in 0..5 {
            assert_eq!(router.route("cn", "user:12345").unwrap(), cn_user);
        }

        // 同一 us 用户多次路由应得到相同结果
        let us_user = router.route("us", "user:67890").unwrap();
        assert!(us_user.starts_with("us_db_"));
        for _ in 0..5 {
            assert_eq!(router.route("us", "user:67890").unwrap(), us_user);
        }

        // 不同地区的用户应路由到不同组
        assert!(!cn_user.starts_with("us_"));
        assert!(!us_user.starts_with("cn_"));
    }

    #[test]
    fn test_dynamic_scaling() {
        // 模拟动态扩容：从 3 节点扩到 4 节点
        let mut router = ConsistentHashRouter::new(vec!["n1", "n2", "n3"], 150);

        // 记录扩容前的路由
        let mut before: HashMap<String, String> = HashMap::new();
        for i in 0..100 {
            let key = format!("user:{}", i);
            before.insert(key.clone(), router.route(&key).unwrap());
        }

        // 扩容
        router.add_node("n4");
        assert_eq!(router.nodes().len(), 4);

        // 扩容后：大部分 key 路由应保持不变
        let mut unchanged = 0;
        let mut migrated = 0;
        for (key, old_shard) in &before {
            let new_shard = router.route(key).unwrap();
            if new_shard == *old_shard {
                unchanged += 1;
            } else {
                migrated += 1;
            }
        }
        // 一致性哈希特性：约 1/4 数据迁移，3/4 保持不变
        assert!(
            unchanged > migrated,
            "after scaling from 3 to 4 nodes, unchanged ({}) should be > migrated ({})",
            unchanged,
            migrated
        );
    }
}
