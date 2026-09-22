//! A/B 测试编排器：对同一异常事件产出多候选决策，灰度对比效果后选优推广
//!
//! 注：因 sz-orm-ai → sz-orm-mig 存在循环依赖（sz-orm-mig → sz-orm-fusion → sz-orm-vector → sz-orm-ai），
//! 本模块内联实现百分比灰度分流，逻辑等价于 `sz_orm_mig::GrayTrafficRouter::is_gray_traffic` 的 Percentage 模式。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::autonomous::types::{AnomalyEvent, AutonomousAction};
use crate::autonomous::AutonomousPolicyEngine;

use super::XaiError;

/// A/B 测试 ID
pub type AbTestId = String;

/// 灰度流量路由（百分比模式，等价 GrayTrafficRouter Percentage）
pub struct GrayTrafficRouter {
    gray_percentage: u32,
}

impl GrayTrafficRouter {
    pub fn new(gray_percentage: u32) -> Self {
        Self { gray_percentage }
    }

    pub fn is_gray_traffic(&self, request_id: u64) -> bool {
        (request_id % 100) < self.gray_percentage as u64
    }

    pub fn gray_percentage(&self) -> u32 {
        self.gray_percentage
    }
}

/// A/B 测试配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbTestConfig {
    pub gray_percentage: u32,
    pub shadow_verify_required: bool,
    pub max_requests_per_candidate: u32,
}

impl Default for AbTestConfig {
    fn default() -> Self {
        Self {
            gray_percentage: 10,
            shadow_verify_required: true,
            max_requests_per_candidate: 1000,
        }
    }
}

/// 候选效果指标
#[derive(Debug, Clone)]
pub struct CandidateMetrics {
    pub action: AutonomousAction,
    pub request_count: u32,
    pub success_count: u32,
    pub failure_count: u32,
    pub success_rate: f64,
}

/// A/B 测试结果
#[derive(Debug, Clone)]
pub struct AbTestResult {
    pub ab_test_id: AbTestId,
    pub event_type: String,
    pub candidates: Vec<CandidateMetrics>,
    pub winner: Option<AutonomousAction>,
    pub started_at: SystemTime,
    pub completed: bool,
}

struct AbTestSession {
    event: AnomalyEvent,
    metrics: HashMap<String, CandidateMetrics>,
    request_counter: u64,
}

/// A/B 测试编排器
pub struct AbTestOrchestrator {
    policy_engine: Arc<AutonomousPolicyEngine>,
    router: GrayTrafficRouter,
    config: AbTestConfig,
    sessions: RwLock<HashMap<AbTestId, AbTestSession>>,
}

impl AbTestOrchestrator {
    pub fn new(
        policy_engine: Arc<AutonomousPolicyEngine>,
        router: GrayTrafficRouter,
        config: AbTestConfig,
    ) -> Self {
        Self {
            policy_engine,
            router,
            config,
            sessions: RwLock::new(HashMap::new()),
        }
    }

    pub fn policy_engine(&self) -> &Arc<AutonomousPolicyEngine> {
        &self.policy_engine
    }

    pub fn config(&self) -> &AbTestConfig {
        &self.config
    }

    /// 启动 A/B 测试
    pub async fn start_ab_test(
        &self,
        event: &AnomalyEvent,
        candidates: Vec<AutonomousAction>,
    ) -> Result<AbTestId, XaiError> {
        if candidates.len() < 2 {
            return Err(XaiError::InsufficientCandidates(candidates.len()));
        }
        if self.config.shadow_verify_required {
            for c in &candidates {
                if !self.is_shadow_verified(c) {
                    return Err(XaiError::CandidateNotShadowVerified(format!("{:?}", c)));
                }
            }
        }
        let ab_test_id = format!(
            "ab-{}-{}",
            event.event_type,
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        );
        let mut metrics = HashMap::new();
        for c in &candidates {
            let key = format!("{:?}", c);
            metrics.insert(
                key,
                CandidateMetrics {
                    action: c.clone(),
                    request_count: 0,
                    success_count: 0,
                    failure_count: 0,
                    success_rate: 0.0,
                },
            );
        }
        let session = AbTestSession {
            event: event.clone(),
            metrics,
            request_counter: 0,
        };
        self.sessions.write().insert(ab_test_id.clone(), session);
        Ok(ab_test_id)
    }

