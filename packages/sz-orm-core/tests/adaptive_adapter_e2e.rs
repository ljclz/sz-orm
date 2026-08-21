//! 自适应查询适配层端到端测试
//!
//! 验证 adaptive_adapter 的三个入口函数真实调用 sz-orm-adaptive 的 AdaptiveExecutor。

use sz_orm_adaptive::ExecutionPath;
use sz_orm_core::adaptive_adapter::{adaptive_decide, adaptive_decision_count, adaptive_record};

#[test]
fn test_adaptive_decide_returns_path() {
    let path = adaptive_decide("e2e_query");
    assert!(
        matches!(
            path,
            ExecutionPath::Normal | ExecutionPath::Paginated | ExecutionPath::Cached
        ),
        "decide should return a valid ExecutionPath"
    );
}

#[test]
fn test_adaptive_record_updates_stats() {
    let slow = adaptive_record("e2e_record", 500, 200);
    assert!(slow, "200ms should be slow with default 100ms threshold");
}

#[test]
fn test_adaptive_count_increments() {
    let before = adaptive_decision_count();
    let _ = adaptive_decide("e2e_count_test");
    let after = adaptive_decision_count();
    assert!(
        after > before,
        "decision count should increment after decide call"
    );
}
