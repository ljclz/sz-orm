//! AI-DIAG-01 接线验证测试（v6.8.0）
//!
//! 验证慢查询/查询错误 → AutoDiagnoseHook → 诊断报告 + 索引建议 端到端管线。

use sz_orm_diagnosis::auto_diagnose_hook::{AutoDiagnoseHook, DiagnoseSeverity};

#[test]
fn wiring_slow_query_with_missing_index_produces_index_ddl() {
    let hook = AutoDiagnoseHook::new(100);
    let result = hook.on_slow_query(
        "SELECT * FROM orders WHERE user_id = 42",
        500,
        "Seq Scan on orders",
    );
    assert_eq!(result.severity, DiagnoseSeverity::Critical);
    assert!(!result.index_suggestions.is_empty());
    let suggestion = &result.index_suggestions[0];
    assert!(suggestion.ddl.contains("CREATE INDEX"));
    assert!(suggestion.ddl.contains("orders"));
    assert!(suggestion.ddl.contains("user_id"));
}

#[test]
fn wiring_query_error_diagnoses_root_cause() {
    let hook = AutoDiagnoseHook::new(100);
    let result = hook.on_query_error("SELECT * FROM large_table", "query timeout after 30s");
    assert_eq!(result.severity, DiagnoseSeverity::Critical);
    assert!(result.root_cause.contains("超时"));
}

#[test]
fn wiring_full_table_scan_root_cause() {
    let hook = AutoDiagnoseHook::new(100);
    let result = hook.on_slow_query(
        "SELECT * FROM users WHERE age = 30",
        300,
        "Seq Scan on users",
    );
    assert!(result.root_cause.contains("全表扫描"));
}

#[test]
fn wiring_order_by_suggests_sort_index() {
    let hook = AutoDiagnoseHook::new(100);
    let result = hook.on_slow_query("SELECT * FROM users ORDER BY created_at", 250, "Sort");
    let has_sort = result
        .index_suggestions
        .iter()
        .any(|s| s.reason.contains("排序"));
    assert!(has_sort);
}

#[test]
fn wiring_deadlock_error_diagnosed() {
    let hook = AutoDiagnoseHook::new(100);
    let result = hook.on_query_error(
        "UPDATE accounts SET balance = 100 WHERE id = 1",
        "deadlock detected while updating",
    );
    assert!(result.root_cause.contains("死锁"));
}

#[test]
fn wiring_pool_exhaustion_diagnosed() {
    let hook = AutoDiagnoseHook::new(100);
    let result = hook.on_query_error("SELECT 1", "connection pool exhausted");
    assert!(result.root_cause.contains("连接池"));
}

#[test]
fn wiring_fallback_to_rules_when_llm_unavailable() {
    let hook = AutoDiagnoseHook::new(100);
    let result = hook.on_slow_query("SELECT * FROM big_table", 500, "Seq Scan");
    assert!(result.fallback_to_rules);
    assert!(!result.llm_used);
}

#[test]
fn wiring_multiple_diagnoses_tracked() {
    let hook = AutoDiagnoseHook::new(100);
    hook.on_slow_query("SELECT * FROM a WHERE x = 1", 200, "Seq Scan");
    hook.on_slow_query("SELECT * FROM b WHERE y = 2", 300, "Seq Scan");
    hook.on_query_error("SELECT * FROM c", "timeout");
    assert_eq!(hook.diagnose_count(), 3);
    assert_eq!(hook.recent_reports().len(), 3);
}

#[test]
fn wiring_severity_based_on_threshold() {
    let hook = AutoDiagnoseHook::new(100);
    let info = hook.on_slow_query("SELECT 1", 150, "");
    assert_eq!(info.severity, DiagnoseSeverity::Info);

    let warning = hook.on_slow_query("SELECT 1", 250, "");
    assert_eq!(warning.severity, DiagnoseSeverity::Warning);

    let critical = hook.on_slow_query("SELECT 1", 400, "");
    assert_eq!(critical.severity, DiagnoseSeverity::Critical);
}

#[test]
fn wiring_join_query_suggests_join_index() {
    let hook = AutoDiagnoseHook::new(100);
    let result = hook.on_slow_query(
        "SELECT * FROM orders JOIN users ON orders.user_id = users.id",
        500,
        "Hash Join",
    );
    assert!(!result.index_suggestions.is_empty());
    let has_join_advice = result
        .index_suggestions
        .iter()
        .any(|s| s.reason.contains("JOIN"));
    assert!(has_join_advice);
}