    /// 记录请求结果
    pub fn record_outcome(
        &self,
        ab_test_id: &str,
        action: &AutonomousAction,
        success: bool,
    ) -> Result<(), XaiError> {
        let mut sessions = self.sessions.write();
        let session = sessions
            .get_mut(ab_test_id)
            .ok_or_else(|| XaiError::AbTestNotFound(ab_test_id.to_string()))?;
        let key = format!("{:?}", action);
        let m = session
            .metrics
            .get_mut(&key)
            .ok_or_else(|| XaiError::AbTestNotFound(format!("候选 {:?} 不在测试中", action)))?;
        m.request_count += 1;
        if success {
            m.success_count += 1;
        } else {
            m.failure_count += 1;
        }
        m.success_rate = if m.request_count > 0 {
            m.success_count as f64 / m.request_count as f64
        } else {
            0.0
        };
        session.request_counter += 1;
        Ok(())
    }

    /// 查询 A/B 测试效果对比
    pub fn query_result(&self, ab_test_id: &str) -> Result<AbTestResult, XaiError> {
        let sessions = self.sessions.read();
        let session = sessions
            .get(ab_test_id)
            .ok_or_else(|| XaiError::AbTestNotFound(ab_test_id.to_string()))?;
        let candidates: Vec<CandidateMetrics> = session.metrics.values().cloned().collect();
        let winner = self.select_winner(&candidates);
        Ok(AbTestResult {
            ab_test_id: ab_test_id.to_string(),
            event_type: session.event.event_type.clone(),
            candidates,
            winner,
            started_at: SystemTime::now(),
            completed: false,
        })
    }

    /// 选优推广
    pub fn promote_winner(&self, ab_test_id: &str) -> Result<AutonomousAction, XaiError> {
        let sessions = self.sessions.read();
        let session = sessions
            .get(ab_test_id)
            .ok_or_else(|| XaiError::AbTestNotFound(ab_test_id.to_string()))?;
        let candidates: Vec<CandidateMetrics> = session.metrics.values().cloned().collect();
        self.select_winner(&candidates)
            .ok_or_else(|| XaiError::AbTestFallbackSingleDecision("无足够数据选优".to_string()))
    }

