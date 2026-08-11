//! WasmRealDbQueryExecutor — WASM 端查询执行器
//!
//! 集成 SQL 白名单、限流、鉴权、指标采集，复用 WasmQuery。

use super::auth::WasmDbAuthValidator;
use super::connection::WasmRealDbConnection;
use super::metrics::WasmRealDbMetrics;
use super::protocol::{ProxyResponse, ProxyStatus};
use super::rate_limiter::WasmDbRateLimiter;
use super::sql_whitelist::WasmDbSqlWhitelist;
use super::WasmRealDbError;
use crate::WasmQuery;

/// WASM 真实 DB 查询执行器
///
/// 组合连接、白名单、限流、鉴权、指标，提供统一的查询/执行接口。
pub struct WasmRealDbQueryExecutor {
    connection: WasmRealDbConnection,
    sql_whitelist: WasmDbSqlWhitelist,
    rate_limiter: WasmDbRateLimiter,
    auth_validator: WasmDbAuthValidator,
    metrics: WasmRealDbMetrics,
    max_result_rows: usize,
}

impl WasmRealDbQueryExecutor {
    /// 创建新执行器
    pub fn new(
        connection: WasmRealDbConnection,
        sql_whitelist: WasmDbSqlWhitelist,
        rate_limiter: WasmDbRateLimiter,
        auth_validator: WasmDbAuthValidator,
        max_result_rows: usize,
    ) -> Self {
        Self {
            connection,
            sql_whitelist,
            rate_limiter,
            auth_validator,
            metrics: WasmRealDbMetrics::new(),
            max_result_rows,
        }
    }

    /// 获取指标快照
    pub fn metrics(&self) -> &WasmRealDbMetrics {
        &self.metrics
    }

    /// 获取连接引用
    pub fn connection(&self) -> &WasmRealDbConnection {
        &self.connection
    }

    /// 获取可变连接
    pub fn connection_mut(&mut self) -> &mut WasmRealDbConnection {
        &mut self.connection
    }

    /// 最大结果行数
    pub fn max_result_rows(&self) -> usize {
        self.max_result_rows
    }

    /// 执行查询前的前置检查
    ///
    /// 返回 Ok(()) 表示通过所有检查，Err 表示被拦截。
    fn pre_check(&mut self, query: &WasmQuery) -> Result<(), WasmRealDbError> {
        if !self.connection.is_connected() {
            return Err(WasmRealDbError::ProxyUnavailable);
        }

        if !self.auth_validator.validate_token(self.connection.token()) {
            self.metrics.record_error();
            return Err(WasmRealDbError::AuthFailed);
        }

        if !self.sql_whitelist.validate(&query.sql) {
            self.metrics.record_error();
            return Err(WasmRealDbError::SqlRejected {
                reason: format!("SQL not in whitelist: {}", query.sql),
            });
        }

        if !self.rate_limiter.check_and_increment() {
            self.metrics.record_error();
            return Err(WasmRealDbError::RateLimited);
        }

        Ok(())
    }

    /// 处理代理响应
    fn handle_response(
        &mut self,
        response: ProxyResponse,
    ) -> Result<ProxyResponse, WasmRealDbError> {
        if response.rows.len() > self.max_result_rows {
            self.metrics.record_error();
            return Err(WasmRealDbError::ResultTooLarge);
        }

        if response.status == ProxyStatus::Error {
            self.metrics.record_error();
            if let Some(err) = response.error {
                return Err(WasmRealDbError::QueryFailed {
                    reason: format!("{:?}", err),
                });
            }
            return Err(WasmRealDbError::QueryFailed {
                reason: "unknown proxy error".to_string(),
            });
        }

        self.metrics.record_query(response.latency_ms);
        Ok(response)
    }

    /// 同步查询（通过 HTTP 代理）
    ///
    /// 在 WASM 环境中应使用 async 版本。
    #[cfg(feature = "wasm-real-db")]
    pub async fn query_async(
        &mut self,
        query: WasmQuery,
    ) -> Result<ProxyResponse, WasmRealDbError> {
        self.pre_check(&query)?;
        let request = self.connection.build_request(query, None);
        let response = self.connection.send_request_http(&request).await?;
        self.handle_response(response)
    }

