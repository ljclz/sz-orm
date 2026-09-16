//! 查询自调优闭环入口（`query-auto-tuning` feature）
//!
//! [`AutoTuningGateway`] 编排完整闭环：
//! 统计 → 推荐 → 审批 → 应用 → 监控 → 回滚。
//!
//! 依赖：`OptimizationAdvisor`（规则引擎）+ `JoinReorderAdvisor`（JOIN 重排）
//! + `ApprovalGate`（审批门控）+ `TuningCooldown`（冷却期）
//! + `TuningChangeAuditor`（变更审计）+ `FeedbackLoop`（反馈循环）。

use std::sync::Arc;

use crate::advisor::OptimizationAdvisor;
use crate::suggestion::OptimizationSuggestion;
use sz_orm_adaptive::stats::QueryStats;
use sz_orm_explain::ExplainPlan;

use super::approval_gate::{ApprovalGate, ApprovalGateConfig};
use super::change_auditor::{PlanSnapshot, TuningChangeAuditor};
use super::cooldown::{CooldownConfig, TuningCooldown};
use super::feedback_loop::FeedbackLoop;
use super::join_reorder::JoinReorderAdvisor;

/// 调优动作类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuningAction {
    /// 自动应用
    Applied,
    /// 跳过（冷却期内或冲突）
    Skipped(String),
    /// 延迟（等待审批）
    PendingApproval(String),
    /// 回滚
    RolledBack(String),
    /// 无建议
    NoSuggestion,
}

/// 调优建议（含元数据）
#[derive(Debug, Clone)]
pub struct TuningSuggestion {
    /// 内部建议
    pub inner: OptimizationSuggestion,
    /// 审批 ID（若提交审批）
    pub approval_id: Option<String>,
    /// 置信度
    pub confidence: f64,
    /// 是否为 DDL
    pub is_ddl: bool,
}

/// 闭环执行报告
#[derive(Debug, Clone)]
pub struct TuningLoopReport {
    /// 查询标识
    pub query_key: String,
    /// 生成建议数
    pub suggestion_count: usize,
    /// 执行动作列表
    pub actions: Vec<TuningAction>,
    /// 是否进入冷却期
    pub entered_cooldown: bool,
    /// 是否触发回滚
    pub triggered_rollback: bool,
}

/// 自调优配置
#[derive(Debug, Clone)]
pub struct AutoTuningConfig {
    /// 自动应用的置信度阈值（默认 0.8）
    pub auto_apply_threshold: f64,
    /// 审批门控配置
    pub approval: ApprovalGateConfig,
    /// 冷却期配置
    pub cooldown: CooldownConfig,
}

impl Default for AutoTuningConfig {
    fn default() -> Self {
        Self {
            auto_apply_threshold: 0.8,
            approval: ApprovalGateConfig::default(),
            cooldown: CooldownConfig::default(),
        }
    }
}

/// 查询自调优闭环入口
pub struct AutoTuningGateway {
    advisor: Arc<OptimizationAdvisor>,
    join_reorder: JoinReorderAdvisor,
    approval_gate: ApprovalGate,
    cooldown: TuningCooldown,
    auditor: TuningChangeAuditor,
    feedback: FeedbackLoop,
    config: AutoTuningConfig,
}

impl AutoTuningGateway {
    pub fn new(advisor: Arc<OptimizationAdvisor>) -> Self {
        Self {
            advisor,
            join_reorder: JoinReorderAdvisor::new(),
            approval_gate: ApprovalGate::with_defaults(),
            cooldown: TuningCooldown::with_defaults(),
            auditor: TuningChangeAuditor::new(),
            feedback: FeedbackLoop::new(),
            config: AutoTuningConfig::default(),
        }
    }

    pub fn with_config(mut self, config: AutoTuningConfig) -> Self {
        self.approval_gate = ApprovalGate::new(config.approval.clone());
        self.cooldown = TuningCooldown::new(config.cooldown.clone());
        self.config = config;
        self
    }

    pub fn with_join_reorder(mut self, advisor: JoinReorderAdvisor) -> Self {
        self.join_reorder = advisor;
        self
    }

    pub fn config(&self) -> &AutoTuningConfig {
        &self.config
    }

    pub fn cooldown(&self) -> &TuningCooldown {
        &self.cooldown
    }

    pub fn cooldown_mut(&mut self) -> &mut TuningCooldown {
        &mut self.cooldown
    }

    pub fn approval_gate(&self) -> &ApprovalGate {
        &self.approval_gate
    }

    pub fn approval_gate_mut(&mut self) -> &mut ApprovalGate {
        &mut self.approval_gate
    }

    pub fn auditor(&self) -> &TuningChangeAuditor {
        &self.auditor
    }

    pub fn feedback(&self) -> &FeedbackLoop {
        &self.feedback
    }

