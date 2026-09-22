//! 跨区域复制配置（v8.1.0，`cross-region-replicate` feature gate）
//!
//! 复制模式配置（async/semi_sync/sync）→ 延迟监控 → 超阈值告警 `CROSS_REGION_REPLICATE_LAG`。
//! 同区域 ≤ 10ms，同大陆 ≤ 1s，跨大洲 ≤ 5s。
//! 复用既有 `replication_lag.rs`（ReplicationLagTracker）和 `global_router.rs`。

use std::collections::HashMap;
use std::time::Duration;

use crate::replication_lag::{LinkType, ReplicationLagTracker};

/// 跨区域复制错误
#[derive(Debug, Clone)]
pub enum DistError {
    /// 复制模式无效
    InvalidReplicateMode(String),
    /// 延迟超阈值（告警 `CROSS_REGION_REPLICATE_LAG`）
    LagExceeded {
        source: String,
        target: String,
        lag: Duration,
        threshold: Duration,
    },
}

impl std::fmt::Display for DistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidReplicateMode(msg) => write!(f, "invalid replicate mode: {msg}"),
            Self::LagExceeded {
                source,
                target,
                lag,
                threshold,
            } => {
                write!(f, "CROSS_REGION_REPLICATE_LAG: {source}->{target} lag={lag:?} > threshold={threshold:?}")
            }
        }
    }
}

impl std::error::Error for DistError {}

/// 复制模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplicateMode {
    /// 异步复制（最高性能，最低一致性）
    Async,
    /// 半同步复制（主 + 至少一个从确认）
    SemiSync,
    /// 同步复制（主 + 所有从确认，最高一致性）
    Sync,
}

impl ReplicateMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReplicateMode::Async => "async",
            ReplicateMode::SemiSync => "semi_sync",
            ReplicateMode::Sync => "sync",
        }
    }

    pub fn parse(s: &str) -> Result<Self, DistError> {
        match s {
            "async" => Ok(ReplicateMode::Async),
            "semi_sync" => Ok(ReplicateMode::SemiSync),
            "sync" => Ok(ReplicateMode::Sync),
            _ => Err(DistError::InvalidReplicateMode(s.to_string())),
        }
    }
}

/// 复制拓扑
#[derive(Debug, Clone, Default)]
pub struct ReplicateTopology {
    /// 区域列表
    pub regions: Vec<String>,
    /// 区域间链接类型（(source, target) → link_type）
    pub links: HashMap<(String, String), LinkType>,
}

impl ReplicateTopology {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_link(&mut self, source: &str, target: &str, link_type: LinkType) {
        let s = source.to_string();
        let t = target.to_string();
        if !self.regions.contains(&s) {
            self.regions.push(s.clone());
        }
        if !self.regions.contains(&t) {
            self.regions.push(t.clone());
        }
        self.links.insert((s, t), link_type);
    }
}

/// 跨区域复制配置
///
/// 复制模式配置 → 延迟监控 → 超阈值告警 `CROSS_REGION_REPLICATE_LAG`。
/// 同区域 ≤ 10ms，同大陆 ≤ 1s，跨大洲 ≤ 5s。
pub struct CrossRegionReplicateConfig {
    mode: ReplicateMode,
    lag_tracker: ReplicationLagTracker,
    /// 链接类型映射（用于阈值查询）
    link_types: HashMap<(String, String), LinkType>,
    /// 同区域延迟阈值（≤ 10ms）
    pub same_region_threshold: Duration,
    /// 同大陆延迟阈值（≤ 1s）
    pub same_continent_threshold: Duration,
    /// 跨大洲延迟阈值（≤ 5s）
    pub cross_continent_threshold: Duration,
}

impl CrossRegionReplicateConfig {
    /// 创建配置
    pub fn new(mode: ReplicateMode) -> Self {
        Self {
            mode,
            lag_tracker: ReplicationLagTracker::new(),
            link_types: HashMap::new(),
            same_region_threshold: Duration::from_millis(10),
            same_continent_threshold: Duration::from_secs(1),
            cross_continent_threshold: Duration::from_secs(5),
        }
    }

    /// 配置复制模式和拓扑
    ///
    /// 复制模式配置 → 延迟监控 → 超阈值告警。
    pub fn configure(&mut self, mode: ReplicateMode, topology: &ReplicateTopology) {
        self.mode = mode;
        self.link_types = topology.links.clone();
        for ((source, target), link_type) in &topology.links {
            let threshold = self.threshold_for_link(*link_type);
            self.lag_tracker.set_threshold(source, target, threshold);
        }
    }

    /// 获取链接类型对应的阈值
    fn threshold_for_link(&self, link_type: LinkType) -> Duration {
        match link_type {
            LinkType::SameCity => self.same_region_threshold,
            LinkType::CrossContinent => self.cross_continent_threshold,
        }
    }

    /// 记录复制延迟
    pub fn record_lag(&self, source: &str, target: &str, lag: Duration) {
        self.lag_tracker.record_lag(source, target, lag);
    }

