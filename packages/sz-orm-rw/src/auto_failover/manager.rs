//! AutoFailoverManager：自动检测 + slave 提升 + 路由更新 + 脑裂检测 + 数据丢失评估

use std::collections::HashMap;
use std::fmt;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use super::split_brain::{DemotionStrategy, SplitBrainDetector};

/// failover 配置
#[derive(Debug, Clone)]
pub struct FailoverConfig {
    pub check_interval: Duration,
    pub failure_threshold: u32,
    pub lag_threshold: Duration,
    pub switch_timeout: Duration,
}

impl Default for FailoverConfig {
    fn default() -> Self {
        Self {
            check_interval: Duration::from_secs(1),
            failure_threshold: 3,
            lag_threshold: Duration::from_secs(1),
            switch_timeout: Duration::from_secs(30),
        }
    }
}

/// failover 操作类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailoverOperator {
    Auto,
    Manual,
}

/// 数据丢失风险评估
#[derive(Debug, Clone)]
pub struct DataLossRisk {
    pub lag: Duration,
    pub estimated_lost_rows: u64,
    pub is_safe: bool,
}

/// failover 事件
#[derive(Debug, Clone)]
pub struct FailoverEvent {
    pub failure_time: Instant,
    pub detection_confirms: u32,
    pub promoted_slave: String,
    pub old_master: String,
    pub data_loss_assessment: DataLossRisk,
    pub recovery_time: Option<Duration>,
    pub operator: FailoverOperator,
}

/// 脑裂状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SplitBrainStatus {
    NoSplitBrain,
    Detected {
        old_master: String,
        new_master: String,
    },
    Resolved,
}

/// failover 错误
#[derive(Debug, Clone)]
pub enum FailoverError {
    NoHealthySlave,
    PromotionFailed { slave: String, reason: String },
    SplitBrain,
    LagTooHigh { lag: Duration },
    SwitchTimeout,
    NotMonitoring,
}

impl fmt::Display for FailoverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FailoverError::NoHealthySlave => write!(f, "no healthy slave available"),
            FailoverError::PromotionFailed { slave, reason } => {
                write!(f, "promotion failed for slave {slave}: {reason}")
            }
            FailoverError::SplitBrain => write!(f, "split brain detected"),
            FailoverError::LagTooHigh { lag } => write!(f, "lag too high: {lag:?}"),
            FailoverError::SwitchTimeout => write!(f, "switch timeout"),
            FailoverError::NotMonitoring => write!(f, "not monitoring"),
        }
    }
}

impl std::error::Error for FailoverError {}

/// slave 信息
#[derive(Debug, Clone)]
pub struct SlaveInfo {
    pub name: String,
    pub healthy: bool,
    pub lag: Duration,
    pub row_count: u64,
}

/// failover 结果
#[derive(Debug, Clone)]
pub struct FailoverResult {
    pub event: FailoverEvent,
    pub split_brain: SplitBrainStatus,
    pub new_master: String,
}

/// 自动 failover 管理器
pub struct AutoFailoverManager {
    config: FailoverConfig,
    master: RwLock<String>,
    slaves: RwLock<HashMap<String, SlaveInfo>>,
    failure_count: RwLock<u32>,
    events: RwLock<Vec<FailoverEvent>>,
    split_brain: RwLock<SplitBrainStatus>,
    split_brain_detector: SplitBrainDetector,
}

impl AutoFailoverManager {
    pub fn new(config: FailoverConfig, master: String) -> Self {
        let split_brain_detector = SplitBrainDetector::new(config.check_interval);
        Self {
            config,
            master: RwLock::new(master),
            slaves: RwLock::new(HashMap::new()),
            failure_count: RwLock::new(0),
            events: RwLock::new(Vec::new()),
            split_brain: RwLock::new(SplitBrainStatus::NoSplitBrain),
            split_brain_detector,
        }
    }

