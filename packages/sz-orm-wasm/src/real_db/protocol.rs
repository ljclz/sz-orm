//! WasmDbProxyProtocol — WASM ↔ 后端 DB 代理协议

use super::WasmRealDbError;
use crate::WasmQuery;
use serde::{Deserialize, Serialize};

/// 代理请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyRequest {
    pub session_id: String,
    pub token: String,
    pub query: WasmQuery,
    pub transaction_id: Option<String>,
}

/// 代理响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyResponse {
    pub status: ProxyStatus,
    pub rows: Vec<serde_json::Value>,
    pub rows_affected: Option<usize>,
    pub error: Option<ProxyError>,
    pub latency_ms: u64,
}

/// 代理状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyStatus {
    Ok,
    Error,
}

/// 代理错误
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "code", content = "detail")]
pub enum ProxyError {
    AuthFailed,
    RateLimited,
    SqlRejected { reason: String },
    QueryFailed { reason: String },
    ProxyUnavailable,
    CredentialsNotExposed,
    ResultTooLarge,
}

/// 序列化格式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SerializationFormat {
    Json,
    MessagePack,
}

/// WASM DB 代理协议
pub struct WasmDbProxyProtocol;

impl WasmDbProxyProtocol {
    /// JSON 序列化 ProxyRequest
    pub fn serialize_request_json(req: &ProxyRequest) -> Result<Vec<u8>, WasmRealDbError> {
        serde_json::to_vec(req).map_err(|e| WasmRealDbError::SerializationError(e.to_string()))
    }

    /// JSON 反序列化 ProxyRequest
    pub fn deserialize_request_json(bytes: &[u8]) -> Result<ProxyRequest, WasmRealDbError> {
        serde_json::from_slice(bytes)
            .map_err(|e| WasmRealDbError::SerializationError(e.to_string()))
    }

    /// JSON 序列化 ProxyResponse
    pub fn serialize_response_json(resp: &ProxyResponse) -> Result<Vec<u8>, WasmRealDbError> {
        serde_json::to_vec(resp).map_err(|e| WasmRealDbError::SerializationError(e.to_string()))
    }

    /// JSON 反序列化 ProxyResponse
    pub fn deserialize_response_json(bytes: &[u8]) -> Result<ProxyResponse, WasmRealDbError> {
        serde_json::from_slice(bytes)
            .map_err(|e| WasmRealDbError::SerializationError(e.to_string()))
    }

    /// MessagePack 序列化 ProxyRequest
    pub fn serialize_request_msgpack(req: &ProxyRequest) -> Result<Vec<u8>, WasmRealDbError> {
        rmp_serde::to_vec(req).map_err(|e| WasmRealDbError::SerializationError(e.to_string()))
    }

    /// MessagePack 反序列化 ProxyRequest
    pub fn deserialize_request_msgpack(bytes: &[u8]) -> Result<ProxyRequest, WasmRealDbError> {
        rmp_serde::from_slice(bytes).map_err(|e| WasmRealDbError::SerializationError(e.to_string()))
    }

    /// MessagePack 序列化 ProxyResponse
    pub fn serialize_response_msgpack(resp: &ProxyResponse) -> Result<Vec<u8>, WasmRealDbError> {
        rmp_serde::to_vec(resp).map_err(|e| WasmRealDbError::SerializationError(e.to_string()))
    }

    /// MessagePack 反序列化 ProxyResponse
    pub fn deserialize_response_msgpack(bytes: &[u8]) -> Result<ProxyResponse, WasmRealDbError> {
        rmp_serde::from_slice(bytes).map_err(|e| WasmRealDbError::SerializationError(e.to_string()))
    }
}

impl ProxyRequest {
    pub fn serialize(&self, format: SerializationFormat) -> Result<Vec<u8>, WasmRealDbError> {
        match format {
            SerializationFormat::Json => WasmDbProxyProtocol::serialize_request_json(self),
            SerializationFormat::MessagePack => {
                WasmDbProxyProtocol::serialize_request_msgpack(self)
            }
        }
    }
}

