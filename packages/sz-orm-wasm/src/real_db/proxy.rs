//! WasmDbProxy — 后端代理
//!
//! 后端代理逻辑：接收 WASM 端请求，转发到真实 DB。
//! DB 凭据仅在代理端持有，不暴露给 WASM。

use super::auth::WasmDbAuthValidator;
use super::protocol::{ProxyError, ProxyRequest, ProxyResponse, ProxyStatus};
use super::rate_limiter::WasmDbRateLimiter;
use super::sql_whitelist::WasmDbSqlWhitelist;
use super::WasmRealDbError;

/// DB 凭据（仅后端代理持有，不暴露给 WASM 端）
#[derive(Debug, Clone)]
pub struct DbCredentials {
    host: String,
    port: u16,
    username: String,
    password: String,
    database: String,
}

impl DbCredentials {
    /// 创建 DB 凭据
    pub fn new(host: &str, port: u16, username: &str, password: &str, database: &str) -> Self {
        Self {
            host: host.to_string(),
            port,
            username: username.to_string(),
            password: password.to_string(),
            database: database.to_string(),
        }
    }

    /// 主机
    pub fn host(&self) -> &str {
        &self.host
    }

    /// 端口
    pub fn port(&self) -> u16 {
        self.port
    }

    /// 用户名
    pub fn username(&self) -> &str {
        &self.username
    }

    /// 密码（仅在代理端可访问）
    pub fn password(&self) -> &str {
        &self.password
    }

    /// 数据库名
    pub fn database(&self) -> &str {
        &self.database
    }

    /// 构建连接 URL（不包含密码）
    pub fn url_safe(&self) -> String {
        format!(
            "{}@{}:{}/{}",
            self.username, self.host, self.port, self.database
        )
    }
}

/// WASM DB 后端代理
///
/// 在服务端运行，接收 WASM 端的 ProxyRequest，
/// 鉴权 + 限流 + 白名单检查后转发到真实 DB。
pub struct WasmDbProxy {
    credentials: DbCredentials,
    auth_validator: WasmDbAuthValidator,
    rate_limiter: WasmDbRateLimiter,
    sql_whitelist: WasmDbSqlWhitelist,
    max_result_rows: usize,
}

impl WasmDbProxy {
    /// 创建新代理
    pub fn new(
        credentials: DbCredentials,
        auth_validator: WasmDbAuthValidator,
        rate_limiter: WasmDbRateLimiter,
        sql_whitelist: WasmDbSqlWhitelist,
        max_result_rows: usize,
    ) -> Self {
        Self {
            credentials,
            auth_validator,
            rate_limiter,
            sql_whitelist,
            max_result_rows,
        }
    }

    /// DB 凭据
    pub fn credentials(&self) -> &DbCredentials {
        &self.credentials
    }

    /// 处理代理请求
    ///
    /// 执行鉴权 → 限流 → SQL 白名单检查 → 返回响应。
    /// 实际的 DB 查询由调用方在外部执行，本方法仅做安全检查。
    pub fn handle_request(
        &mut self,
        request: &ProxyRequest,
    ) -> Result<ProxyResponse, WasmRealDbError> {
        if !self.auth_validator.validate_token(&request.token) {
            return Ok(ProxyResponse {
                status: ProxyStatus::Error,
                rows: vec![],
                rows_affected: None,
                error: Some(ProxyError::AuthFailed),
                latency_ms: 0,
            });
        }

        if !self.auth_validator.validate_session(&request.session_id) {
            return Ok(ProxyResponse {
                status: ProxyStatus::Error,
                rows: vec![],
                rows_affected: None,
                error: Some(ProxyError::AuthFailed),
                latency_ms: 0,
            });
        }

        if !self.rate_limiter.check_and_increment() {
            return Ok(ProxyResponse {
                status: ProxyStatus::Error,
                rows: vec![],
                rows_affected: None,
                error: Some(ProxyError::RateLimited),
                latency_ms: 0,
            });
        }

        if !self.sql_whitelist.validate(&request.query.sql) {
            return Ok(ProxyResponse {
                status: ProxyStatus::Error,
                rows: vec![],
                rows_affected: None,
                error: Some(ProxyError::SqlRejected {
                    reason: format!("SQL not allowed: {}", request.query.sql),
                }),
                latency_ms: 0,
            });
        }

        Ok(ProxyResponse {
            status: ProxyStatus::Ok,
            rows: vec![],
            rows_affected: Some(0),
            error: None,
            latency_ms: 0,
        })
    }

    /// 最大结果行数
    pub fn max_result_rows(&self) -> usize {
        self.max_result_rows
    }

