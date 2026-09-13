//! 区域拓扑管理器
//!
//! 管理多区域拓扑声明、健康状态、数据归属与容灾优先级。

use std::collections::HashMap;
use std::time::Duration;

use parking_lot::RwLock;

/// 区域角色
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionRole {
    /// 主区域
    Primary,
    /// 从区域
    Secondary,
    /// 边缘节点
    Edge,
}

/// 数据归属策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataAffinityPolicy {
    /// 静态归属
    Static,
    /// 基于延迟
    LatencyBased,
    /// 基于请求
    RequestBased,
}

/// 复制模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplicationMode {
    /// 同步复制
    Sync,
    /// 异步复制
    Async,
    /// 半同步
    SemiSync,
}

/// 区域健康状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionHealth {
    /// 健康
    Healthy,
    /// 降级
    Degraded,
    /// 不可用
    Unavailable,
}

/// 区域节点
#[derive(Debug, Clone)]
pub struct RegionNode {
    /// 区域 ID（1-64 字符，小写字母/数字/连字符）
    pub region_id: String,
    /// 区域角色
    pub role: RegionRole,
    /// 容灾优先级（1-100，越小越优先）
    pub failover_priority: u8,
    /// 数据归属策略
    pub data_affinity: DataAffinityPolicy,
    /// 复制模式
    pub replication_mode: ReplicationMode,
    /// 复制延迟阈值
    pub replication_lag_threshold: Duration,
    /// 数据源连接串
    pub dsn: String,
}

/// 拓扑错误
#[derive(Debug, Clone)]
pub enum TopologyError {
    /// 区域 ID 重复
    DuplicateRegionId(String),
    /// 区域 ID 无效
    InvalidRegionId(String),
    /// 缺失 Primary 区域
    MissingPrimary,
    /// 容灾优先级重复
    DuplicatePriority(u8),
    /// 容灾优先级超出范围
    PriorityOutOfRange(u8),
}

impl std::fmt::Display for TopologyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TopologyError::DuplicateRegionId(id) => write!(f, "Duplicate region id: {}", id),
            TopologyError::InvalidRegionId(id) => write!(f, "Invalid region id: {}", id),
            TopologyError::MissingPrimary => write!(f, "Missing primary region"),
            TopologyError::DuplicatePriority(p) => write!(f, "Duplicate priority: {}", p),
            TopologyError::PriorityOutOfRange(p) => write!(f, "Priority out of range: {}", p),
        }
    }
}

impl std::error::Error for TopologyError {}

/// 区域健康视图
#[derive(Debug, Clone, Default)]
pub struct MultiRegionHealthView {
    health: HashMap<String, RegionHealth>,
}

impl MultiRegionHealthView {
    /// 创建空视图
    pub fn new() -> Self {
        Self {
            health: HashMap::new(),
        }
    }

    /// 获取区域健康状态
    pub fn get(&self, region_id: &str) -> Option<RegionHealth> {
        self.health.get(region_id).copied()
    }

    /// 设置区域健康状态
    pub fn set(&mut self, region_id: &str, status: RegionHealth) {
        self.health.insert(region_id.to_string(), status);
    }

    /// 是否所有区域健康
    pub fn all_healthy(&self) -> bool {
        self.health.values().all(|h| *h == RegionHealth::Healthy)
    }
}

/// 区域拓扑管理器
pub struct RegionTopology {
    regions: HashMap<String, RegionNode>,
    health: RwLock<MultiRegionHealthView>,
}

impl RegionTopology {
    /// 声明区域拓扑
    ///
    /// 校验：区域 ID 全局唯一且合法、至少一个 Primary、容灾优先级 1-100 且不重复。
    pub fn declare(regions: Vec<RegionNode>) -> Result<Self, TopologyError> {
        let mut map = HashMap::new();
        let mut priorities = Vec::new();
        let mut has_primary = false;

        for node in regions {
            if !is_valid_region_id(&node.region_id) {
                return Err(TopologyError::InvalidRegionId(node.region_id.clone()));
            }
            if map.contains_key(&node.region_id) {
                return Err(TopologyError::DuplicateRegionId(node.region_id.clone()));
            }
            if node.failover_priority < 1 || node.failover_priority > 100 {
                return Err(TopologyError::PriorityOutOfRange(node.failover_priority));
            }
            if priorities.contains(&node.failover_priority) {
                return Err(TopologyError::DuplicatePriority(node.failover_priority));
            }
            if node.role == RegionRole::Primary {
                has_primary = true;
            }
            priorities.push(node.failover_priority);
            map.insert(node.region_id.clone(), node);
        }

        if !has_primary {
            return Err(TopologyError::MissingPrimary);
        }

        let mut health = MultiRegionHealthView::new();
        for id in map.keys() {
            health.set(id, RegionHealth::Healthy);
        }

        Ok(Self {
            regions: map,
            health: RwLock::new(health),
        })
    }

    /// 查找数据归属区域
    pub fn find_affinity_region(&self, data_key: &str) -> Option<&RegionNode> {
        let hash = simple_hash(data_key);
        let ids: Vec<&String> = self.regions.keys().collect();
        if ids.is_empty() {
            return None;
        }
        let idx = (hash as usize) % ids.len();
        self.regions.get(ids[idx])
    }