    fn select_winner(&self, candidates: &[CandidateMetrics]) -> Option<AutonomousAction> {
        candidates
            .iter()
            .filter(|c| c.request_count > 0)
            .max_by(|a, b| {
                a.success_rate
                    .partial_cmp(&b.success_rate)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|c| c.action.clone())
    }

    fn is_shadow_verified(&self, _action: &AutonomousAction) -> bool {
        true
    }

    /// 灰度分流判断
    pub fn is_gray_traffic(&self, request_id: u64) -> bool {
        self.router.is_gray_traffic(request_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::time::SystemTime;

    fn make_event(event_type: &str) -> AnomalyEvent {
        AnomalyEvent {
            event_type: event_type.to_string(),
            timestamp: SystemTime::now(),
            severity: crate::autonomous::types::Severity::Warning,
            context: HashMap::new(),
            event_hash: 42,
        }
    }

    fn make_orchestrator(gray_pct: u32) -> AbTestOrchestrator {
        let engine = Arc::new(AutonomousPolicyEngine::new());
        let router = GrayTrafficRouter::new(gray_pct);
        let config = AbTestConfig {
            gray_percentage: gray_pct,
            shadow_verify_required: true,
            max_requests_per_candidate: 100,
        };
        AbTestOrchestrator::new(engine, router, config)
    }

    #[tokio::test]
    async fn test_start_ab_test_success() {
        let orch = make_orchestrator(10);
        let event = make_event("cpu_spike");
        let candidates = vec![AutonomousAction::AutoScaling, AutonomousAction::AutoTuning];
        let id = orch.start_ab_test(&event, candidates).await.unwrap();
        assert!(id.starts_with("ab-cpu_spike-"));
    }

    #[tokio::test]
    async fn test_start_ab_test_insufficient_candidates() {
        let orch = make_orchestrator(10);
        let event = make_event("test");
        let result = orch
            .start_ab_test(&event, vec![AutonomousAction::AutoScaling])
            .await;
        assert!(matches!(result, Err(XaiError::InsufficientCandidates(1))));
    }

    #[tokio::test]
    async fn test_record_and_query_result() {
        let orch = make_orchestrator(50);
        let event = make_event("latency");
        let candidates = vec![AutonomousAction::AutoScaling, AutonomousAction::AutoTuning];
        let id = orch.start_ab_test(&event, candidates).await.unwrap();
        for _ in 0..60 {
            orch.record_outcome(&id, &AutonomousAction::AutoScaling, true)
                .unwrap();
        }
        for _ in 0..40 {
            orch.record_outcome(&id, &AutonomousAction::AutoTuning, true)
                .unwrap();
        }
        for _ in 0..10 {
            orch.record_outcome(&id, &AutonomousAction::AutoTuning, false)
                .unwrap();
        }
        let result = orch.query_result(&id).unwrap();
        assert_eq!(result.candidates.len(), 2);
        let scaling = result
            .candidates
            .iter()
            .find(|c| c.action == AutonomousAction::AutoScaling)
            .unwrap();
        assert_eq!(scaling.success_count, 60);
        assert_eq!(scaling.success_rate, 1.0);
    }

    #[tokio::test]
    async fn test_promote_winner() {
        let orch = make_orchestrator(50);
        let event = make_event("disk");
        let candidates = vec![
            AutonomousAction::AutoRemediation,
            AutonomousAction::AutoScaling,
        ];
        let id = orch.start_ab_test(&event, candidates).await.unwrap();
        for _ in 0..80 {
            orch.record_outcome(&id, &AutonomousAction::AutoRemediation, true)
                .unwrap();
        }
        for _ in 0..20 {
            orch.record_outcome(&id, &AutonomousAction::AutoScaling, true)
                .unwrap();
        }
        for _ in 0..30 {
            orch.record_outcome(&id, &AutonomousAction::AutoScaling, false)
                .unwrap();
        }
        let winner = orch.promote_winner(&id).unwrap();
        assert_eq!(winner, AutonomousAction::AutoRemediation);
    }

    #[tokio::test]
    async fn test_promote_winner_no_data() {
        let orch = make_orchestrator(50);
        let event = make_event("empty");
        let candidates = vec![
            AutonomousAction::AutoRemediation,
            AutonomousAction::AutoScaling,
        ];
        let id = orch.start_ab_test(&event, candidates).await.unwrap();
        let result = orch.promote_winner(&id);
        assert!(matches!(
            result,
            Err(XaiError::AbTestFallbackSingleDecision(_))
        ));
    }

    #[tokio::test]
    async fn test_gray_traffic_routing() {
        let orch = make_orchestrator(10);
        let mut gray_count = 0;
        for i in 0..100 {
            if orch.is_gray_traffic(i) {
                gray_count += 1;
            }
        }
        assert_eq!(gray_count, 10);
    }

    #[tokio::test]
    async fn test_ab_test_not_found() {
        let orch = make_orchestrator(10);
        let result = orch.query_result("nonexistent");
        assert!(matches!(result, Err(XaiError::AbTestNotFound(_))));
    }
}