    /// 添加 slave
    pub fn add_slave(&self, name: &str, lag: Duration, row_count: u64) {
        let mut slaves = self.slaves.write().expect("slaves lock poisoned");
        slaves.insert(
            name.to_string(),
            SlaveInfo {
                name: name.to_string(),
                healthy: true,
                lag,
                row_count,
            },
        );
    }

    /// 更新 slave 健康状态
    pub fn update_slave_health(&self, name: &str, healthy: bool, lag: Duration) {
        let mut slaves = self.slaves.write().expect("slaves lock poisoned");
        if let Some(slave) = slaves.get_mut(name) {
            slave.healthy = healthy;
            slave.lag = lag;
        }
    }

    /// 记录主库健康检查失败
    pub fn record_master_failure(&self) -> Option<FailoverResult> {
        let should_failover = {
            let mut count = self
                .failure_count
                .write()
                .expect("failure_count lock poisoned");
            *count += 1;
            *count >= self.config.failure_threshold
        };

        if should_failover {
            return self.trigger_failover(FailoverOperator::Auto);
        }
        None
    }

    /// 记录主库健康检查成功
    pub fn record_master_success(&self) {
        let mut count = self
            .failure_count
            .write()
            .expect("failure_count lock poisoned");
        *count = 0;
    }

    /// 手动触发 failover
    pub fn manual_failover(&self) -> Result<FailoverResult, FailoverError> {
        self.trigger_failover(FailoverOperator::Manual)
            .ok_or(FailoverError::NoHealthySlave)
    }

    /// 触发 failover
    fn trigger_failover(&self, operator: FailoverOperator) -> Option<FailoverResult> {
        let best_slave = self.select_best_slave()?;
        let old_master = self.master.read().expect("master lock poisoned").clone();

        let data_loss = self.assess_data_loss(&best_slave);
        if !data_loss.is_safe && data_loss.lag > self.config.lag_threshold {
            return None;
        }

        let split_brain = self.detect_split_brain(&old_master, &best_slave.name);

        let event = FailoverEvent {
            failure_time: Instant::now(),
            detection_confirms: *self
                .failure_count
                .read()
                .expect("failure_count lock poisoned"),
            promoted_slave: best_slave.name.clone(),
            old_master: old_master.clone(),
            data_loss_assessment: data_loss,
            recovery_time: Some(Duration::from_secs(0)),
            operator,
        };

        {
            let mut master = self.master.write().expect("master lock poisoned");
            *master = best_slave.name.clone();
        }
        {
            let mut count = self
                .failure_count
                .write()
                .expect("failure_count lock poisoned");
            *count = 0;
        }
        {
            let mut events = self.events.write().expect("events lock poisoned");
            events.push(event.clone());
        }
        {
            let mut sb = self.split_brain.write().expect("split_brain lock poisoned");
            *sb = split_brain.clone();
        }

        self.split_brain_detector.register_old_master(&old_master);

        Some(FailoverResult {
            event,
            split_brain,
            new_master: best_slave.name,
        })
    }

    /// 选择最佳 slave（延迟最低 + 健康）
    fn select_best_slave(&self) -> Option<SlaveInfo> {
        let slaves = self.slaves.read().expect("slaves lock poisoned");
        slaves
            .values()
            .filter(|s| s.healthy && s.lag <= self.config.lag_threshold)
            .min_by_key(|s| s.lag)
            .cloned()
    }

    /// 数据丢失风险评估
    pub fn assess_data_loss(&self, slave: &SlaveInfo) -> DataLossRisk {
        let is_safe = slave.lag <= self.config.lag_threshold;
        let estimated_lost_rows = if is_safe { 0 } else { slave.row_count / 100 };
        DataLossRisk {
            lag: slave.lag,
            estimated_lost_rows,
            is_safe,
        }
    }

