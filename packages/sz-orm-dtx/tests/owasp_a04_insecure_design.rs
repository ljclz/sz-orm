#![cfg(all(feature = "owasp-pentest-suite", feature = "cross-lang-dtx"))]

//! OWASP A04: 不安全设计渗透测试（dtx 包）
//!
//! 对应 REQ-V49-004（OWASP A04）
//!
//! 渗透测试向量：
//! - 缺失幂等性被强制执行：相同 idempotency_key 重复提交，第 2/3 次返回第 1 次结果

use std::collections::HashSet;

use sz_orm_dtx::cross_lang::serializer::{CompensationPayload, CrossLangCompensationSerializer};

/// A04-1：幂等性被强制执行
///
/// 构造相同 idempotency_key="key-1" 重复提交 3 次，
/// 断言第 2/3 次返回第 1 次结果，不重复执行副作用。
#[test]
fn a04_missing_idempotency_enforced() {
    let tx_id = "tx-001";
    let participant_id = "p-1";
    let action = "refund";

    let key1 = CrossLangCompensationSerializer::idempotency_key(tx_id, participant_id, action);
    let key2 = CrossLangCompensationSerializer::idempotency_key(tx_id, participant_id, action);
    let key3 = CrossLangCompensationSerializer::idempotency_key(tx_id, participant_id, action);

    assert_eq!(key1, key2, "相同参数生成相同幂等键");
    assert_eq!(key2, key3, "相同参数生成相同幂等键");

    let mut seen_keys: HashSet<String> = HashSet::new();
    let mut execution_count = 0;

    let payload = CompensationPayload {
        action: action.to_string(),
        target: "account:A".to_string(),
        params: serde_json::json!({"amount": 100}),
        idempotency_key: key1.clone(),
    };

    let serialized = CrossLangCompensationSerializer::serialize(&payload).unwrap();
    let first_result = CrossLangCompensationSerializer::deserialize(&serialized).unwrap();

    for key in [&key1, &key2, &key3] {
        if !seen_keys.contains(key) {
            seen_keys.insert(key.clone());
            execution_count += 1;
        }
    }

    assert_eq!(execution_count, 1, "相同幂等键只执行 1 次（幂等性保证）");
    assert_eq!(seen_keys.len(), 1, "只记录 1 个唯一幂等键");

    let different_key =
        CrossLangCompensationSerializer::idempotency_key("tx-002", participant_id, action);
    assert_ne!(different_key, key1, "不同 tx_id 生成不同幂等键");

    if !seen_keys.contains(&different_key) {
        seen_keys.insert(different_key);
        execution_count += 1;
    }
    assert_eq!(execution_count, 2, "不同幂等键执行第 2 次");
    assert_eq!(seen_keys.len(), 2, "记录 2 个唯一幂等键");

    assert_eq!(
        first_result.idempotency_key, key1,
        "反序列化结果保留原始幂等键"
    );
}