    pub fn feedback_mut(&mut self) -> &mut FeedbackLoop {
        &mut self.feedback
    }

    /// 生成调优建议
    pub fn suggest(
        &self,
        _query_key: &str,
        plan: Option<&ExplainPlan>,
        stats: Option<&QueryStats>,
    ) -> Vec<TuningSuggestion> {
        let raw = self.advisor.suggest(plan, stats, None);
        raw.into_iter()
            .map(|s| TuningSuggestion {
                is_ddl: s.suggestion_type.is_ddl(),
                confidence: s.confidence,
                approval_id: None,
                inner: s,
            })
            .collect()
    }

    /// 运行自调优闭环
    ///
    /// 编排：冷却检查 → 建议生成 → 审批/自动应用 → 变更审计 → 反馈注册
    pub fn run_loop(
        &mut self,
        query_key: &str,
        plan: Option<&ExplainPlan>,
        stats: Option<&QueryStats>,
        before_ms: f64,
        after_ms: f64,
    ) -> TuningLoopReport {
        let mut actions = Vec::new();
        let mut entered_cooldown = false;
        let mut triggered_rollback = false;

        if self.cooldown.is_in_cooldown(query_key) {
            return TuningLoopReport {
                query_key: query_key.to_string(),
                suggestion_count: 0,
                actions: vec![TuningAction::Skipped("in cooldown".into())],
                entered_cooldown: false,
                triggered_rollback: false,
            };
        }

        let suggestions = self.suggest(query_key, plan, stats);
        let suggestion_count = suggestions.len();

        if suggestions.is_empty() {
            actions.push(TuningAction::NoSuggestion);
        }

        for s in &suggestions {
            if s.confidence >= self.config.auto_apply_threshold {
                self.auditor.record_apply(
                    query_key,
                    s.inner.clone(),
                    plan.map(|p| PlanSnapshot {
                        scan_type: format!("{:?}", p.scan_type),
                        table: p.table.clone(),
                        estimated_rows: p.rows,
                    }),
                    None,
                    before_ms,
                    after_ms,
                    format!("auto-apply (confidence={:.2})", s.confidence),
                );
                self.feedback.register_applied(query_key, s.inner.clone());
                self.feedback.record_sample(query_key, before_ms, after_ms);
                actions.push(TuningAction::Applied);
                entered_cooldown = true;
            } else {
                let id = self.approval_gate.submit(s.inner.clone());
                actions.push(TuningAction::PendingApproval(id));
            }
        }

        if entered_cooldown {
            self.cooldown.enter(query_key);
        }

        if self.feedback.should_rollback(query_key) {
            if let Ok(reason) = self.feedback.auto_rollback(query_key) {
                self.auditor
                    .record_rollback(query_key, suggestions[0].inner.clone(), &reason);
                actions.push(TuningAction::RolledBack(reason));
                triggered_rollback = true;
                entered_cooldown = false;
            }
        }

        TuningLoopReport {
            query_key: query_key.to_string(),
            suggestion_count,
            actions,
            entered_cooldown,
            triggered_rollback,
        }
    }

    /// 手动回滚
    pub fn rollback(&mut self, query_key: &str) -> Result<String, String> {
        let history = self.auditor.history(query_key);
        let last_apply = history
            .iter()
            .rev()
            .find(|r| r.kind == super::change_auditor::ChangeKind::Apply);
        let Some(record) = last_apply else {
            return Err("no applied suggestion to rollback".into());
        };
        let suggestion = record.suggestion.clone();
        let reason = format!("manual rollback: {}", suggestion.description);
        self.auditor.record_rollback(query_key, suggestion, &reason);
        self.cooldown.exit(query_key);
        Ok(reason)
    }

    /// 处理审批通过的建议
    pub fn apply_approved(
        &mut self,
        query_key: &str,
        approval_id: &str,
        before_ms: f64,
        after_ms: f64,
    ) -> Result<TuningAction, String> {
        if !self.approval_gate.is_approved(approval_id) {
            return Err("suggestion not approved".into());
        }
        let suggestion = self
            .approval_gate
            .suggestion(approval_id)
            .ok_or("approval not found")?
            .clone();
        self.auditor.record_apply(
            query_key,
            suggestion.clone(),
            None,
            None,
            before_ms,
            after_ms,
            "manual approved apply",
        );
        self.feedback.register_applied(query_key, suggestion);
        self.feedback.record_sample(query_key, before_ms, after_ms);
        self.cooldown.enter(query_key);
        Ok(TuningAction::Applied)
    }

