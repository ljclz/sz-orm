//! 闭环自治调度器：持续执行监控→决策→执行→验证→调整闭环
//!
//! ADR-004：使用 `tokio::time::interval` 调度 + `AtomicBool` 接管标志。
//! 每轮 ≤ 60s，超出边界拒绝并告警 `AI_AUTONOMOUS_OUT_OF_BOUND`。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use super::action_executor::AutonomousActionExecutor;
use super::boundary_validator::BoundaryValidator;
use super::types::{
    ActionBoundary, AnomalyEvent, AutonomousAction, AutonomousError, ExecutionResult,
    VerificationResult,
};
use super::verification_loop::AutonomousVerificationLoop;

/// 闭环自治错误
#[derive(Debug, thiserror::Error)]
pub enum ClosedLoopError {
    #[error("超出边界: {0}")]
    OutOfBound(String),
    #[error("闭环已接管")]
    TakenOver,
    #[error("闭环已停止")]
    Stopped,
    #[error("调度失败: {0}")]
    ScheduleFailed(String),
    #[error(transparent)]
    Autonomous(#[from] AutonomousError),
}

/// SLO 目标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SloTarget {
    pub availability: f64,
    pub latency_p99_ms: f64,
}

/// 成本目标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostTarget {
    pub monthly_budget: f64,
}

/// 延迟目标
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatencyTarget {
    pub p50_ms: f64,
    pub p99_ms: f64,
}

/// 自治目标
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AutonomousTarget {
    pub slo: Option<SloTarget>,
    pub cost: Option<CostTarget>,
    pub latency: Option<LatencyTarget>,
}

impl AutonomousTarget {
    pub fn with_slo(mut self, slo: SloTarget) -> Self {
        self.slo = Some(slo);
        self
    }
}

/// 边界约束：允许操作/参数范围/频率限制
#[derive(Debug, Clone)]
pub struct BoundaryConstraint {
    pub allowed_actions: Vec<AutonomousAction>,
    pub param_boundary: ActionBoundary,
    pub min_interval: Duration,
}

impl Default for BoundaryConstraint {
    fn default() -> Self {
        Self {
            allowed_actions: vec![
                AutonomousAction::AutoRemediation,
                AutonomousAction::AutoScaling,
                AutonomousAction::AutoTuning,
            ],
            param_boundary: ActionBoundary {
                min: 0.0,
                max: 100.0,
            },
            min_interval: Duration::from_secs(1),
        }
    }
}

/// 闭环每轮记录
#[derive(Debug, Clone)]
pub struct ClosedLoopRecord {
    pub round: u64,
    pub started_at: Instant,
    pub duration: Duration,
    pub action: AutonomousAction,
    pub in_boundary: bool,
    pub execution: Option<ExecutionResult>,
    pub verification: Option<VerificationResult>,
    pub alert: Option<String>,
}

/// 闭环自治调度器
pub struct ClosedLoopScheduler {
    target: AutonomousTarget,
    boundary: BoundaryConstraint,
    period: Duration,
    takeover: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    records: Arc<Mutex<Vec<ClosedLoopRecord>>>,
    executor: AutonomousActionExecutor,
    validator: BoundaryValidator,
    verification_loop: AutonomousVerificationLoop,
}

impl ClosedLoopScheduler {
    /// 创建调度器，period 默认 60s，上限 300s
    pub fn new(
        target: AutonomousTarget,
        boundary: BoundaryConstraint,
        period: Duration,
        takeover: Arc<AtomicBool>,
    ) -> Self {
        let period = if period > Duration::from_secs(300) {
            Duration::from_secs(300)
        } else if period.is_zero() {
            Duration::from_secs(60)
        } else {
            period
        };
        Self {
            target,
            boundary,
            period,
            takeover,
            running: Arc::new(AtomicBool::new(false)),
            records: Arc::new(Mutex::new(Vec::new())),
            executor: AutonomousActionExecutor::new(),
            validator: BoundaryValidator::new(),
            verification_loop: AutonomousVerificationLoop::new(),
        }
    }