    /// 脑裂检测
    pub fn detect_split_brain(&self, old_master: &str, new_master: &str) -> SplitBrainStatus {
        let slaves = self.slaves.read().expect("slaves lock poisoned");
        let old_master_still_healthy = slaves.values().any(|s| s.name == old_master && s.healthy);

        if old_master_still_healthy && old_master != new_master {
            SplitBrainStatus::Detected {
                old_master: old_master.to_string(),
                new_master: new_master.to_string(),
            }
        } else {
            SplitBrainStatus::NoSplitBrain
        }
    }

    /// 获取当前主库
    pub fn current_master(&self) -> String {
        self.master.read().expect("master lock poisoned").clone()
    }

    /// 获取 failover 历史
    pub fn events(&self) -> Vec<FailoverEvent> {
        self.events.read().expect("events lock poisoned").clone()
    }

    /// 获取脑裂状态
    pub fn split_brain_status(&self) -> SplitBrainStatus {
        self.split_brain
            .read()
            .expect("split_brain lock poisoned")
            .clone()
    }

    /// 获取配置
    pub fn config(&self) -> &FailoverConfig {
        &self.config
    }

    /// 检测脑裂：旧主库是否已恢复（由外部健康检查提供）
    pub fn check_split_brain(
        &self,
        old_master: &str,
        old_master_healthy: bool,
    ) -> SplitBrainStatus {
        let new_master = self.current_master();
        self.split_brain_detector
            .check_split_brain(old_master, &new_master, old_master_healthy)
    }

    /// 解决脑裂：降级或隔离旧主库
    pub fn resolve_split_brain(
        &self,
        old_master: &str,
        strategy: DemotionStrategy,
    ) -> Option<super::split_brain::SplitBrainAlert> {
        let new_master = self.current_master();
        if old_master == new_master {
            return None;
        }
        let alert =
            self.split_brain_detector
                .resolve_split_brain(old_master, &new_master, strategy);
        {
            let mut sb = self.split_brain.write().expect("split_brain lock poisoned");
            *sb = SplitBrainStatus::Resolved;
        }
        Some(alert)
    }

    /// 定期检测脑裂
    pub fn periodic_split_brain_check<F>(&self, health_fn: F) -> Vec<SplitBrainStatus>
    where
        F: Fn(&str) -> bool,
    {
        let new_master = self.current_master();
        self.split_brain_detector
            .periodic_check(&new_master, health_fn)
    }