    /// 检查反馈循环并自动回滚（独立于 run_loop 调用）
    ///
    /// 用于在 run_loop 之后持续监控性能，检测回归时触发回滚。
    pub fn check_and_rollback(&mut self, query_key: &str) -> Option<TuningAction> {
        if !self.feedback.should_rollback(query_key) {
            return None;
        }
        let history = self.auditor.history(query_key);
        let last_apply = history
            .iter()
            .rev()
            .find(|r| r.kind == super::change_auditor::ChangeKind::Apply);
        if let Ok(reason) = self.feedback.auto_rollback(query_key) {
            if let Some(record) = last_apply {
                self.auditor
                    .record_rollback(query_key, record.suggestion.clone(), &reason);
            }
            self.cooldown.exit(query_key);
            return Some(TuningAction::RolledBack(reason));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::super::ChangeKind;
    use super::*;
    use sz_orm_explain::{ExplainPlan, ScanType};

    fn advisor() -> Arc<OptimizationAdvisor> {
        Arc::new(OptimizationAdvisor::with_defaults())
    }

    fn full_scan_plan() -> ExplainPlan {
        ExplainPlan {
            scan_type: ScanType::FullTable,
            table: "users".into(),
            index: None,
            rows: 50000,
            extra: vec![],
        }
    }

    #[test]
    fn run_loop_generates_and_applies_high_confidence() {
        let mut gw = AutoTuningGateway::new(advisor());
        let plan = full_scan_plan();
        let report = gw.run_loop("q1", Some(&plan), None, 500.0, 50.0);
        assert!(report.suggestion_count > 0);
        assert!(report.actions.contains(&TuningAction::Applied));
        assert!(report.entered_cooldown);
    }

    #[test]
    fn run_loop_skips_during_cooldown() {
        let mut gw = AutoTuningGateway::new(advisor());
        let plan = full_scan_plan();
        let _first = gw.run_loop("q1", Some(&plan), None, 500.0, 50.0);
        let second = gw.run_loop("q1", Some(&plan), None, 500.0, 50.0);
        assert!(second
            .actions
            .iter()
            .any(|a| matches!(a, TuningAction::Skipped(_))));
    }

    #[test]
    fn run_loop_no_suggestion_without_plan() {
        let mut gw = AutoTuningGateway::new(advisor());
        let report = gw.run_loop("q1", None, None, 100.0, 100.0);
        assert_eq!(report.suggestion_count, 0);
        assert!(report.actions.contains(&TuningAction::NoSuggestion));
    }

    #[test]
    fn low_confidence_goes_to_approval() {
        let config = AutoTuningConfig {
            auto_apply_threshold: 0.99,
            ..Default::default()
        };
        let mut gw = AutoTuningGateway::new(advisor()).with_config(config);
        let plan = full_scan_plan();
        let report = gw.run_loop("q1", Some(&plan), None, 500.0, 50.0);
        assert!(report
            .actions
            .iter()
            .any(|a| matches!(a, TuningAction::PendingApproval(_))));
        assert!(gw.approval_gate().pending_count() > 0);
    }

    #[test]
    fn rollback_after_apply() {
        let mut gw = AutoTuningGateway::new(advisor());
        let plan = full_scan_plan();
        let _ = gw.run_loop("q1", Some(&plan), None, 500.0, 50.0);
        let result = gw.rollback("q1");
        assert!(result.is_ok());
    }

    #[test]
    fn rollback_without_apply_fails() {
        let mut gw = AutoTuningGateway::new(advisor());
        let result = gw.rollback("q1");
        assert!(result.is_err());
    }

    #[test]
    fn apply_approved_after_manual_approval() {
        let config = AutoTuningConfig {
            auto_apply_threshold: 0.99,
            ..Default::default()
        };
        let mut gw = AutoTuningGateway::new(advisor()).with_config(config);
        let plan = full_scan_plan();
        let report = gw.run_loop("q1", Some(&plan), None, 500.0, 50.0);
        let approval_id = report
            .actions
            .into_iter()
            .find_map(|a| match a {
                TuningAction::PendingApproval(id) => Some(id),
                _ => None,
            })
            .expect("should have pending approval");
        assert!(gw.approval_gate_mut().approve(&approval_id));
        let result = gw.apply_approved("q1", &approval_id, 500.0, 50.0);
        assert!(result.is_ok());
    }

    #[test]
    fn auto_rollback_on_regression() {
        let mut gw = AutoTuningGateway::new(advisor());
        let plan = full_scan_plan();
        let _ = gw.run_loop("q1", Some(&plan), None, 100.0, 200.0);
        gw.feedback_mut().record_sample("q1", 100.0, 250.0);
        gw.feedback_mut().record_sample("q1", 100.0, 300.0);
        let action = gw.check_and_rollback("q1");
        assert!(matches!(action, Some(TuningAction::RolledBack(_))));
        let history = gw.auditor().history("q1");
        assert!(history.iter().any(|r| r.kind == ChangeKind::Rollback));
    }
}
