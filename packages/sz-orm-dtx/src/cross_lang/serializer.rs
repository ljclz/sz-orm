//! 跨语言补偿序列化器
//!
//! 将 Rust 侧的补偿逻辑序列化为跨语言可执行的协议消息（操作描述），
//! 跨语言参与者收到补偿请求后执行其语言侧补偿逻辑。

use super::CrossLangTxError;
use serde::{Deserialize, Serialize};

/// 补偿操作负载
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompensationPayload {
    /// 操作名称（如 "deduct"、"refund"）
    pub action: String,
    /// 操作目标（如 "account:A"）
    pub target: String,
    /// 操作参数
    pub params: serde_json::Value,
    /// 幂等键
    pub idempotency_key: String,
}

/// 跨语言补偿序列化器
pub struct CrossLangCompensationSerializer;

impl CrossLangCompensationSerializer {
    /// 序列化补偿负载为 JSON 字节
    pub fn serialize(payload: &CompensationPayload) -> Result<Vec<u8>, CrossLangTxError> {
        serde_json::to_vec(payload)
            .map_err(|e| CrossLangTxError::Transport(format!("serialization failed: {e}")))
    }

    /// 反序列化 JSON 字节为补偿负载
    pub fn deserialize(bytes: &[u8]) -> Result<CompensationPayload, CrossLangTxError> {
        serde_json::from_slice(bytes)
            .map_err(|e| CrossLangTxError::Transport(format!("deserialization failed: {e}")))
    }

    /// 自动构造补偿操作描述
    ///
    /// 将原操作映射为逆操作（如 deduct → refund、create → delete、add → remove）
    pub fn build_rollback_payload(
        original_action: &str,
        target: &str,
        params: &serde_json::Value,
    ) -> CompensationPayload {
        let rollback_action = match original_action {
            "deduct" => "refund",
            "create" => "delete",
            "add" => "remove",
            "insert" => "delete",
            "update" => "revert",
            "charge" => "refund",
            "reserve" => "release",
            "lock" => "unlock",
            "enable" => "disable",
            "subscribe" => "unsubscribe",
            other => other,
        };
        CompensationPayload {
            action: rollback_action.to_string(),
            target: target.to_string(),
            params: params.clone(),
            idempotency_key: String::new(),
        }
    }

    /// 生成幂等键
    ///
    /// 格式：`{tx_id}:{participant_id}:{action}`
    pub fn idempotency_key(tx_id: &str, participant_id: &str, action: &str) -> String {
        format!("{tx_id}:{participant_id}:{action}")
    }

    /// 构建带幂等键的补偿负载
    pub fn build_compensation(
        tx_id: &str,
        participant_id: &str,
        original_action: &str,
        target: &str,
        params: &serde_json::Value,
    ) -> CompensationPayload {
        let mut payload = Self::build_rollback_payload(original_action, target, params);
        payload.idempotency_key = Self::idempotency_key(tx_id, participant_id, &payload.action);
        payload
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let payload = CompensationPayload {
            action: "refund".to_string(),
            target: "account:A".to_string(),
            params: serde_json::json!({"amount": 100}),
            idempotency_key: "tx-001:participant-1:refund".to_string(),
        };
        let bytes = CrossLangCompensationSerializer::serialize(&payload).unwrap();
        let decoded = CrossLangCompensationSerializer::deserialize(&bytes).unwrap();
        assert_eq!(decoded.action, "refund");
        assert_eq!(decoded.target, "account:A");
        assert_eq!(decoded.params, serde_json::json!({"amount": 100}));
        assert_eq!(decoded.idempotency_key, "tx-001:participant-1:refund");
    }

    #[test]
    fn test_build_rollback_payload_deduct_to_refund() {
        let payload = CrossLangCompensationSerializer::build_rollback_payload(
            "deduct",
            "account:A",
            &serde_json::json!({"amount": 100}),
        );
        assert_eq!(payload.action, "refund");
        assert_eq!(payload.target, "account:A");
        assert_eq!(payload.params, serde_json::json!({"amount": 100}));
    }

    #[test]
    fn test_build_rollback_payload_create_to_delete() {
        let payload = CrossLangCompensationSerializer::build_rollback_payload(
            "create",
            "order:123",
            &serde_json::json!({"id": 123}),
        );
        assert_eq!(payload.action, "delete");
    }

    #[test]
    fn test_build_rollback_payload_reserve_to_release() {
        let payload = CrossLangCompensationSerializer::build_rollback_payload(
            "reserve",
            "stock:item-1",
            &serde_json::json!({"qty": 5}),
        );
        assert_eq!(payload.action, "release");
    }

    #[test]
    fn test_build_rollback_payload_unknown_action_passthrough() {
        let payload = CrossLangCompensationSerializer::build_rollback_payload(
            "custom_action",
            "resource:X",
            &serde_json::json!({}),
        );
        assert_eq!(payload.action, "custom_action");
    }

    #[test]
    fn test_idempotency_key_deterministic() {
        let key1 = CrossLangCompensationSerializer::idempotency_key("tx-001", "p-1", "refund");
        let key2 = CrossLangCompensationSerializer::idempotency_key("tx-001", "p-1", "refund");
        assert_eq!(key1, key2);
        assert_eq!(key1, "tx-001:p-1:refund");
    }

    #[test]
    fn test_idempotency_key_different_inputs() {
        let key1 = CrossLangCompensationSerializer::idempotency_key("tx-001", "p-1", "refund");
        let key2 = CrossLangCompensationSerializer::idempotency_key("tx-002", "p-1", "refund");
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_build_compensation_with_idempotency_key() {
        let payload = CrossLangCompensationSerializer::build_compensation(
            "tx-001",
            "participant-1",
            "deduct",
            "account:A",
            &serde_json::json!({"amount": 100}),
        );
        assert_eq!(payload.action, "refund");
        assert_eq!(payload.idempotency_key, "tx-001:participant-1:refund");
    }

    #[test]
    fn test_deserialize_invalid_json() {
        let result = CrossLangCompensationSerializer::deserialize(b"not json");
        assert!(result.is_err());
    }
}