    /// 验证结果集大小
    pub fn validate_result_size(&self, rows: &[serde_json::Value]) -> Result<(), WasmRealDbError> {
        if rows.len() > self.max_result_rows {
            return Err(WasmRealDbError::ResultTooLarge);
        }
        Ok(())
    }
}

impl std::fmt::Debug for WasmDbProxy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WasmDbProxy")
            .field("credentials", &self.credentials.url_safe())
            .field("max_result_rows", &self.max_result_rows)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WasmQuery;

    fn make_proxy(max_qps: u32) -> WasmDbProxy {
        let creds = DbCredentials::new("localhost", 3306, "root", "secret", "test");
        let mut auth = WasmDbAuthValidator::new();
        auth.add_token("valid-token");
        auth.create_session("sess-1");
        let rl = WasmDbRateLimiter::new(max_qps);
        let wl = WasmDbSqlWhitelist::new();
        WasmDbProxy::new(creds, auth, rl, wl, 1000)
    }

    fn make_request(sql: &str) -> ProxyRequest {
        ProxyRequest {
            session_id: "sess-1".to_string(),
            token: "valid-token".to_string(),
            query: WasmQuery::new(sql),
            transaction_id: None,
        }
    }

    #[test]
    fn test_credentials_safe_url() {
        let creds = DbCredentials::new("localhost", 3306, "root", "secret", "test");
        let url = creds.url_safe();
        assert!(url.contains("root@localhost:3306/test"));
        assert!(!url.contains("secret"));
    }

    #[test]
    fn test_credentials_accessors() {
        let creds = DbCredentials::new("host", 5432, "user", "pass", "db");
        assert_eq!(creds.host(), "host");
        assert_eq!(creds.port(), 5432);
        assert_eq!(creds.username(), "user");
        assert_eq!(creds.password(), "pass");
        assert_eq!(creds.database(), "db");
    }

    #[test]
    fn test_proxy_allow_valid_request() {
        let mut proxy = make_proxy(100);
        let req = make_request("SELECT * FROM users");
        let resp = proxy.handle_request(&req).unwrap();
        assert_eq!(resp.status, ProxyStatus::Ok);
    }

    #[test]
    fn test_proxy_reject_invalid_token() {
        let mut proxy = make_proxy(100);
        let mut req = make_request("SELECT * FROM users");
        req.token = "invalid".to_string();
        let resp = proxy.handle_request(&req).unwrap();
        assert_eq!(resp.status, ProxyStatus::Error);
        assert!(matches!(resp.error, Some(ProxyError::AuthFailed)));
    }

    #[test]
    fn test_proxy_reject_invalid_session() {
        let mut proxy = make_proxy(100);
        let mut req = make_request("SELECT * FROM users");
        req.session_id = "unknown".to_string();
        let resp = proxy.handle_request(&req).unwrap();
        assert_eq!(resp.status, ProxyStatus::Error);
        assert!(matches!(resp.error, Some(ProxyError::AuthFailed)));
    }

    #[test]
    fn test_proxy_reject_sql() {
        let mut proxy = make_proxy(100);
        let req = make_request("DROP TABLE users");
        let resp = proxy.handle_request(&req).unwrap();
        assert_eq!(resp.status, ProxyStatus::Error);
        assert!(matches!(resp.error, Some(ProxyError::SqlRejected { .. })));
    }

    #[test]
    fn test_proxy_rate_limited() {
        let mut proxy = make_proxy(1);
        let req = make_request("SELECT * FROM users");
        let resp1 = proxy.handle_request(&req).unwrap();
        let resp2 = proxy.handle_request(&req).unwrap();
        assert_eq!(resp1.status, ProxyStatus::Ok);
        assert_eq!(resp2.status, ProxyStatus::Error);
        assert!(matches!(resp2.error, Some(ProxyError::RateLimited)));
    }

    #[test]
    fn test_validate_result_size_ok() {
        let proxy = make_proxy(100);
        let rows = vec![serde_json::json!({"id": 1}); 500];
        assert!(proxy.validate_result_size(&rows).is_ok());
    }

    #[test]
    fn test_validate_result_size_too_large() {
        let proxy = make_proxy(100);
        let rows = vec![serde_json::json!({"id": 1}); 1001];
        assert!(matches!(
            proxy.validate_result_size(&rows),
            Err(WasmRealDbError::ResultTooLarge)
        ));
    }

    #[test]
    fn test_proxy_max_result_rows() {
        let proxy = make_proxy(100);
        assert_eq!(proxy.max_result_rows(), 1000);
    }
}