    /// 检查延迟是否超阈值
    pub fn check_lag(&self, source: &str, target: &str) -> Result<(), DistError> {
        if let Some(true) = self.lag_tracker.check_threshold(source, target) {
            let lag = self.lag_tracker.p99_lag(source, target);
            let key = (source.to_string(), target.to_string());
            let link_type = self
                .link_types
                .get(&key)
                .copied()
                .unwrap_or(LinkType::CrossContinent);
            let threshold = self.threshold_for_link(link_type);
            return Err(DistError::LagExceeded {
                source: source.to_string(),
                target: target.to_string(),
                lag,
                threshold,
            });
        }
        Ok(())
    }

    /// 当前复制模式
    pub fn mode(&self) -> ReplicateMode {
        self.mode
    }

    /// P99 延迟
    pub fn p99_lag(&self, source: &str, target: &str) -> Duration {
        self.lag_tracker.p99_lag(source, target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replicate_mode_from_str() {
        assert_eq!(ReplicateMode::parse("async").unwrap(), ReplicateMode::Async);
        assert_eq!(
            ReplicateMode::parse("semi_sync").unwrap(),
            ReplicateMode::SemiSync
        );
        assert_eq!(ReplicateMode::parse("sync").unwrap(), ReplicateMode::Sync);
        assert!(ReplicateMode::parse("invalid").is_err());
    }

    #[test]
    fn test_async_mode_configure() {
        let mut config = CrossRegionReplicateConfig::new(ReplicateMode::Async);
        let mut topo = ReplicateTopology::new();
        topo.add_link("region_a", "region_b", LinkType::SameCity);
        config.configure(ReplicateMode::Async, &topo);
        assert_eq!(config.mode(), ReplicateMode::Async);
    }

    #[test]
    fn test_semi_sync_mode_configure() {
        let mut config = CrossRegionReplicateConfig::new(ReplicateMode::SemiSync);
        let mut topo = ReplicateTopology::new();
        topo.add_link("region_a", "region_b", LinkType::SameCity);
        config.configure(ReplicateMode::SemiSync, &topo);
        assert_eq!(config.mode(), ReplicateMode::SemiSync);
    }

    #[test]
    fn test_sync_mode_configure() {
        let mut config = CrossRegionReplicateConfig::new(ReplicateMode::Sync);
        let mut topo = ReplicateTopology::new();
        topo.add_link("region_a", "region_b", LinkType::SameCity);
        config.configure(ReplicateMode::Sync, &topo);
        assert_eq!(config.mode(), ReplicateMode::Sync);
    }

    #[test]
    fn test_same_region_lag_within_threshold() {
        let mut config = CrossRegionReplicateConfig::new(ReplicateMode::Async);
        let mut topo = ReplicateTopology::new();
        topo.add_link("a", "b", LinkType::SameCity);
        config.configure(ReplicateMode::Async, &topo);
        config.record_lag("a", "b", Duration::from_millis(5));
        assert!(config.check_lag("a", "b").is_ok());
    }

    #[test]
    fn test_same_region_lag_exceeded() {
        let mut config = CrossRegionReplicateConfig::new(ReplicateMode::Async);
        let mut topo = ReplicateTopology::new();
        topo.add_link("a", "b", LinkType::SameCity);
        config.configure(ReplicateMode::Async, &topo);
        config.record_lag("a", "b", Duration::from_millis(50));
        let err = config.check_lag("a", "b").unwrap_err();
        assert!(matches!(err, DistError::LagExceeded { .. }));
    }

    #[test]
    fn test_cross_continent_lag_within_threshold() {
        let mut config = CrossRegionReplicateConfig::new(ReplicateMode::Async);
        let mut topo = ReplicateTopology::new();
        topo.add_link("us", "eu", LinkType::CrossContinent);
        config.configure(ReplicateMode::Async, &topo);
        config.record_lag("us", "eu", Duration::from_secs(3));
        assert!(config.check_lag("us", "eu").is_ok());
    }

    #[test]
    fn test_cross_continent_lag_exceeded() {
        let mut config = CrossRegionReplicateConfig::new(ReplicateMode::Async);
        let mut topo = ReplicateTopology::new();
        topo.add_link("us", "eu", LinkType::CrossContinent);
        config.configure(ReplicateMode::Async, &topo);
        config.record_lag("us", "eu", Duration::from_secs(8));
        let err = config.check_lag("us", "eu").unwrap_err();
        assert!(matches!(err, DistError::LagExceeded { .. }));
    }

    #[test]
    fn test_p99_lag_calculation() {
        let mut config = CrossRegionReplicateConfig::new(ReplicateMode::Async);
        let mut topo = ReplicateTopology::new();
        topo.add_link("a", "b", LinkType::SameCity);
        config.configure(ReplicateMode::Async, &topo);
        for i in 1..=100 {
            config.record_lag("a", "b", Duration::from_millis(i));
        }
        let p99 = config.p99_lag("a", "b");
        assert!(p99 >= Duration::from_millis(99));
    }

    #[test]
    fn test_multiple_links_mixed() {
        let mut config = CrossRegionReplicateConfig::new(ReplicateMode::SemiSync);
        let mut topo = ReplicateTopology::new();
        topo.add_link("a", "b", LinkType::SameCity);
        topo.add_link("us", "eu", LinkType::CrossContinent);
        config.configure(ReplicateMode::SemiSync, &topo);
        config.record_lag("a", "b", Duration::from_millis(5));
        config.record_lag("us", "eu", Duration::from_secs(2));
        assert!(config.check_lag("a", "b").is_ok());
        assert!(config.check_lag("us", "eu").is_ok());
    }
}
