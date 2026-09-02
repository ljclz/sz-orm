//! TASK-018 验证测试：LLM 降级

use sz_orm_agent::fallback::LlmFallbackManager;

#[test]
fn test_fallback_threshold() {
    let manager = LlmFallbackManager::new(3);
    assert!(!manager.record_failure());
    assert!(!manager.record_failure());
    assert!(manager.record_failure(), "第 3 次失败触发降级");
    assert!(manager.is_degraded());
}

#[test]
fn test_fallback_recovery() {
    let manager = LlmFallbackManager::new(2);
    manager.record_failure();
    manager.record_failure();
    assert!(manager.is_degraded());
    manager.record_success();
    assert!(!manager.is_degraded(), "成功后恢复");
}