impl ProxyResponse {
    pub fn deserialize(bytes: &[u8], format: SerializationFormat) -> Result<Self, WasmRealDbError> {
        match format {
            SerializationFormat::Json => WasmDbProxyProtocol::deserialize_response_json(bytes),
            SerializationFormat::MessagePack => {
                WasmDbProxyProtocol::deserialize_response_msgpack(bytes)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_request() -> ProxyRequest {
        ProxyRequest {
            session_id: "sess-123".to_string(),
            token: "token-abc".to_string(),
            query: WasmQuery::with_params(
                "SELECT * FROM users WHERE id = ?",
                vec![serde_json::json!(1)],
            ),
            transaction_id: None,
        }
    }

    fn make_response() -> ProxyResponse {
        ProxyResponse {
            status: ProxyStatus::Ok,
            rows: vec![serde_json::json!({"id": 1, "name": "Alice"})],
            rows_affected: None,
            error: None,
            latency_ms: 42,
        }
    }

    #[test]
    fn test_json_request_roundtrip() {
        let req = make_request();
        let bytes = WasmDbProxyProtocol::serialize_request_json(&req).unwrap();
        let req2 = WasmDbProxyProtocol::deserialize_request_json(&bytes).unwrap();
        assert_eq!(req.session_id, req2.session_id);
        assert_eq!(req.token, req2.token);
        assert_eq!(req.query.sql, req2.query.sql);
    }

    #[test]
    fn test_json_response_roundtrip() {
        let resp = make_response();
        let bytes = WasmDbProxyProtocol::serialize_response_json(&resp).unwrap();
        let resp2 = WasmDbProxyProtocol::deserialize_response_json(&bytes).unwrap();
        assert_eq!(resp.status, resp2.status);
        assert_eq!(resp.latency_ms, resp2.latency_ms);
        assert_eq!(resp.rows.len(), resp2.rows.len());
    }

    #[test]
    fn test_msgpack_request_roundtrip() {
        let req = make_request();
        let bytes = WasmDbProxyProtocol::serialize_request_msgpack(&req).unwrap();
        let req2 = WasmDbProxyProtocol::deserialize_request_msgpack(&bytes).unwrap();
        assert_eq!(req.session_id, req2.session_id);
        assert_eq!(req.query.sql, req2.query.sql);
    }

    #[test]
    fn test_msgpack_response_roundtrip() {
        let resp = make_response();
        let bytes = WasmDbProxyProtocol::serialize_response_msgpack(&resp).unwrap();
        let resp2 = WasmDbProxyProtocol::deserialize_response_msgpack(&bytes).unwrap();
        assert_eq!(resp.status, resp2.status);
        assert_eq!(resp.rows.len(), resp2.rows.len());
    }

    #[test]
    fn test_msgpack_smaller_than_json() {
        let resp = make_response();
        let json_size = WasmDbProxyProtocol::serialize_response_json(&resp)
            .unwrap()
            .len();
        let msgpack_size = WasmDbProxyProtocol::serialize_response_msgpack(&resp)
            .unwrap()
            .len();
        assert!(
            msgpack_size < json_size,
            "MessagePack ({}) should be smaller than JSON ({})",
            msgpack_size,
            json_size
        );
    }

    #[test]
    fn test_proxy_request_serialize_method() {
        let req = make_request();
        let json_bytes = req.serialize(SerializationFormat::Json).unwrap();
        let msgpack_bytes = req.serialize(SerializationFormat::MessagePack).unwrap();
        assert!(!json_bytes.is_empty());
        assert!(!msgpack_bytes.is_empty());
    }

    #[test]
    fn test_proxy_response_deserialize_method() {
        let resp = make_response();
        let json_bytes = WasmDbProxyProtocol::serialize_response_json(&resp).unwrap();
        let resp2 = ProxyResponse::deserialize(&json_bytes, SerializationFormat::Json).unwrap();
        assert_eq!(resp.status, resp2.status);
    }
}
