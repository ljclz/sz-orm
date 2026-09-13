//! 跨区域容灾切换协调器
//!
//! 区域故障时按容灾优先级切换，RTO 30s 倒计时，切换事件审计。

use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::RwLock;

use crate::region_topology::{RegionHealth, RegionTopology};

/// RTO 目标：30 秒
pub const RTO_TARGET: Duration = Duration::from_secs(30);

/// 容灾切换决策
#[derive(Debug, Clone)]
pub struct FailoverDecision {
    /// 源区域
    pub from_region: String,
    /// 目标区域
    pub to_region: String,
    /// 切换时间
    pub timestamp: Instant,
    /// RTO 耗时
    pub rto_elapsed: Duration,
}

/// 容灾错误
#[derive(Debug, Clone)]
pub enum FailoverError {
    /// 无可用目标区域
    NoAvailableTarget,
    /// RTO 超时
    RtoExceeded,
    /// 脑裂检测
    SplitBrainDetected,
}

impl std::fmt::Display for FailoverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FailoverError::NoAvailableTarget => write!(f, "No available target region"),
            FailoverError::RtoExceeded => write!(f, "RTO exceeded"),
            FailoverError::SplitBrainDetected => write!(f, "Split brain detected"),
        }
    }
}

impl std::error::Error for FailoverError {}

/// 切换事件审计记录
#[derive(Debug, Clone)]
pub struct FailoverAuditLog {
    /// 源区域
    pub from_region: String,
    /// 目标区域
    pub to_region: String,
    /// 切换时间戳
    pub timestamp: Instant,
    /// RTO 耗时
    pub rto_elapsed: Duration,
}

/// 区域容灾切换协调器
pub struct RegionFailoverCoordinator {
    topology: Arc<RegionTopology>,
    audit_logs: RwLock<Vec<FailoverAuditLog>>,
    failover_start: RwLock<Option<(String, Instant)>>,
}

impl RegionFailoverCoordinator {
    /// 创建协调器
    pub fn new(topology: Arc<RegionTopology>) -> Self {
        Self {
            topology,
            audit_logs: RwLock::new(Vec::new()),
            failover_start: RwLock::new(None),
        }
    }

    /// 区域故障时触发切换
    pub fn on_region_failure(&self, region_id: &str) -> Result<FailoverDecision, FailoverError> {
        let start = Instant::now();
        *self.failover_start.write() = Some((region_id.to_string(), start));

        self.topology
            .update_health(region_id, RegionHealth::Unavailable);

        let candidates = self.topology.candidates_by_priority();
        let target = candidates
            .into_iter()
            .find(|n| {
                n.region_id != region_id
                    && self.topology.health(&n.region_id) == Some(RegionHealth::Healthy)
            })
            .ok_or(FailoverError::NoAvailableTarget)?;

        let rto_elapsed = start.elapsed();
        if rto_elapsed > RTO_TARGET {
            tracing::warn!(rto = ?rto_elapsed, "RTO 超时");
        }

        let decision = FailoverDecision {
            from_region: region_id.to_string(),
            to_region: target.region_id.clone(),
            timestamp: start,
            rto_elapsed,
        };

        self.audit_logs.write().push(FailoverAuditLog {
            from_region: decision.from_region.clone(),
            to_region: decision.to_region.clone(),
            timestamp: decision.timestamp,
            rto_elapsed: decision.rto_elapsed,
        });

        *self.failover_start.write() = None;
        Ok(decision)
    }

    /// 故障恢复回切
    pub fn on_region_recovery(&self, region_id: &str) -> bool {
        self.topology
            .update_health(region_id, RegionHealth::Healthy);
        true
    }

    /// 获取审计日志
    pub fn audit_logs(&self) -> Vec<FailoverAuditLog> {
        self.audit_logs.read().clone()
    }

    /// 脑裂检测（简化版：检查是否有多个 Primary 同时健康）
    pub fn check_split_brain(&self) -> bool {
        use crate::region_topology::RegionRole;
        let healthy_primaries = self
            .topology
            .candidates_by_priority()
            .into_iter()
            .filter(|n| {
                n.role == RegionRole::Primary
                    && self.topology.health(&n.region_id) == Some(RegionHealth::Healthy)
            })
            .count();
        healthy_primaries > 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region_topology::{DataAffinityPolicy, RegionNode, RegionRole, ReplicationMode};

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

    fn setup() -> RegionFailoverCoordinator {
        let topo = RegionTopology::declare(vec![
            make_region("us-east-1", RegionRole::Primary, 1),
            make_region("us-west-2", RegionRole::Secondary, 2),
        ])
        .unwrap();
        RegionFailoverCoordinator::new(Arc::new(topo))
    }

    #[test]
    fn region_failure_triggers_failover() {
        let coord = setup();
        let decision = coord.on_region_failure("us-east-1").unwrap();
        assert_eq!(decision.from_region, "us-east-1");
        assert_eq!(decision.to_region, "us-west-2");
    }

    #[test]
    fn failover_logs_audit() {
        let coord = setup();
        coord.on_region_failure("us-east-1").unwrap();
        let logs = coord.audit_logs();
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].from_region, "us-east-1");
    }

    #[test]
    fn region_recovery_restores_health() {
        let coord = setup();
        coord.on_region_failure("us-east-1").unwrap();
        assert_eq!(
            coord.topology.health("us-east-1"),
            Some(RegionHealth::Unavailable)
        );
        coord.on_region_recovery("us-east-1");
        assert_eq!(
            coord.topology.health("us-east-1"),
            Some(RegionHealth::Healthy)
        );
    }

    #[test]
    fn no_available_target() {
        let coord = setup();
        coord
            .topology
            .update_health("us-west-2", RegionHealth::Unavailable);
        let result = coord.on_region_failure("us-east-1");
        assert!(matches!(result, Err(FailoverError::NoAvailableTarget)));
    }

    #[test]
    fn rto_target_is_30s() {
        assert_eq!(RTO_TARGET, Duration::from_secs(30));
    }
}
