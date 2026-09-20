//! v7.6.0 组3.9：Saga 自动补偿增强端到端测试
//!
//! 验证：自动补偿生成 + 幂等校验 + 重试策略 + 补偿延迟 ≤ 1s

use sz_orm_dtx::saga::{
    AutoCompensationGenerator, BackoffStrategy, CompensationRetryPolicy, IdempotencyChecker,
    SagaOperationData, SagaOperationType,
};

#[test]
fn test_e2e_auto_compensation_insert() {
    let forward = SagaOperationData {
        op_type: SagaOperationType::Insert,
        table: "users".to_string(),
        primary_key: serde_json::json!(1),
        old_values: None,
        new_values: Some(serde_json::json!({"name": "Alice"})),
    };
    let comp = AutoCompensationGenerator::generate_compensation(&forward);
    assert_eq!(comp.op_type, SagaOperationType::Delete);
    let sql = AutoCompensationGenerator::generate_compensation_sql(&forward);
    assert!(sql.contains("$1"));
}

#[test]
fn test_e2e_auto_compensation_update() {
    let forward = SagaOperationData {
        op_type: SagaOperationType::Update,
        table: "users".to_string(),
        primary_key: serde_json::json!(1),
        old_values: Some(serde_json::json!({"name": "Alice"})),
        new_values: Some(serde_json::json!({"name": "Bob"})),
    };
    let comp = AutoCompensationGenerator::generate_compensation(&forward);
    assert_eq!(comp.op_type, SagaOperationType::Update);
    assert_eq!(comp.old_values, Some(serde_json::json!({"name": "Bob"})));
}

#[test]
fn test_e2e_idempotency_and_retry() {
    let delete = SagaOperationData {
        op_type: SagaOperationType::Delete,
        table: "t".to_string(),
        primary_key: serde_json::json!(1),
        old_values: None,
        new_values: None,
    };
    assert!(IdempotencyChecker::check_idempotency(&delete));

    let policy = CompensationRetryPolicy::new(3, BackoffStrategy::Exponential, 50);
    assert!(policy.backoff_duration(0) <= std::time::Duration::from_millis(100));
}

#[test]
fn test_e2e_compensation_latency() {
    let start = std::time::Instant::now();
    let forward = SagaOperationData {
        op_type: SagaOperationType::Insert,
        table: "orders".to_string(),
        primary_key: serde_json::json!(42),
        old_values: None,
        new_values: Some(serde_json::json!({"total": 100})),
    };
    let _comp = AutoCompensationGenerator::generate_compensation(&forward);
    let latency = start.elapsed().as_millis();
    assert!(latency <= 1000, "补偿生成延迟应 ≤ 1s");
}
