//! v9.7.0 NUMA 感知连接池 — 拓扑探测
//!
//! 运行时探测系统 NUMA 拓扑（节点数、每节点 CPU 核心集合），
//! 用于连接池分区配置。非 Linux、单节点、容器环境自动降级为单节点。
//!
//! # 安全保证
//!
//! 仅读取固定路径 `/sys/devices/system/node/`，不接受用户输入，无路径遍历风险。

use std::sync::atomic::{AtomicU64, Ordering};

/// NUMA 节点
#[derive(Debug, Clone)]
pub struct NumaNode {
    /// 节点 ID
    pub id: usize,
    /// 该节点 CPU 核心集合
    pub cpu_set: Vec<usize>,
}

/// NUMA 拓扑结构
///
/// 运行时探测当前系统 NUMA 拓扑（节点数、每节点 CPU 核心集合）。
/// 非 Linux、单节点、容器环境自动降级为单节点。
#[derive(Debug, Clone)]
pub struct NumaTopology {
    /// NUMA 节点列表（每节点含 CPU 核心集合）
    pub nodes: Vec<NumaNode>,
    /// 是否降级（单节点/非 Linux）
    pub degraded: bool,
}

impl NumaTopology {
    /// 探测当前系统 NUMA 拓扑
    ///
    /// Linux：读取 `/sys/devices/system/node/` 解析节点数+CPU 映射
    /// 非 Linux/容器/读取失败：降级为单节点，`degraded=true`
    ///
    /// // SAFETY: 仅读取固定路径 `/sys/devices/system/node/`，不接受用户输入，
    /// // // 无路径遍历风险。读取失败时降级为单节点，不 panic。
    pub fn detect() -> Self {
        #[cfg(target_os = "linux")]
        {
            return Self::detect_linux();
        }

        #[cfg(not(target_os = "linux"))]
        {
            tracing::warn!("NUMA topology detection: non-Linux platform, degrading to single node");
            Self::single_node()
        }
    }

    /// 降级为单节点拓扑
    fn single_node() -> Self {
        Self {
            nodes: vec![NumaNode {
                id: 0,
                cpu_set: (0..num_cpus()).collect(),
            }],
            degraded: true,
        }
    }

    /// Linux 平台 NUMA 拓扑探测
    #[cfg(target_os = "linux")]
    fn detect_linux() -> Self {
        // SAFETY: 仅读取固定路径 /sys/devices/system/node/，不接受用户输入，
        // 无路径遍历风险。读取失败时降级为单节点，不 panic。
        let node_dir = "/sys/devices/system/node/";

        let entries = match std::fs::read_dir(node_dir) {
            Ok(e) => e,
            Err(_) => {
                tracing::warn!(
                    "NUMA topology: cannot read {}, degrading to single node",
                    node_dir
                );
                return Self::single_node();
            }
        };

        let mut nodes = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            if !name_str.starts_with("node") {
                continue;
            }

            let id_str = &name_str[4..];
            let id: usize = match id_str.parse() {
                Ok(v) => v,
                Err(_) => continue,
            };

            let cpulist_path = format!("{}/{}/cpulist", node_dir, name_str);
            let cpu_set = match std::fs::read_to_string(&cpulist_path) {
                Ok(content) => parse_cpulist(&content),
                Err(_) => Vec::new(),
            };

            nodes.push(NumaNode { id, cpu_set });
        }

        if nodes.is_empty() || nodes.len() == 1 {
            if nodes.is_empty() {
                tracing::warn!("NUMA topology: no nodes found, degrading to single node");
            }
            return Self::single_node();
        }

        nodes.sort_by_key(|n| n.id);

        Self {
            nodes,
            degraded: false,
        }
    }

    /// 获取当前线程所在 NUMA 节点 ID
    ///
    /// 通过当前 CPU 亲和性判定当前线程所在 NUMA 节点。
    /// 降级模式返回 0（唯一节点）。
    pub fn current_node(&self) -> usize {
        if self.degraded {
            return 0;
        }

        #[cfg(target_os = "linux")]
        {
            if let Some(cpu) = current_cpu_linux() {
                for node in &self.nodes {
                    if node.cpu_set.contains(&cpu) {
                        return node.id;
                    }
                }
            }
        }

        0
    }

    /// 节点数
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// 是否为多节点（`node_count > 1` 且 `!degraded`）
    pub fn is_multi_node(&self) -> bool {
        self.node_count() > 1 && !self.degraded
    }

    /// 获取指定节点 ID 的 CPU 核心数
    pub fn cpu_count(&self, node_id: usize) -> usize {
        self.nodes
            .iter()
            .find(|n| n.id == node_id)
            .map(|n| n.cpu_set.len())
            .unwrap_or(0)
    }

    /// 总 CPU 核心数
    pub fn total_cpus(&self) -> usize {
        self.nodes.iter().map(|n| n.cpu_set.len()).sum()
    }
}