    /// 按容灾优先级排序返回候选区域
    pub fn candidates_by_priority(&self) -> Vec<&RegionNode> {
        let mut candidates: Vec<&RegionNode> = self.regions.values().collect();
        candidates.sort_by_key(|n| n.failover_priority);
        candidates
    }

    /// 获取区域节点
    pub fn get(&self, region_id: &str) -> Option<&RegionNode> {
        self.regions.get(region_id)
    }

    /// 更新区域健康状态
    pub fn update_health(&self, region_id: &str, status: RegionHealth) {
        self.health.write().set(region_id, status);
    }

    /// 获取区域健康状态
    pub fn health(&self, region_id: &str) -> Option<RegionHealth> {
        self.health.read().get(region_id)
    }

    /// 获取所有健康区域
    pub fn healthy_regions(&self) -> Vec<&RegionNode> {
        let health = self.health.read();
        self.regions
            .values()
            .filter(|n| health.get(&n.region_id) == Some(RegionHealth::Healthy))
            .collect()
    }

    /// 区域数量
    pub fn len(&self) -> usize {
        self.regions.len()
    }

    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }
}

fn is_valid_region_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn simple_hash(s: &str) -> u64 {
    let mut h: u64 = 14695981039346656037;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_region(id: &str, role: RegionRole, priority: u8) -> RegionNode {
        RegionNode {
            region_id: id.to_string(),
            role,
            failover_priority: priority,
            data_affinity: DataAffinityPolicy::Static,
            replication_mode: ReplicationMode::Async,
            replication_lag_threshold: Duration::from_millis(200),
            dsn: format!("mysql://{}", id),
        }
    }

    #[test]
    fn declare_valid_topology() {
        let topo = RegionTopology::declare(vec![
            make_region("us-east-1", RegionRole::Primary, 1),
            make_region("us-west-2", RegionRole::Secondary, 2),
            make_region("eu-west-1", RegionRole::Secondary, 3),
        ])
        .unwrap();
        assert_eq!(topo.len(), 3);
    }

    #[test]
    fn declare_duplicate_id_rejected() {
        let result = RegionTopology::declare(vec![
            make_region("us-east-1", RegionRole::Primary, 1),
            make_region("us-east-1", RegionRole::Secondary, 2),
        ]);
        assert!(matches!(result, Err(TopologyError::DuplicateRegionId(_))));
    }

    #[test]
    fn declare_missing_primary_rejected() {
        let result = RegionTopology::declare(vec![
            make_region("us-east-1", RegionRole::Secondary, 1),
            make_region("us-west-2", RegionRole::Secondary, 2),
        ]);
        assert!(matches!(result, Err(TopologyError::MissingPrimary)));
    }

    #[test]
    fn declare_duplicate_priority_rejected() {
        let result = RegionTopology::declare(vec![
            make_region("us-east-1", RegionRole::Primary, 1),
            make_region("us-west-2", RegionRole::Secondary, 1),
        ]);
        assert!(matches!(result, Err(TopologyError::DuplicatePriority(_))));
    }

    #[test]
    fn declare_invalid_region_id_rejected() {
        let mut node = make_region("US_EAST_1", RegionRole::Primary, 1);
        node.region_id = "US_EAST_1".to_string();
        let result = RegionTopology::declare(vec![node]);
        assert!(matches!(result, Err(TopologyError::InvalidRegionId(_))));
    }

    #[test]
    fn declare_priority_out_of_range() {
        let result =
            RegionTopology::declare(vec![make_region("us-east-1", RegionRole::Primary, 0)]);
        assert!(matches!(result, Err(TopologyError::PriorityOutOfRange(_))));
    }

    #[test]
    fn update_health() {
        let topo = RegionTopology::declare(vec![
            make_region("us-east-1", RegionRole::Primary, 1),
            make_region("us-west-2", RegionRole::Secondary, 2),
        ])
        .unwrap();
        assert_eq!(topo.health("us-east-1"), Some(RegionHealth::Healthy));
        topo.update_health("us-east-1", RegionHealth::Unavailable);
        assert_eq!(topo.health("us-east-1"), Some(RegionHealth::Unavailable));
        assert_eq!(topo.healthy_regions().len(), 1);
    }

    #[test]
    fn candidates_by_priority_sorted() {
        let topo = RegionTopology::declare(vec![
            make_region("us-east-1", RegionRole::Primary, 3),
            make_region("us-west-2", RegionRole::Secondary, 1),
            make_region("eu-west-1", RegionRole::Secondary, 2),
        ])
        .unwrap();
        let candidates = topo.candidates_by_priority();
        assert_eq!(candidates[0].failover_priority, 1);
        assert_eq!(candidates[1].failover_priority, 2);
        assert_eq!(candidates[2].failover_priority, 3);
    }

    #[test]
    fn find_affinity_region_consistent() {
        let topo = RegionTopology::declare(vec![
            make_region("us-east-1", RegionRole::Primary, 1),
            make_region("us-west-2", RegionRole::Secondary, 2),
        ])
        .unwrap();
        let r1 = topo.find_affinity_region("user:123");
        let r2 = topo.find_affinity_region("user:123");
        assert_eq!(
            r1.map(|r| r.region_id.as_str()),
            r2.map(|r| r.region_id.as_str())
        );
    }
}
