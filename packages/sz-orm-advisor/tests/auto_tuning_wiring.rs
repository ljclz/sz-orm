//! 查询自调优接线测试（`query-auto-tuning` feature）
//!
//! 验证 `AutoTuningGateway` 端到端闭环：
//! 建议生成 → 自动应用 → 冷却期 → 审批门控 → 反馈监控 → 自动回滚 → 变更审计。
//!
//! 生产调用点证据：
//! - `AutoTuningGateway::run_loop` → `packages/sz-orm-advisor/src/auto_tuning/gateway.rs:189`
//! - `AutoTuningGateway::rollback` → `packages/sz-orm-advisor/src/auto_tuning/gateway.rs:253`
//! - `AutoTuningGateway::check_and_rollback` → `packages/sz-orm-advisor/src/auto_tuning/gateway.rs:301`

use std::sync::Arc;

use sz_orm_advisor::{
    advisor::OptimizationAdvisor, suggestion::SuggestionType, AutoTuningConfig, AutoTuningGateway,
    ChangeKind, JoinReorderAdvisor, JoinTable, TableStats, TuningAction,
};
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

/// W1: AutoTuningGateway 生成建议并自动应用高置信度建议
#[test]
fn wiring_gateway_generates_and_applies() {
    let mut gw = AutoTuningGateway::new(advisor());
    let plan = full_scan_plan();
    let report = gw.run_loop("q1", Some(&plan), None, 500.0, 50.0);
    assert!(report.suggestion_count > 0, "should generate suggestions");
    assert!(
        report.actions.contains(&TuningAction::Applied),
        "should auto-apply high confidence"
    );
    assert!(report.entered_cooldown, "should enter cooldown after apply");
    let summary = gw.auditor().summary();
    assert!(summary.apply_count > 0, "auditor should record apply");
}

/// W2: 冷却期内暂缓新建议
#[test]
fn wiring_cooldown_blocks_during_cooldown() {
    let mut gw = AutoTuningGateway::new(advisor());
    let plan = full_scan_plan();
    let first = gw.run_loop("q1", Some(&plan), None, 500.0, 50.0);
    assert!(first.entered_cooldown);
    let second = gw.run_loop("q1", Some(&plan), None, 500.0, 50.0);
    assert!(second
        .actions
        .iter()
        .any(|a| matches!(a, TuningAction::Skipped(_))));
    assert_eq!(second.suggestion_count, 0);
}

/// W3: 低置信度建议进入审批门控
#[test]
fn wiring_low_confidence_goes_to_approval() {
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
    assert!(gw.approval_gate().pending_count() > 0);
    assert!(gw.approval_gate_mut().approve(&approval_id));
    let result = gw.apply_approved("q1", &approval_id, 500.0, 50.0);
    assert!(result.is_ok());
}

/// W4: 反馈循环检测回归并自动回滚
#[test]
fn wiring_feedback_loop_auto_rollback() {
    let mut gw = AutoTuningGateway::new(advisor());
    let plan = full_scan_plan();
    let _ = gw.run_loop("q1", Some(&plan), None, 100.0, 200.0);
    assert!(gw.feedback().sample_count("q1") > 0);
    gw.feedback_mut().record_sample("q1", 100.0, 250.0);
    gw.feedback_mut().record_sample("q1", 100.0, 300.0);
    let action = gw.check_and_rollback("q1");
    assert!(
        matches!(action, Some(TuningAction::RolledBack(_))),
        "should auto-rollback on regression"
    );
    let history = gw.auditor().history("q1");
    assert!(history.iter().any(|r| r.kind == ChangeKind::Rollback));
}

/// W5: 手动回滚已应用的建议
#[test]
fn wiring_manual_rollback() {
    let mut gw = AutoTuningGateway::new(advisor());
    let plan = full_scan_plan();
    let _ = gw.run_loop("q1", Some(&plan), None, 500.0, 50.0);
    let result = gw.rollback("q1");
    assert!(result.is_ok());
    let history = gw.auditor().history("q1");
    assert!(history.iter().any(|r| r.kind == ChangeKind::Rollback));
    assert!(!gw.cooldown().is_in_cooldown("q1"));
}

/// W6: JoinReorderAdvisor 基于统计重排 JOIN
#[test]
fn wiring_join_reorder_advisor() {
    let mut join_advisor = JoinReorderAdvisor::new();
    join_advisor.register_all(vec![
        TableStats {
            table: "orders".into(),
            row_count: 1_000_000,
            selectivity: 1.0,
        },
        TableStats {
            table: "users".into(),
            row_count: 100,
            selectivity: 1.0,
        },
    ]);
    let tables = vec![
        JoinTable {
            table: "orders".into(),
            alias: None,
            on_clause: "orders.user_id = users.id".into(),
        },
        JoinTable {
            table: "users".into(),
            alias: None,
            on_clause: "users.id = orders.user_id".into(),
        },
    ];
    let suggestion = join_advisor
        .advise("q1", &tables)
        .expect("should suggest reorder");
    assert_eq!(suggestion.suggestion_type, SuggestionType::RewriteQuery);
    let result = join_advisor.reorder(&tables);
    assert_eq!(result.reordered_tables, vec!["users", "orders"]);
}

/// W7: 变更审计记录完整上下文
#[test]
fn wiring_change_auditor_records_context() {
    let mut gw = AutoTuningGateway::new(advisor());
    let plan = full_scan_plan();
    let _ = gw.run_loop("q1", Some(&plan), None, 500.0, 50.0);
    let history = gw.auditor().history("q1");
    assert!(!history.is_empty());
    let apply_record = history
        .iter()
        .find(|r| r.kind == ChangeKind::Apply)
        .expect("should have apply record");
    assert!(
        apply_record.before_plan.is_some(),
        "should record before plan"
    );
    assert!(
        apply_record.before_ms > apply_record.after_ms,
        "should record metrics"
    );
}

/// W8: 无建议时不产生副作用
#[test]
fn wiring_no_suggestion_no_side_effects() {
    let mut gw = AutoTuningGateway::new(advisor());
    let report = gw.run_loop("q1", None, None, 100.0, 100.0);
    assert_eq!(report.suggestion_count, 0);
    assert!(report.actions.contains(&TuningAction::NoSuggestion));
    assert!(!report.entered_cooldown);
    assert!(gw.auditor().is_empty());
    assert!(gw.cooldown().is_empty());
}