/// 获取当前 CPU ID（Linux）
#[cfg(target_os = "linux")]
fn current_cpu_linux() -> Option<usize> {
    // 尝试通过 /proc/self/stat 获取当前 CPU
    // /proc/self/stat 的第 39 个字段（从 0 开始索引 38）是当前 CPU ID
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let fields: Vec<&str> = stat.split_whitespace().collect();
    fields.get(38).and_then(|s| s.parse().ok())
}

/// 解析 cpulist 格式（如 "0-3,8-11" → [0,1,2,3,8,9,10,11]）
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_cpulist(content: &str) -> Vec<usize> {
    let mut result = Vec::new();
    for part in content.trim().split(',') {
        if part.is_empty() {
            continue;
        }
        if let Some((start, end)) = part.split_once('-') {
            if let (Ok(s), Ok(e)) = (start.parse::<usize>(), end.parse::<usize>()) {
                result.extend(s..=e);
            }
        } else if let Ok(v) = part.parse::<usize>() {
            result.push(v);
        }
    }
    result
}

/// 获取 CPU 核心数
fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

// ============================================================================
// 分区策略 + NUMA 配置
// ============================================================================

/// 分区策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionStrategy {
    /// 均匀分区：各节点连接数相等
    Even,
    /// 按 CPU 核心数加权：节点连接数 ∝ CPU 核心数
    Weighted,
}

/// NUMA 感知连接池配置
#[derive(Debug, Clone)]
pub struct NumaConfig {
    /// 是否启用 NUMA 感知（默认 false，向后兼容单分区）
    pub enabled: bool,
    /// 分区策略（默认 Even）
    pub partition_strategy: PartitionStrategy,
    /// 当前分区无连接时是否回退到其他分区（默认 true，不阻塞业务）
    pub fallback: bool,
    /// 跨节点访问率告警阈值（默认 0.5，∈ (0, 1]）
    pub cross_node_alert_threshold: f64,
    /// 各分区是否独立预热（默认 true）
    pub independent_warmup: bool,
}

impl Default for NumaConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            partition_strategy: PartitionStrategy::Even,
            fallback: true,
            cross_node_alert_threshold: 0.5,
            independent_warmup: true,
        }
    }
}

impl NumaConfig {
    /// 计算各分区连接数
    ///
    /// `Even`：各分区 `max_size / node_count`
    /// `Weighted`：各分区 `max_size * cpu_count / total_cpus`
    pub fn partition_sizes(&self, max_size: u32, topology: &NumaTopology) -> Vec<u32> {
        let node_count = topology.node_count();
        if node_count == 0 {
            return vec![max_size];
        }

        match self.partition_strategy {
            PartitionStrategy::Even => {
                let per_node = max_size / node_count as u32;
                let remainder = max_size - per_node * node_count as u32;
                let mut sizes = vec![per_node; node_count];
                if remainder > 0 {
                    sizes[0] += remainder;
                }
                sizes
            }
            PartitionStrategy::Weighted => {
                let total_cpus = topology.total_cpus() as u32;
                if total_cpus == 0 {
                    return vec![max_size / node_count as u32; node_count];
                }
                topology
                    .nodes
                    .iter()
                    .map(|n| {
                        let cpu_count = n.cpu_set.len() as u32;
                        max_size * cpu_count / total_cpus
                    })
                    .collect()
            }
        }
    }
}

// ============================================================================
// NUMA 分区连接池可观测指标
// ============================================================================

/// NUMA 分区连接池可观测指标
#[derive(Debug, Clone)]
pub struct NumaPoolMetrics {
    /// 节点数
    pub node_count: usize,
    /// 各分区连接统计
    pub partitions: Vec<NumaPartitionStats>,
    /// 跨节点访问总次数
    pub cross_node_access_count: u64,
    /// 总 acquire 次数
    pub total_acquire_count: u64,
}

impl NumaPoolMetrics {
    /// 跨节点访问率 = cross_node / total
    pub fn cross_node_access_rate(&self) -> f64 {
        if self.total_acquire_count == 0 {
            0.0
        } else {
            self.cross_node_access_count as f64 / self.total_acquire_count as f64
        }
    }
}

