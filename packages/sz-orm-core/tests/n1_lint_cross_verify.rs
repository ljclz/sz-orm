//! v4.3.0 M2-T3：N+1 静态检测 ↔ 运行时 N1QueryDetector 交叉验证
//!
//! 仅当 sz-orm-core 启用 `n1-lint` feature 时编译。
//!
//! 验证目标：同一 N+1 查询模式（循环内逐条查询）应被**静态分析**（开发期）
//! 与**运行时检测**（`N1QueryDetector`，entity_graph.rs:641）一致检出，
//! 静态在前、运行时兜底，互补无矛盾。
//!
//! ```bash
//! cargo test -p sz-orm-core --features n1-lint --test n1_lint_cross_verify
//! ```

#![cfg(feature = "n1-lint")]

use sz_orm_core::entity_graph::{N1DetectionConfig, N1QueryDetector};

/// 模拟的 N+1 代码模式（与静态分析输入同构）
const N1_PATTERN_CODE: &str = r#"
fn process_users(users: Vec<User>) {
    for user in users {
        let orders = Order::find_by_user(user.id); // N+1 模式
    }
}
"#;

#[test]
fn static_analysis_detects_query_in_loop() {
    let findings = sz_orm_n1_lint::analyze_str(N1_PATTERN_CODE, "pattern.rs");
    assert!(
        findings
            .iter()
            .any(|f| f.pattern == sz_orm_n1_lint::N1Pattern::QueryInLoop),
        "static analysis must flag query-in-loop"
    );
}

#[test]
fn runtime_detector_flags_same_pattern() {
    // 模拟循环内逐条查询（threshold=5，5 次单条加载无批量）
    let detector = N1QueryDetector::new(N1DetectionConfig::new().with_threshold(5));
    detector.start_window();
    for _ in 0..5 {
        detector.record_single_load("orders");
    }
    let alerts = detector.end_window();
    assert!(
        alerts
            .iter()
            .any(|a| a.relation == "orders" && a.no_batch_used()),
        "runtime detector must flag orders N+1 with no batch usage"
    );
}

#[test]
fn batch_load_suppresses_runtime_alert() {
    // 批量加载后同一窗口不再告警（与静态检测的 where_in 建议对应）
    let detector = N1QueryDetector::new(N1DetectionConfig::new().with_threshold(5));
    detector.start_window();
    for _ in 0..5 {
        detector.record_single_load("orders");
    }
    detector.record_batch_load("orders", 5); // 批量加载兜底
    let alerts = detector.end_window();
    assert!(
        !alerts
            .iter()
            .any(|a| a.relation == "orders" && a.no_batch_used()),
        "batch load must suppress the N+1 alert"
    );
}

#[test]
fn suggested_batch_size_is_reasonable() {
    let detector = N1QueryDetector::new(N1DetectionConfig::new().with_threshold(5));
    detector.start_window();
    for _ in 0..50 {
        detector.record_single_load("orders");
    }
    let alerts = detector.end_window();
    let alert = alerts
        .iter()
        .find(|a| a.relation == "orders")
        .expect("alert exists");
    assert!(alert.suggested_batch_size() >= 50);
}