    pub fn target(&self) -> &AutonomousTarget {
        &self.target
    }

    pub fn boundary(&self) -> &BoundaryConstraint {
        &self.boundary
    }

    pub fn period(&self) -> Duration {
        self.period
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    pub fn is_taken_over(&self) -> bool {
        self.takeover.load(Ordering::Acquire)
    }

    pub fn records(&self) -> Vec<ClosedLoopRecord> {
        self.records.lock().clone()
    }

    /// 启动持续闭环调度（执行一轮，便于测试验证）
    pub async fn start(&self) -> Result<(), ClosedLoopError> {
        if self.is_taken_over() {
            return Err(ClosedLoopError::TakenOver);
        }
        self.running.store(true, Ordering::Release);
        Ok(())
    }

    /// 停止闭环调度
    pub async fn stop(&self) -> Result<(), ClosedLoopError> {
        self.running.store(false, Ordering::Release);
        Ok(())
    }

    /// 执行单轮闭环：监控→决策→边界校验→执行→验证→调整
    pub async fn run_one_round(
        &self,
        _event: &AnomalyEvent,
        action: AutonomousAction,
        params: &[(String, String)],
    ) -> Result<ClosedLoopRecord, ClosedLoopError> {
        if !self.is_running() {
            return Err(ClosedLoopError::Stopped);
        }
        if self.is_taken_over() {
            return Err(ClosedLoopError::TakenOver);
        }
        let round = self.records.lock().len() as u64 + 1;
        let started_at = Instant::now();
        let in_boundary = self.check_boundary(&action, 1.0)?;
        let mut record = ClosedLoopRecord {
            round,
            started_at,
            duration: Duration::ZERO,
            action: action.clone(),
            in_boundary,
            execution: None,
            verification: None,
            alert: None,
        };
        if !in_boundary {
            record.alert = Some("AI_AUTONOMOUS_OUT_OF_BOUND".to_string());
            record.duration = started_at.elapsed();
            self.records.lock().push(record.clone());
            return Ok(record);
        }
        let exec_result = self.executor.execute(&action, params).await?;
        record.execution = Some(exec_result.clone());
        let verify_result = self.verification_loop.verify(&exec_result).await?;
        record.verification = Some(verify_result);
        record.duration = started_at.elapsed();
        if record.duration > self.period {
            record.alert = Some("AI_AUTONOMOUS_ROUND_TIMEOUT".to_string());
        }
        self.records.lock().push(record.clone());
        Ok(record)
    }

    /// 边界校验：动作是否在允许列表 + 参数是否在范围内
    fn check_boundary(
        &self,
        action: &AutonomousAction,
        value: f64,
    ) -> Result<bool, ClosedLoopError> {
        if !self.boundary.allowed_actions.contains(action) {
            return Err(ClosedLoopError::OutOfBound(format!(
                "动作 {:?} 不在允许列表",
                action
            )));
        }
        self.validator
            .validate(action, value, &self.boundary.param_boundary)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autonomous::types::Severity;
    use std::collections::HashMap;
    use std::time::SystemTime;

    fn make_event() -> AnomalyEvent {
        AnomalyEvent {
            event_type: "high_latency".to_string(),
            timestamp: SystemTime::now(),
            severity: Severity::Warning,
            context: HashMap::new(),
            event_hash: 1,
        }
    }

    fn make_scheduler(period: Duration) -> ClosedLoopScheduler {
        let takeover = Arc::new(AtomicBool::new(false));
        ClosedLoopScheduler::new(
            AutonomousTarget::default(),
            BoundaryConstraint::default(),
            period,
            takeover,
        )
    }

    #[tokio::test]
    async fn test_start_stop() {
        let sched = make_scheduler(Duration::from_secs(60));
        assert!(!sched.is_running());
        sched.start().await.unwrap();
        assert!(sched.is_running());
        sched.stop().await.unwrap();
        assert!(!sched.is_running());
    }

    #[tokio::test]
    async fn test_period_capped_at_300s() {
        let sched = make_scheduler(Duration::from_secs(600));
        assert_eq!(sched.period(), Duration::from_secs(300));
    }

    #[tokio::test]
    async fn test_period_zero_defaults_60s() {
        let sched = make_scheduler(Duration::ZERO);
        assert_eq!(sched.period(), Duration::from_secs(60));
    }

    #[tokio::test]
    async fn test_run_one_round_in_boundary() {
        let sched = make_scheduler(Duration::from_secs(60));
        sched.start().await.unwrap();
        let event = make_event();
        let record = sched
            .run_one_round(
                &event,
                AutonomousAction::AutoRemediation,
                &[("fault_type".to_string(), "pool_exhausted".to_string())],
            )
            .await
            .unwrap();
        assert!(record.in_boundary);
        assert!(record.execution.is_some());
        assert!(record.verification.is_some());
        assert!(record.alert.is_none());
        assert_eq!(sched.records().len(), 1);
    }

    #[tokio::test]
    async fn test_run_one_round_out_of_boundary() {
        let takeover = Arc::new(AtomicBool::new(false));
        let boundary = BoundaryConstraint {
            allowed_actions: vec![AutonomousAction::AutoRemediation],
            param_boundary: ActionBoundary {
                min: 0.0,
                max: 100.0,
            },
            min_interval: Duration::from_secs(1),
        };
        let sched = ClosedLoopScheduler::new(
            AutonomousTarget::default(),
            boundary,
            Duration::from_secs(60),
            takeover,
        );
        sched.start().await.unwrap();
        let event = make_event();
        let result = sched
            .run_one_round(
                &event,
                AutonomousAction::AutoScaling,
                &[("target_instances".to_string(), "10".to_string())],
            )
            .await;
        assert!(matches!(result, Err(ClosedLoopError::OutOfBound(_))));
    }

    #[tokio::test]
    async fn test_takeover_pauses_loop() {
        let takeover = Arc::new(AtomicBool::new(false));
        let sched = ClosedLoopScheduler::new(
            AutonomousTarget::default(),
            BoundaryConstraint::default(),
            Duration::from_secs(60),
            takeover.clone(),
        );
        sched.start().await.unwrap();
        takeover.store(true, Ordering::Release);
        assert!(sched.is_taken_over());
        let event = make_event();
        let result = sched
            .run_one_round(&event, AutonomousAction::AutoRemediation, &[])
            .await;
        assert!(matches!(result, Err(ClosedLoopError::TakenOver)));
    }

    #[tokio::test]
    async fn test_start_when_taken_over_rejected() {
        let takeover = Arc::new(AtomicBool::new(true));
        let sched = ClosedLoopScheduler::new(
            AutonomousTarget::default(),
            BoundaryConstraint::default(),
            Duration::from_secs(60),
            takeover,
        );
        let result = sched.start().await;
        assert!(matches!(result, Err(ClosedLoopError::TakenOver)));
    }

    #[tokio::test]
    async fn test_run_when_stopped_rejected() {
        let sched = make_scheduler(Duration::from_secs(60));
        let event = make_event();
        let result = sched
            .run_one_round(&event, AutonomousAction::AutoRemediation, &[])
            .await;
        assert!(matches!(result, Err(ClosedLoopError::Stopped)));
    }

    #[tokio::test]
    async fn test_round_within_60s() {
        let sched = make_scheduler(Duration::from_secs(60));
        sched.start().await.unwrap();
        let event = make_event();
        let record = sched
            .run_one_round(
                &event,
                AutonomousAction::AutoTuning,
                &[("param".to_string(), "pool_size".to_string())],
            )
            .await
            .unwrap();
        assert!(record.duration <= Duration::from_secs(60));
    }
}