/// 单个 NUMA 分区统计
#[derive(Debug, Clone)]
pub struct NumaPartitionStats {
    /// 节点 ID
    pub node_id: usize,
    /// 该分区空闲连接数
    pub idle_count: usize,
    /// 该分区总连接数
    pub total_count: usize,
    /// 该分区 CPU 核心数
    pub cpu_count: usize,
}

/// 跨节点访问计数器（原子计数，用于 `NumaPartitionedPool` 内部）
#[derive(Debug)]
pub struct CrossNodeCounter {
    cross_node: AtomicU64,
    total: AtomicU64,
}

impl CrossNodeCounter {
    /// 创建新的跨节点计数器
    pub fn new() -> Self {
        Self {
            cross_node: AtomicU64::new(0),
            total: AtomicU64::new(0),
        }
    }

    /// 记录一次跨节点访问
    pub fn record_cross_node(&self) {
        self.cross_node.fetch_add(1, Ordering::Relaxed);
    }

    /// 记录一次总访问
    pub fn record_total(&self) {
        self.total.fetch_add(1, Ordering::Relaxed);
    }

    /// 获取跨节点访问次数
    pub fn cross_node_count(&self) -> u64 {
        self.cross_node.load(Ordering::Relaxed)
    }

    /// 获取总访问次数
    pub fn total_count(&self) -> u64 {
        self.total.load(Ordering::Relaxed)
    }

    /// 跨节点访问率（total == 0 时返回 0.0）
    pub fn rate(&self) -> f64 {
        let total = self.total_count();
        if total == 0 {
            0.0
        } else {
            self.cross_node_count() as f64 / total as f64
        }
    }
}