    /// 获取脑裂检测器
    pub fn split_brain_detector(&self) -> &SplitBrainDetector {
        &self.split_brain_detector
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_failover_config_default() {
        let config = FailoverConfig::default();
        assert_eq!(config.check_interval, Duration::from_secs(1));
        assert_eq!(config.failure_threshold, 3);
        assert_eq!(config.lag_threshold, Duration::from_secs(1));
        assert_eq!(config.switch_timeout, Duration::from_secs(30));
    }

    #[test]
    fn test_data_loss_risk_safe() {
        let slave = SlaveInfo {
            name: "slave1".to_string(),
            healthy: true,
            lag: Duration::from_millis(500),
            row_count: 1000,
        };
        let manager = AutoFailoverManager::new(FailoverConfig::default(), "master".to_string());
        let risk = manager.assess_data_loss(&slave);
        assert!(risk.is_safe);
        assert_eq!(risk.estimated_lost_rows, 0);
    }

    #[test]
    fn test_data_loss_risk_unsafe() {
        let slave = SlaveInfo {
            name: "slave1".to_string(),
            healthy: true,
            lag: Duration::from_secs(5),
            row_count: 1000,
        };
        let manager = AutoFailoverManager::new(FailoverConfig::default(), "master".to_string());
        let risk = manager.assess_data_loss(&slave);
        assert!(!risk.is_safe);
        assert!(risk.estimated_lost_rows > 0);
    }

    #[test]
    fn test_auto_failover_after_threshold() {
        let manager = AutoFailoverManager::new(FailoverConfig::default(), "master".to_string());
        manager.add_slave("slave1", Duration::from_millis(100), 1000);
        manager.add_slave("slave2", Duration::from_millis(200), 1000);

        assert!(manager.record_master_failure().is_none());
        assert!(manager.record_master_failure().is_none());

        let result = manager.record_master_failure();
        assert!(result.is_some());
        assert_eq!(manager.current_master(), "slave1");
    }

    #[test]
    fn test_select_best_slave() {
        let manager = AutoFailoverManager::new(FailoverConfig::default(), "master".to_string());
        manager.add_slave("slave1", Duration::from_millis(200), 1000);
        manager.add_slave("slave2", Duration::from_millis(100), 1000);
        manager.add_slave("slave3", Duration::from_millis(300), 1000);

        let best = manager.select_best_slave().unwrap();
        assert_eq!(best.name, "slave2");
    }

    #[test]
    fn test_no_healthy_slave() {
        let manager = AutoFailoverManager::new(FailoverConfig::default(), "master".to_string());
        manager.add_slave("slave1", Duration::from_millis(100), 1000);
        manager.update_slave_health("slave1", false, Duration::from_millis(100));

        assert!(manager.select_best_slave().is_none());
    }

    #[test]
    fn test_lag_too_high() {
        let manager = AutoFailoverManager::new(FailoverConfig::default(), "master".to_string());
        manager.add_slave("slave1", Duration::from_secs(10), 1000);

        assert!(manager.select_best_slave().is_none());
    }

    #[test]
    fn test_manual_failover() {
        let manager = AutoFailoverManager::new(FailoverConfig::default(), "master".to_string());
        manager.add_slave("slave1", Duration::from_millis(100), 1000);

        let result = manager.manual_failover().unwrap();
        assert_eq!(result.new_master, "slave1");
        assert_eq!(result.event.operator, FailoverOperator::Manual);
    }

    #[test]
    fn test_manual_failover_no_slave() {
        let manager = AutoFailoverManager::new(FailoverConfig::default(), "master".to_string());
        assert!(manager.manual_failover().is_err());
    }

    #[test]
    fn test_split_brain_detection() {
        let manager = AutoFailoverManager::new(FailoverConfig::default(), "master".to_string());
        manager.add_slave("slave1", Duration::from_millis(100), 1000);

        let sb = manager.detect_split_brain("master", "slave1");
        assert_eq!(sb, SplitBrainStatus::NoSplitBrain);
    }

    #[test]
    fn test_failover_events_recorded() {
        let manager = AutoFailoverManager::new(FailoverConfig::default(), "master".to_string());
        manager.add_slave("slave1", Duration::from_millis(100), 1000);

        manager.manual_failover().unwrap();

        let events = manager.events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].promoted_slave, "slave1");
        assert_eq!(events[0].old_master, "master");
    }

    #[test]
    fn test_master_success_resets_count() {
        let manager = AutoFailoverManager::new(FailoverConfig::default(), "master".to_string());
        manager.add_slave("slave1", Duration::from_millis(100), 1000);

        manager.record_master_failure();
        manager.record_master_failure();
        manager.record_master_success();

        assert!(manager.record_master_failure().is_none());
        assert!(manager.record_master_failure().is_none());
        let result = manager.record_master_failure();
        assert!(result.is_some());
    }

    #[test]
    fn test_failover_error_display() {
        let err = FailoverError::NoHealthySlave;
        assert!(err.to_string().contains("no healthy slave"));

        let err = FailoverError::PromotionFailed {
            slave: "s1".to_string(),
            reason: "test".to_string(),
        };
        assert!(err.to_string().contains("s1"));
    }

    #[test]
    fn test_split_brain_status_variants() {
        let no_sb = SplitBrainStatus::NoSplitBrain;
        let detected = SplitBrainStatus::Detected {
            old_master: "m1".to_string(),
            new_master: "m2".to_string(),
        };
        let resolved = SplitBrainStatus::Resolved;

        assert_ne!(no_sb, detected);
        assert_ne!(detected, resolved);
        assert_ne!(no_sb, resolved);
    }

    #[test]
    fn test_failover_result() {
        let manager = AutoFailoverManager::new(FailoverConfig::default(), "master".to_string());
        manager.add_slave("slave1", Duration::from_millis(50), 1000);
        manager.add_slave("slave2", Duration::from_millis(100), 2000);

        let result = manager.manual_failover().unwrap();
        assert_eq!(result.new_master, "slave1");
        assert!(result.event.data_loss_assessment.is_safe);
        assert_eq!(result.split_brain, SplitBrainStatus::NoSplitBrain);
    }

    #[test]
    fn test_split_brain_after_failover_detected() {
        let manager = AutoFailoverManager::new(FailoverConfig::default(), "master".to_string());
        manager.add_slave("slave1", Duration::from_millis(100), 1000);

        manager.manual_failover().unwrap();
        assert_eq!(manager.current_master(), "slave1");

        let status = manager.check_split_brain("master", true);
        assert_eq!(
            status,
            SplitBrainStatus::Detected {
                old_master: "master".to_string(),
                new_master: "slave1".to_string(),
            }
        );
    }

    #[test]
    fn test_split_brain_not_detected_when_old_master_unhealthy() {
        let manager = AutoFailoverManager::new(FailoverConfig::default(), "master".to_string());
        manager.add_slave("slave1", Duration::from_millis(100), 1000);

        manager.manual_failover().unwrap();

        let status = manager.check_split_brain("master", false);
        assert_eq!(status, SplitBrainStatus::NoSplitBrain);
    }

    #[test]
    fn test_split_brain_resolve_demote() {
        let manager = AutoFailoverManager::new(FailoverConfig::default(), "master".to_string());
        manager.add_slave("slave1", Duration::from_millis(100), 1000);

        manager.manual_failover().unwrap();

        let alert = manager
            .resolve_split_brain("master", DemotionStrategy::DemoteToSlave)
            .unwrap();
        assert_eq!(alert.old_master, "master");
        assert_eq!(alert.new_master, "slave1");
        assert!(alert.message.contains("demoted to slave"));
        assert_eq!(manager.split_brain_status(), SplitBrainStatus::Resolved);
    }

    #[test]
    fn test_split_brain_resolve_isolate() {
        let manager = AutoFailoverManager::new(FailoverConfig::default(), "master".to_string());
        manager.add_slave("slave1", Duration::from_millis(100), 1000);

        manager.manual_failover().unwrap();

        let alert = manager
            .resolve_split_brain("master", DemotionStrategy::Isolate)
            .unwrap();
        assert!(alert.message.contains("isolated"));
    }

    #[test]
    fn test_split_brain_periodic_check() {
        let manager = AutoFailoverManager::new(FailoverConfig::default(), "master".to_string());
        manager.add_slave("slave1", Duration::from_millis(100), 1000);

        manager.manual_failover().unwrap();

        let results = manager.periodic_split_brain_check(|name| name == "master");
        assert_eq!(results.len(), 1);
        match &results[0] {
            SplitBrainStatus::Detected { old_master, .. } => {
                assert_eq!(old_master, "master");
            }
            _ => panic!("expected Detected"),
        }
    }

    #[test]
    fn test_split_brain_periodic_check_after_resolve() {
        let manager = AutoFailoverManager::new(FailoverConfig::default(), "master".to_string());
        manager.add_slave("slave1", Duration::from_millis(100), 1000);

        manager.manual_failover().unwrap();
        manager.resolve_split_brain("master", DemotionStrategy::DemoteToSlave);

        let results = manager.periodic_split_brain_check(|_| true);
        assert!(results.is_empty());
    }

    #[test]
    fn test_no_split_brain_when_same_master() {
        let manager = AutoFailoverManager::new(FailoverConfig::default(), "master".to_string());
        manager.add_slave("slave1", Duration::from_millis(100), 1000);

        let status = manager.check_split_brain("master", true);
        assert_eq!(status, SplitBrainStatus::NoSplitBrain);
    }
}
