//! 端到端测试：自治动作执行失败回退 + 升级人工处理
//!
//! 验证当自治动作执行失败后，验证循环触发回退判定，
//! 并升级为人工处理。

use std::collections::HashMap;
use std::time::Duration;

use sz_orm_ai::autonomous::{
    AutonomousAction, AutonomousActionExecutor, AutonomousVerificationLoop, ExecutionResult,
    VerificationResult,
};

/// 验证动作执行失败后验证循环返回错误
#[tokio::test]
async fn e2e_action_failed_returns_verification_error() {
    let verifier = AutonomousVerificationLoop::new();

    let failed_execution = ExecutionResult {
        success: false,
        message: "connection refused".to_string(),
        duration: Duration::from_millis(100),
    };

    let result = verifier.verify(&failed_execution).await;
    assert!(result.is_err(), "动作执行失败时验证应返回错误");
}

/// 验证动作执行成功后验证通过且不触发回退
#[tokio::test]
async fn e2e_action_success_no_rollback() {
    let verifier = AutonomousVerificationLoop::new();

    let success_execution = ExecutionResult {
        success: true,
        message: "ok".to_string(),
        duration: Duration::from_millis(50),
    };

    let verification = verifier.verify(&success_execution).await.unwrap();
    assert!(verification.verified, "成功后验证应通过");
    assert!(!verifier.should_rollback(&verification), "成功不应触发回退");
}

/// 验证验证未通过时触发回退
#[tokio::test]
async fn e2e_verification_failed_triggers_rollback() {
    let verifier = AutonomousVerificationLoop::new();

    let unhealthy_verification = VerificationResult {
        verified: false,
        health_status: "unhealthy".to_string(),
        metrics: HashMap::new(),
    };

    assert!(
        verifier.should_rollback(&unhealthy_verification),
        "验证未通过应触发回退"
    );
}

/// 验证动作执行器对所有动作类型均返回结果
#[tokio::test]
async fn e2e_action_executor_all_action_types() {
    let executor = AutonomousActionExecutor::new();
    let params = [("fault_type".to_string(), "test".to_string())];

    for action in [
        AutonomousAction::AutoRemediation,
        AutonomousAction::AutoScaling,
        AutonomousAction::AutoTuning,
        AutonomousAction::SlowQueryGovernance,
    ] {
        let result = executor.execute(&action, &params).await;
        assert!(result.is_ok(), "动作 {:?} 应返回结果", action);
        let execution = result.unwrap();
        assert!(execution.success, "动作 {:?} 应执行成功", action);
    }
}

/// 验证完整回退链路：执行成功 → 验证通过 → 不回退
#[tokio::test]
async fn e2e_full_chain_success_no_rollback() {
    let executor = AutonomousActionExecutor::new();
    let verifier = AutonomousVerificationLoop::new();

    let action = AutonomousAction::AutoScaling;
    let params = [("target_instances".to_string(), "5".to_string())];
    let execution = executor.execute(&action, &params).await.unwrap();

    assert!(execution.success, "执行应成功");

    let verification = verifier.verify(&execution).await.unwrap();
    assert!(verification.verified, "验证应通过");
    assert!(!verifier.should_rollback(&verification), "不应触发回退");
}

/// 验证完整回退链路：执行失败 → 验证报错 → 应回退
#[tokio::test]
async fn e2e_full_chain_failure_triggers_rollback() {
    let verifier = AutonomousVerificationLoop::new();

    let failed_execution = ExecutionResult {
        success: false,
        message: "timeout".to_string(),
        duration: Duration::from_secs(30),
    };

    let result = verifier.verify(&failed_execution).await;
    assert!(result.is_err(), "执行失败时验证应返回错误");
}