    /// 从已收到的原始响应字节处理查询结果
    ///
    /// 适用于 WebSocket 或自定义 transport 场景：
    /// 调用方负责传输，本方法负责前置检查 + 反序列化 + 后置处理。
    pub fn process_response_bytes(
        &mut self,
        query: &WasmQuery,
        response_bytes: &[u8],
    ) -> Result<ProxyResponse, WasmRealDbError> {
        self.pre_check(query)?;
        let response = self.connection.deserialize_response(response_bytes)?;
        self.handle_response(response)
    }

    /// 重置指标
    pub fn reset_metrics(&mut self) {
        self.metrics.reset();
    }
}

impl std::fmt::Debug for WasmRealDbQueryExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmRealDbQueryExecutor")
            .field("connection", &self.connection)
            .field("max_result_rows", &self.max_result_rows)
            .field("metrics", &self.metrics)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::real_db::connection::WasmTransport;
    use crate::real_db::protocol::SerializationFormat;

    fn make_executor(max_result_rows: usize) -> WasmRealDbQueryExecutor {
        let mut conn = WasmRealDbConnection::new(
            "https://proxy.example.com/db",
            WasmTransport::Http,
            "sess-001",
            "valid-token",
            SerializationFormat::Json,
        );
        conn.connect().unwrap();

        let whitelist = WasmDbSqlWhitelist::new();
        let rate_limiter = WasmDbRateLimiter::new(100);
        let mut auth = WasmDbAuthValidator::new();
        auth.add_token("valid-token");

        WasmRealDbQueryExecutor::new(conn, whitelist, rate_limiter, auth, max_result_rows)
    }

    fn make_ok_response(rows: Vec<serde_json::Value>) -> ProxyResponse {
        ProxyResponse {
            status: ProxyStatus::Ok,
            rows,
            rows_affected: None,
            error: None,
            latency_ms: 10,
        }
    }

    #[test]
    fn test_executor_creation() {
        let exec = make_executor(1000);
        assert_eq!(exec.max_result_rows(), 1000);
        assert!(exec.connection().is_connected());
    }

    #[test]
    fn test_process_response_success() {
        let mut exec = make_executor(1000);
        let query = WasmQuery::new("SELECT * FROM users");
        let resp = make_ok_response(vec![serde_json::json!({"id": 1})]);
        let bytes = serde_json::to_vec(&resp).unwrap();
        let result = exec.process_response_bytes(&query, &bytes).unwrap();
        assert_eq!(result.status, ProxyStatus::Ok);
        assert_eq!(result.rows.len(), 1);
        assert_eq!(exec.metrics().total_queries(), 1);
    }

    #[test]
    fn test_process_response_not_connected() {
        let mut exec = make_executor(1000);
        exec.connection_mut().close();
        let query = WasmQuery::new("SELECT * FROM users");
        let bytes = vec![];
        let result = exec.process_response_bytes(&query, &bytes);
        assert!(matches!(result, Err(WasmRealDbError::ProxyUnavailable)));
    }

    #[test]
    fn test_process_response_auth_failed() {
        let mut conn = WasmRealDbConnection::new(
            "https://proxy.example.com/db",
            WasmTransport::Http,
            "sess",
            "invalid-token",
            SerializationFormat::Json,
        );
        conn.connect().unwrap();
        let whitelist = WasmDbSqlWhitelist::new();
        let rate_limiter = WasmDbRateLimiter::new(100);
        let mut auth = WasmDbAuthValidator::new();
        auth.add_token("valid-token");

        let mut exec = WasmRealDbQueryExecutor::new(conn, whitelist, rate_limiter, auth, 1000);
        let query = WasmQuery::new("SELECT * FROM users");
        let resp = make_ok_response(vec![]);
        let bytes = serde_json::to_vec(&resp).unwrap();
        let result = exec.process_response_bytes(&query, &bytes);
        assert!(matches!(result, Err(WasmRealDbError::AuthFailed)));
    }

    #[test]
    fn test_process_response_sql_rejected() {
        let mut exec = make_executor(1000);
        let query = WasmQuery::new("DROP TABLE users");
        let resp = make_ok_response(vec![]);
        let bytes = serde_json::to_vec(&resp).unwrap();
        let result = exec.process_response_bytes(&query, &bytes);
        assert!(matches!(result, Err(WasmRealDbError::SqlRejected { .. })));
    }

    #[test]
    fn test_process_response_result_too_large() {
        let mut exec = make_executor(2);
        let query = WasmQuery::new("SELECT * FROM users");
        let resp = make_ok_response(vec![
            serde_json::json!({"id": 1}),
            serde_json::json!({"id": 2}),
            serde_json::json!({"id": 3}),
        ]);
        let bytes = serde_json::to_vec(&resp).unwrap();
        let result = exec.process_response_bytes(&query, &bytes);
        assert!(matches!(result, Err(WasmRealDbError::ResultTooLarge)));
    }

    #[test]
    fn test_process_response_proxy_error() {
        let mut exec = make_executor(1000);
        let query = WasmQuery::new("SELECT * FROM users");
        let resp = ProxyResponse {
            status: ProxyStatus::Error,
            rows: vec![],
            rows_affected: None,
            error: Some(super::super::protocol::ProxyError::QueryFailed {
                reason: "syntax error".to_string(),
            }),
            latency_ms: 0,
        };
        let bytes = serde_json::to_vec(&resp).unwrap();
        let result = exec.process_response_bytes(&query, &bytes);
        assert!(matches!(result, Err(WasmRealDbError::QueryFailed { .. })));
    }

    #[test]
    fn test_metrics_recorded() {
        let mut exec = make_executor(1000);
        let query = WasmQuery::new("SELECT * FROM users");
        let resp = make_ok_response(vec![]);
        let bytes = serde_json::to_vec(&resp).unwrap();
        exec.process_response_bytes(&query, &bytes).unwrap();
        assert_eq!(exec.metrics().total_queries(), 1);
        assert_eq!(exec.metrics().total_errors(), 0);
        assert_eq!(exec.metrics().total_latency_ms(), 10);
    }

    #[test]
    fn test_reset_metrics() {
        let mut exec = make_executor(1000);
        let query = WasmQuery::new("SELECT * FROM users");
        let resp = make_ok_response(vec![]);
        let bytes = serde_json::to_vec(&resp).unwrap();
        exec.process_response_bytes(&query, &bytes).unwrap();
        assert_eq!(exec.metrics().total_queries(), 1);
        exec.reset_metrics();
        assert_eq!(exec.metrics().total_queries(), 0);
    }

    #[test]
    fn test_rate_limited() {
        let mut conn = WasmRealDbConnection::new(
            "https://proxy.example.com/db",
            WasmTransport::Http,
            "sess",
            "valid-token",
            SerializationFormat::Json,
        );
        conn.connect().unwrap();
        let whitelist = WasmDbSqlWhitelist::new();
        let rate_limiter = WasmDbRateLimiter::new(2);
        let mut auth = WasmDbAuthValidator::new();
        auth.add_token("valid-token");

        let mut exec = WasmRealDbQueryExecutor::new(conn, whitelist, rate_limiter, auth, 1000);

        let query = WasmQuery::new("SELECT * FROM users");
        let resp = make_ok_response(vec![]);
        let bytes = serde_json::to_vec(&resp).unwrap();

        let r1 = exec.process_response_bytes(&query, &bytes);
        let r2 = exec.process_response_bytes(&query, &bytes);
        let r3 = exec.process_response_bytes(&query, &bytes);

        assert!(r1.is_ok());
        assert!(r2.is_ok());
        assert!(matches!(r3, Err(WasmRealDbError::RateLimited)));
    }
}