impl Default for CrossNodeCounter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_numa_topology_detect() {
        let topo = NumaTopology::detect();
        assert!(topo.node_count() >= 1, "should have at least 1 node");
        for node in &topo.nodes {
            assert!(
                node.cpu_set.iter().all(|&cpu| cpu < 1024),
                "cpu ids should be reasonable"
            );
        }
    }

    #[test]
    fn test_numa_topology_degrade_single_node() {
        let topo = NumaTopology::single_node();
        assert!(topo.degraded);
        assert_eq!(topo.node_count(), 1);
        assert!(!topo.is_multi_node());
    }

    #[test]
    fn test_numa_topology_current_node_degraded() {
        let topo = NumaTopology::single_node();
        assert_eq!(topo.current_node(), 0);
    }

    #[test]
    fn test_numa_topology_current_node_detect() {
        let topo = NumaTopology::detect();
        let node = topo.current_node();
        assert!(node < topo.node_count(), "current node should be valid");
    }

    #[test]
    fn test_numa_topology_is_multi_node() {
        let topo = NumaTopology::single_node();
        assert!(!topo.is_multi_node());

        let multi = NumaTopology {
            nodes: vec![
                NumaNode {
                    id: 0,
                    cpu_set: vec![0, 1],
                },
                NumaNode {
                    id: 1,
                    cpu_set: vec![2, 3],
                },
            ],
            degraded: false,
        };
        assert!(multi.is_multi_node());
    }

    #[test]
    fn test_numa_topology_cpu_count() {
        let topo = NumaTopology {
            nodes: vec![
                NumaNode {
                    id: 0,
                    cpu_set: vec![0, 1, 2],
                },
                NumaNode {
                    id: 1,
                    cpu_set: vec![3, 4],
                },
            ],
            degraded: false,
        };
        assert_eq!(topo.cpu_count(0), 3);
        assert_eq!(topo.cpu_count(1), 2);
        assert_eq!(topo.cpu_count(99), 0);
        assert_eq!(topo.total_cpus(), 5);
    }

    #[test]
    fn test_parse_cpulist() {
        assert_eq!(parse_cpulist("0-3"), vec![0, 1, 2, 3]);
        assert_eq!(parse_cpulist("0-3,8-11"), vec![0, 1, 2, 3, 8, 9, 10, 11]);
        assert_eq!(parse_cpulist("0,2,4"), vec![0, 2, 4]);
        assert_eq!(parse_cpulist("0-1,3,5-6"), vec![0, 1, 3, 5, 6]);
        assert_eq!(parse_cpulist(""), Vec::<usize>::new());
        assert_eq!(parse_cpulist("  0-3  "), vec![0, 1, 2, 3]);
    }

    #[test]
    fn test_numa_config_default() {
        let config = NumaConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.partition_strategy, PartitionStrategy::Even);
        assert!(config.fallback);
        assert!((config.cross_node_alert_threshold - 0.5).abs() < 1e-9);
        assert!(config.independent_warmup);
    }

    #[test]
    fn test_numa_config_partition_sizes_even() {
        let topo = NumaTopology {
            nodes: vec![
                NumaNode {
                    id: 0,
                    cpu_set: vec![0, 1],
                },
                NumaNode {
                    id: 1,
                    cpu_set: vec![2, 3],
                },
            ],
            degraded: false,
        };
        let config = NumaConfig {
            enabled: true,
            partition_strategy: PartitionStrategy::Even,
            ..Default::default()
        };
        let sizes = config.partition_sizes(20, &topo);
        assert_eq!(sizes.len(), 2);
        assert_eq!(sizes[0], 10);
        assert_eq!(sizes[1], 10);
    }

    #[test]
    fn test_numa_config_partition_sizes_even_remainder() {
        let topo = NumaTopology {
            nodes: vec![
                NumaNode {
                    id: 0,
                    cpu_set: vec![0, 1],
                },
                NumaNode {
                    id: 1,
                    cpu_set: vec![2, 3],
                },
            ],
            degraded: false,
        };
        let config = NumaConfig::default();
        let sizes = config.partition_sizes(21, &topo);
        assert_eq!(sizes.len(), 2);
        assert_eq!(sizes[0], 11);
        assert_eq!(sizes[1], 10);
    }

    #[test]
    fn test_numa_config_partition_sizes_weighted() {
        let topo = NumaTopology {
            nodes: vec![
                NumaNode {
                    id: 0,
                    cpu_set: vec![0, 1, 2, 3],
                },
                NumaNode {
                    id: 1,
                    cpu_set: vec![4, 5],
                },
            ],
            degraded: false,
        };
        let config = NumaConfig {
            enabled: true,
            partition_strategy: PartitionStrategy::Weighted,
            ..Default::default()
        };
        let sizes = config.partition_sizes(12, &topo);
        assert_eq!(sizes.len(), 2);
        assert_eq!(sizes[0], 8);
        assert_eq!(sizes[1], 4);
    }

    #[test]
    fn test_numa_config_partition_sizes_single_node() {
        let topo = NumaTopology::single_node();
        let config = NumaConfig::default();
        let sizes = config.partition_sizes(10, &topo);
        assert_eq!(sizes, vec![10]);
    }

    #[test]
    fn test_numa_pool_metrics_cross_node_rate() {
        let metrics = NumaPoolMetrics {
            node_count: 2,
            partitions: vec![],
            cross_node_access_count: 200,
            total_acquire_count: 1000,
        };
        assert!((metrics.cross_node_access_rate() - 0.2).abs() < 1e-9);
    }

    #[test]
    fn test_numa_pool_metrics_cross_node_rate_zero() {
        let metrics = NumaPoolMetrics {
            node_count: 1,
            partitions: vec![],
            cross_node_access_count: 0,
            total_acquire_count: 0,
        };
        assert_eq!(metrics.cross_node_access_rate(), 0.0);
    }

    #[test]
    fn test_cross_node_counter() {
        let counter = CrossNodeCounter::new();
        counter.record_total();
        counter.record_total();
        counter.record_total();
        counter.record_cross_node();
        assert_eq!(counter.total_count(), 3);
        assert_eq!(counter.cross_node_count(), 1);
        assert!((counter.rate() - (1.0 / 3.0)).abs() < 1e-9);
    }

    #[test]
    fn test_partition_strategy_eq() {
        assert_eq!(PartitionStrategy::Even, PartitionStrategy::Even);
        assert_ne!(PartitionStrategy::Even, PartitionStrategy::Weighted);
    }

    #[test]
    fn test_numa_node_clone() {
        let node = NumaNode {
            id: 0,
            cpu_set: vec![0, 1, 2],
        };
        let cloned = node.clone();
        assert_eq!(cloned.id, 0);
        assert_eq!(cloned.cpu_set, vec![0, 1, 2]);
    }

    #[test]
    fn test_numa_topology_clone() {
        let topo = NumaTopology {
            nodes: vec![NumaNode {
                id: 0,
                cpu_set: vec![0],
            }],
            degraded: true,
        };
        let cloned = topo.clone();
        assert_eq!(cloned.node_count(), 1);
        assert!(cloned.degraded);
    }

    #[test]
    fn test_numa_config_clone() {
        let config = NumaConfig {
            enabled: true,
            partition_strategy: PartitionStrategy::Weighted,
            fallback: false,
            cross_node_alert_threshold: 0.3,
            independent_warmup: false,
        };
        let cloned = config.clone();
        assert!(cloned.enabled);
        assert_eq!(cloned.partition_strategy, PartitionStrategy::Weighted);
        assert!(!cloned.fallback);
        assert!(!cloned.independent_warmup);
    }
}
