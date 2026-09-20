//! 灰度发布编排器

use std::sync::Arc;
use std::time::Instant;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

/// 流量切换方式
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrafficSwitchMethod {
    Percentage,
    Header,
    Cookie,
}

/// 健康判定条件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthJudgeCondition {
    pub metric: String,
    pub threshold: f64,
}

/// 灰度发布配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrayReleaseConfig {
    pub name: String,
    pub gray_percentage: u32,
    pub canary_instance_count: u32,
    pub health_judge_conditions: Vec<HealthJudgeCondition>,
    pub rollback_threshold: f64,
    pub traffic_switch: TrafficSwitchMethod,
}

impl Default for GrayReleaseConfig {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            gray_percentage: 10,
            canary_instance_count: 1,
            health_judge_conditions: vec![HealthJudgeCondition {
                metric: "error_rate".to_string(),
                threshold: 0.01,
            }],
            rollback_threshold: 0.05,
            traffic_switch: TrafficSwitchMethod::Percentage,
        }
    }
}

/// 发布状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReleaseStatus {
    Pending,
    Running,
    Paused,
    Completed,
    RolledBack,
}

/// 灰度进度
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrayProgress {
    pub release_id: String,
    pub status: ReleaseStatus,
    pub current_percentage: u32,
    pub canary_healthy: bool,
    pub started_at: i64,
}

/// 回滚报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RollbackReport {
    pub release_id: String,
    pub reason: String,
    pub rolled_back_at: i64,
    pub success: bool,
}

/// 灰度发布编排器
pub struct GrayReleaseOrchestrator {
    config: GrayReleaseConfig,
    progress: Arc<RwLock<GrayProgress>>,
    started: bool,
}

impl GrayReleaseOrchestrator {
    pub fn new(config: GrayReleaseConfig) -> Self {
        Self {
            config,
            progress: Arc::new(RwLock::new(GrayProgress {
                release_id: String::new(),
                status: ReleaseStatus::Pending,
                current_percentage: 0,
                canary_healthy: false,
                started_at: 0,
            })),
            started: false,
        }
    }

    pub fn start_release(&mut self, release_id: &str) -> GrayProgress {
        self.started = true;
        let progress = GrayProgress {
            release_id: release_id.to_string(),
            status: ReleaseStatus::Running,
            current_percentage: self.config.gray_percentage,
            canary_healthy: true,
            started_at: Instant::now().elapsed().as_millis() as i64,
        };
        *self.progress.write() = progress.clone();
        progress
    }

    pub fn advance(&self, healthy: bool) -> Result<GrayProgress, String> {
        let mut progress = self.progress.write();
        if progress.status != ReleaseStatus::Running {
            return Err(format!("发布状态 {:?} 不允许推进", progress.status));
        }
        if !healthy {
            return Err("健康判定未通过，不能推进".to_string());
        }
        progress.current_percentage = (progress.current_percentage + 10).min(100);
        if progress.current_percentage >= 100 {
            progress.status = ReleaseStatus::Completed;
        }
        Ok(progress.clone())
    }

    pub fn pause(&self) -> Result<GrayProgress, String> {
        let mut progress = self.progress.write();
        if progress.status != ReleaseStatus::Running {
            return Err(format!("发布状态 {:?} 不允许暂停", progress.status));
        }
        progress.status = ReleaseStatus::Paused;
        Ok(progress.clone())
    }

    pub fn resume(&self) -> Result<GrayProgress, String> {
        let mut progress = self.progress.write();
        if progress.status != ReleaseStatus::Paused {
            return Err(format!("发布状态 {:?} 不允许恢复", progress.status));
        }
        progress.status = ReleaseStatus::Running;
        Ok(progress.clone())
    }

    pub fn force_rollback(&self, reason: &str) -> RollbackReport {
        let mut progress = self.progress.write();
        progress.status = ReleaseStatus::RolledBack;
        RollbackReport {
            release_id: progress.release_id.clone(),
            reason: reason.to_string(),
            rolled_back_at: Instant::now().elapsed().as_millis() as i64,
            success: true,
        }
    }

    pub fn query_progress(&self) -> GrayProgress {
        self.progress.read().clone()
    }

    pub fn config(&self) -> &GrayReleaseConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_start_release() {
        let mut orch = GrayReleaseOrchestrator::new(GrayReleaseConfig::default());
        let progress = orch.start_release("rel-1");
        assert_eq!(progress.status, ReleaseStatus::Running);
        assert_eq!(progress.current_percentage, 10);
    }

    #[test]
    fn test_advance_healthy() {
        let mut orch = GrayReleaseOrchestrator::new(GrayReleaseConfig::default());
        orch.start_release("rel-1");
        let progress = orch.advance(true).unwrap();
        assert_eq!(progress.current_percentage, 20);
    }

    #[test]
    fn test_advance_unhealthy_rejected() {
        let mut orch = GrayReleaseOrchestrator::new(GrayReleaseConfig::default());
        orch.start_release("rel-1");
        assert!(orch.advance(false).is_err());
    }

    #[test]
    fn test_pause_resume() {
        let mut orch = GrayReleaseOrchestrator::new(GrayReleaseConfig::default());
        orch.start_release("rel-1");
        orch.pause().unwrap();
        assert_eq!(orch.query_progress().status, ReleaseStatus::Paused);
        orch.resume().unwrap();
        assert_eq!(orch.query_progress().status, ReleaseStatus::Running);
    }

    #[test]
    fn test_force_rollback() {
        let mut orch = GrayReleaseOrchestrator::new(GrayReleaseConfig::default());
        orch.start_release("rel-1");
        let report = orch.force_rollback("error rate exceeded");
        assert!(report.success);
        assert_eq!(orch.query_progress().status, ReleaseStatus::RolledBack);
    }

    #[test]
    fn test_advance_to_completion() {
        let mut orch = GrayReleaseOrchestrator::new(GrayReleaseConfig::default());
        orch.start_release("rel-1");
        for _ in 0..9 {
            orch.advance(true).unwrap();
        }
        assert_eq!(orch.query_progress().status, ReleaseStatus::Completed);
    }

    #[test]
    fn test_pause_not_running_rejected() {
        let orch = GrayReleaseOrchestrator::new(GrayReleaseConfig::default());
        assert!(orch.pause().is_err());
    }
}
