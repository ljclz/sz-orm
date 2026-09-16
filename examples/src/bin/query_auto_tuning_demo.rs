//! 查询自调优生产入口 demo（v7.1.0）
//!
//! 演示 `AutoTuningGateway` 完整闭环：
//! 1. 慢查询统计收集
//! 2. 索引推荐生成
//! 3. 高置信度自动应用 + 低置信度审批门控
//! 4. 冷却期管理
//! 5. 性能劣化自动回滚
//!
//! 运行：`cargo run --bin query_auto_tuning_demo --features query-auto-tuning`

use std::sync::Arc;

use sz_orm_advisor::{
    advisor::OptimizationAdvisor, AutoTuningConfig, AutoTuningGateway, JoinReorderAdvisor,
    JoinTable, TableStats, TuningAction,
};
use sz_orm_explain::{ExplainPlan, ScanType};

fn main() {
    println!("=== sz-orm v7.1.0 查询自调优 demo ===\n");

    let advisor = Arc::new(OptimizationAdvisor::with_defaults());
    let mut gw = AutoTuningGateway::new(advisor);

    let plan = ExplainPlan {
        scan_type: ScanType::FullTable,
        table: "users".into(),
        index: None,
        rows: 100_000,
        extra: vec![],
    };

    println!("[1] 慢查询检测：FullTable scan on users (100000 rows)");
    let report = gw.run_loop("slow_q1", Some(&plan), None, 800.0, 80.0);
    println!(
        "    建议数: {}, 动作: {:?}",
        report.suggestion_count, report.actions
    );
    println!("    进入冷却期: {}", report.entered_cooldown);
    println!(
        "    审计记录: {} 条\n",
        gw.auditor().history("slow_q1").len()
    );

    println!("[2] 冷却期内再次调优（应跳过）");
    let report2 = gw.run_loop("slow_q1", Some(&plan), None, 800.0, 80.0);
    println!("    动作: {:?}\n", report2.actions);

    println!("[3] JOIN 重排建议");
    let mut join_advisor = JoinReorderAdvisor::new();
    join_advisor.register_all(vec![
        TableStats {
            table: "orders".into(),
            row_count: 5_000_000,
            selectivity: 1.0,
        },
        TableStats {
            table: "users".into(),
            row_count: 500,
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
    let result = join_advisor.reorder(&tables);
    println!("    原始顺序: {}", result.original_order.join(" → "));
    println!("    重排顺序: {}", result.reordered_tables.join(" → "));
    println!("    预估改善: {:.1}%\n", result.estimated_improvement_pct);

    println!("[4] 低置信度建议审批门控");
    let config = AutoTuningConfig {
        auto_apply_threshold: 0.99,
        ..Default::default()
    };
    let mut gw2 =
        AutoTuningGateway::new(Arc::new(OptimizationAdvisor::with_defaults())).with_config(config);
    let report3 = gw2.run_loop("low_conf_q", Some(&plan), None, 800.0, 80.0);
    let approval_id = report3
        .actions
        .into_iter()
        .find_map(|a| match a {
            TuningAction::PendingApproval(id) => Some(id),
            _ => None,
        })
        .expect("should have pending approval");
    println!("    提交审批: {}", approval_id);
    println!("    待审批数: {}", gw2.approval_gate().pending_count());
    gw2.approval_gate_mut().approve(&approval_id);
    println!(
        "    审批通过后应用: {:?}",
        gw2.apply_approved("low_conf_q", &approval_id, 800.0, 80.0)
    );
    println!();

    println!("[5] 性能劣化自动回滚");
    gw.feedback_mut().record_sample("slow_q1", 80.0, 200.0);
    gw.feedback_mut().record_sample("slow_q1", 80.0, 250.0);
    let rollback_action = gw.check_and_rollback("slow_q1");
    println!("    回滚动作: {:?}", rollback_action);
    let history = gw.auditor().history("slow_q1");
    println!("    审计历史: {} 条 (Apply + Rollback)\n", history.len());

    println!("[6] 审计摘要");
    let summary = gw.auditor().summary();
    println!("    总变更: {}", summary.total_changes);
    println!("    应用数: {}", summary.apply_count);
    println!("    回滚数: {}", summary.rollback_count);

    println!("\n=== demo 完成 ===");
}
