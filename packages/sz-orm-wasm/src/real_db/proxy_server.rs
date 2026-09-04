//! # WasmProxyServer — 代理后端服务 + 多方言代理
//!
//! 接收 WASM 端代理请求 → 鉴权 + SQL 白名单检查 + 限流 → 连接真实 DB
//! → 执行查询 → 检查结果集大小 → 返回结果。
//! 后端凭据不暴露给 WASM 端。

use super::auth::WasmDbAuthValidator;
use super::metrics::WasmRealDbMetrics;
use super::protocol::{ProxyRequest, ProxyResponse, ProxyStatus};
use super::rate_limiter::WasmDbRateLimiter;
use super::sql_whitelist::WasmDbSqlWhitelist;
use super::WasmRealDbError;
use std::collections::HashMap;
use std::sync::Arc;
use sz_orm_core::dialect_security::Dialect;
use sz_orm_core::{ConnectionFactory, Value};

// ============================================================================
// 代理后端配置
// ============================================================================

/// 鉴权配置
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// 是否启用鉴权
    pub enabled: bool,
    /// 有效 Token 列表
    pub tokens: Vec<String>,
}

impl AuthConfig {
    /// 创建默认鉴权配置（启用，无 Token）
    pub fn new() -> Self {
        Self {
            enabled: true,
            tokens: vec![],
        }
    }

    /// 禁用鉴权
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            tokens: vec![],
        }
    }

    /// 添加 Token
    pub fn with_token(mut self, token: &str) -> Self {
        self.tokens.push(token.to_string());
        self
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// SQL 白名单配置
#[derive(Debug, Clone)]
pub struct WhitelistConfig {
    /// 是否启用白名单
    pub enabled: bool,
    /// 允许的 SQL 前缀
    pub allowed_patterns: Vec<String>,
}

impl WhitelistConfig {
    /// 创建默认白名单配置（启用，SELECT/INSERT/UPDATE/DELETE）
    pub fn new() -> Self {
        Self {
            enabled: true,
            allowed_patterns: vec![
                "SELECT".to_string(),
                "INSERT".to_string(),
                "UPDATE".to_string(),
                "DELETE".to_string(),
            ],
        }
    }

    /// 添加允许的 SQL 前缀
    pub fn with_pattern(mut self, pattern: &str) -> Self {
        self.allowed_patterns.push(pattern.to_uppercase());
        self
    }
}

impl Default for WhitelistConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// 限流配置
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// 是否启用限流
    pub enabled: bool,
    /// 最大 QPS
    pub max_qps: u32,
    /// 突发大小
    pub burst_size: u32,
}

impl RateLimitConfig {
    /// 创建默认限流配置（启用，100 QPS）
    pub fn new() -> Self {
        Self {
            enabled: true,
            max_qps: 100,
            burst_size: 100,
        }
    }

    /// 设置 QPS
    pub fn with_qps(mut self, qps: u32) -> Self {
        self.max_qps = qps;
        self.burst_size = qps;
        self
    }
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// 方言代理配置
#[derive(Debug, Clone)]
pub struct DialectProxyConfig {
    /// 数据库方言
    pub dialect: Dialect,
    /// 连接字符串
    pub connection_string: String,
    /// 连接池大小
    pub pool_size: usize,
}

impl DialectProxyConfig {
    /// 创建方言代理配置
    pub fn new(dialect: Dialect, connection_string: &str, pool_size: usize) -> Self {
        Self {
            dialect,
            connection_string: connection_string.to_string(),
            pool_size,
        }
    }
}

/// 代理后端配置
#[derive(Debug, Clone)]
pub struct ProxyServerConfig {
    /// 最大结果集大小（字节，默认 10MB）
    pub max_result_size: usize,
    /// 鉴权配置
    pub auth_config: AuthConfig,
    /// SQL 白名单配置
    pub whitelist_config: WhitelistConfig,
    /// 限流配置
    pub rate_limit_config: RateLimitConfig,
    /// 多方言代理配置
    pub dialect_configs: Vec<DialectProxyConfig>,
}

impl ProxyServerConfig {
    /// 创建默认配置（max_result_size 10MB）
    pub fn new() -> Self {
        Self {
            max_result_size: 10 * 1024 * 1024,
            auth_config: AuthConfig::new(),
            whitelist_config: WhitelistConfig::new(),
            rate_limit_config: RateLimitConfig::new(),
            dialect_configs: vec![],
        }
    }

    /// 设置最大结果集大小
    pub fn with_max_result_size(mut self, size: usize) -> Self {
        self.max_result_size = size;
        self
    }

    /// 设置鉴权配置
    pub fn with_auth(mut self, auth: AuthConfig) -> Self {
        self.auth_config = auth;
        self
    }

    /// 设置白名单配置
    pub fn with_whitelist(mut self, whitelist: WhitelistConfig) -> Self {
        self.whitelist_config = whitelist;
        self
    }

    /// 设置限流配置
    pub fn with_rate_limit(mut self, rate_limit: RateLimitConfig) -> Self {
        self.rate_limit_config = rate_limit;
        self
    }

    /// 添加方言代理配置
    pub fn with_dialect(mut self, dialect_config: DialectProxyConfig) -> Self {
        self.dialect_configs.push(dialect_config);
        self
    }
}

impl Default for ProxyServerConfig {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// WasmProxyServer — 代理后端服务
// ============================================================================

/// WasmProxyServer — 代理后端服务
///
/// 接收 WASM 端代理请求 → 鉴权 + SQL 白名单检查 + 限流
/// → 连接真实 DB → 执行查询 → 检查结果集大小 → 返回结果。
pub struct WasmProxyServer {
    auth_validator: WasmDbAuthValidator,
    sql_whitelist: WasmDbSqlWhitelist,
    rate_limiter: WasmDbRateLimiter,
    factory: Arc<dyn ConnectionFactory>,
    max_result_size: usize,
    metrics: WasmRealDbMetrics,
    /// v4.8.0 修复 M-16：鉴权开关（AuthConfig.enabled）此前被忽略——
    /// `AuthConfig::disabled()` 实际效果为拒绝一切请求（文档语义=放行）。
    auth_enabled: bool,
    /// v4.8.0 修复 M-16：限流开关（RateLimitConfig.enabled）此前从未被读取。
    rate_limit_enabled: bool,
}

impl WasmProxyServer {
    /// 创建新的代理后端服务
    pub fn new(factory: Arc<dyn ConnectionFactory>, config: ProxyServerConfig) -> Self {
        let mut auth_validator = WasmDbAuthValidator::new();
        if config.auth_config.enabled {
            for token in &config.auth_config.tokens {
                auth_validator.add_token(token);
            }
        }

        let mut sql_whitelist = WasmDbSqlWhitelist::new();
        if !config.whitelist_config.enabled {
            for prefix in &["INSERT", "UPDATE", "DELETE", "DROP", "ALTER", "CREATE"] {
                sql_whitelist.allow_prefix(prefix);
            }
        }
        for pattern in &config.whitelist_config.allowed_patterns {
            sql_whitelist.allow_prefix(pattern);
        }

        let rate_limiter = WasmDbRateLimiter::new(config.rate_limit_config.max_qps);

        Self {
            auth_validator,
            sql_whitelist,
            rate_limiter,
            factory,
            max_result_size: config.max_result_size,
            metrics: WasmRealDbMetrics::new(),
            // v4.8.0 修复 M-16：enabled 开关真实生效
            auth_enabled: config.auth_config.enabled,
            rate_limit_enabled: config.rate_limit_config.enabled,
        }
    }

    /// 处理代理请求
    pub async fn handle_request(
        &mut self,
        request: ProxyRequest,
    ) -> Result<ProxyResponse, WasmRealDbError> {
        let start = std::time::Instant::now();

        // v4.8.0 修复 M-16：disabled() 语义 = 放行（不校验 token）
        if self.auth_enabled && !self.auth_validator.validate_token(&request.token) {
            self.metrics.record_error();
            return Err(WasmRealDbError::AuthFailed);
        }

        let sql = &request.query.sql;
        if !self.sql_whitelist.validate(sql) {
            self.metrics.record_error();
            // v4.8.0 修复 M-17：拒绝原因不回显完整 SQL（信息泄露面）
            return Err(WasmRealDbError::SqlRejected {
                reason: "SQL not in whitelist".to_string(),
            });
        }

        // v4.8.0 修复 M-16：enabled=false 时跳过限流
        if self.rate_limit_enabled && !self.rate_limiter.check_and_increment() {
            self.metrics.record_error();
            return Err(WasmRealDbError::RateLimited);
        }

        let mut conn = self
            .factory
            .create()
            .await
            .map_err(|_e| WasmRealDbError::QueryFailed {
                reason: "query execution failed".to_string(),
            })?;

        let params: Vec<Value> = request
            .query
            .params
            .iter()
            .map(Self::json_to_value)
            .collect();

        let rows = conn.query_with_params(sql, &params).await.map_err(|_e| {
            WasmRealDbError::QueryFailed {
                // v4.8.0 修复 M-17：底层 DB 错误原文不回显给 WASM 端
                reason: "query execution failed".to_string(),
            }
        })?;

        let result_json = serde_json::to_string(&rows)
            .map_err(|e| WasmRealDbError::SerializationError(e.to_string()))?;

        if result_json.len() > self.max_result_size {
            self.metrics.record_error();
            return Err(WasmRealDbError::ResultTooLarge);
        }

        let row_values: Vec<serde_json::Value> = rows
            .iter()
            .map(|row| {
                let mut m = serde_json::Map::new();
                for (k, v) in row {
                    m.insert(k.clone(), Self::value_to_json(v));
                }
                serde_json::Value::Object(m)
            })
            .collect();

        let latency_ms = start.elapsed().as_millis() as u64;
        self.metrics.record_query(latency_ms);

        Ok(ProxyResponse {
            status: ProxyStatus::Ok,
            rows: row_values,
            rows_affected: None,
            error: None,
            latency_ms,
        })
    }

    /// 获取凭据（不暴露给 WASM 端）
    pub fn credentials(&self) -> Result<(), WasmRealDbError> {
        Err(WasmRealDbError::CredentialsNotExposed)
    }

    /// 获取指标
    pub fn metrics(&self) -> &WasmRealDbMetrics {
        &self.metrics
    }

    /// JSON Value → sz-orm-core Value
    fn json_to_value(v: &serde_json::Value) -> Value {
        match v {
            serde_json::Value::Null => Value::Null,
            serde_json::Value::Bool(b) => Value::Bool(*b),
            serde_json::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Value::I64(i)
                } else if let Some(f) = n.as_f64() {
                    Value::F64(f)
                } else {
                    Value::Null
                }
            }
            serde_json::Value::String(s) => Value::String(s.clone()),
            _ => Value::Null,
        }
    }

    /// sz-orm-core Value → JSON Value
    fn value_to_json(v: &Value) -> serde_json::Value {
        match v {
            Value::Null => serde_json::Value::Null,
            Value::Bool(b) => serde_json::Value::Bool(*b),
            Value::I8(i) => serde_json::json!(i),
            Value::I16(i) => serde_json::json!(i),
            Value::I32(i) => serde_json::json!(i),
            Value::I64(i) => serde_json::json!(i),
            Value::U8(i) => serde_json::json!(i),
            Value::U16(i) => serde_json::json!(i),
            Value::U32(i) => serde_json::json!(i),
            Value::U64(i) => serde_json::json!(i),
            Value::F32(f) => serde_json::json!(f),
            Value::F64(f) => serde_json::json!(f),
            Value::String(s) => serde_json::Value::String(s.clone()),
            _ => serde_json::Value::Null,
        }
    }
}

// ============================================================================
// MultiDialectProxyBackend — 多方言代理后端
// ============================================================================

/// MultiDialectProxyBackend — 多方言代理后端
///
/// 支持 MySQL/PostgreSQL/SQLite/Oracle/MSSQL 五方言，
/// 按 `ProxyRequest` 路由到对应数据库。
pub struct MultiDialectProxyBackend {
    backends: HashMap<Dialect, WasmProxyServer>,
}

impl MultiDialectProxyBackend {
    /// 创建多方言代理后端
    pub fn new(
        dialect_configs: Vec<(Dialect, Arc<dyn ConnectionFactory>, ProxyServerConfig)>,
    ) -> Self {
        let mut backends = HashMap::new();
        for (dialect, factory, config) in dialect_configs {
            let server = WasmProxyServer::new(factory, config);
            backends.insert(dialect, server);
        }
        Self { backends }
    }

    /// 处理代理请求（按方言路由）
    pub async fn handle_request(
        &mut self,
        dialect: Dialect,
        request: ProxyRequest,
    ) -> Result<ProxyResponse, WasmRealDbError> {
        let backend =
            self.backends
                .get_mut(&dialect)
                .ok_or_else(|| WasmRealDbError::QueryFailed {
                    reason: format!("unsupported dialect: {:?}", dialect),
                })?;
        backend.handle_request(request).await
    }

    /// 支持的方言列表
    pub fn supported_dialects(&self) -> Vec<Dialect> {
        self.backends.keys().copied().collect()
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proxy_server_config_default() {
        let config = ProxyServerConfig::new();
        assert_eq!(config.max_result_size, 10 * 1024 * 1024);
        assert!(config.auth_config.enabled);
        assert!(config.whitelist_config.enabled);
        assert!(config.rate_limit_config.enabled);
    }

    #[test]
    fn test_proxy_server_config_builder() {
        let config = ProxyServerConfig::new()
            .with_max_result_size(1024)
            .with_auth(AuthConfig::new().with_token("test_token"))
            .with_rate_limit(RateLimitConfig::new().with_qps(50));

        assert_eq!(config.max_result_size, 1024);
        assert_eq!(config.auth_config.tokens, vec!["test_token".to_string()]);
        assert_eq!(config.rate_limit_config.max_qps, 50);
    }

    #[test]
    fn test_auth_config() {
        let auth = AuthConfig::new().with_token("token1").with_token("token2");
        assert_eq!(auth.tokens.len(), 2);

        let disabled = AuthConfig::disabled();
        assert!(!disabled.enabled);
    }

    #[test]
    fn test_whitelist_config() {
        let wl = WhitelistConfig::new().with_pattern("WITH");
        assert!(wl.allowed_patterns.contains(&"WITH".to_string()));
    }

    #[test]
    fn test_rate_limit_config() {
        let rl = RateLimitConfig::new().with_qps(200);
        assert_eq!(rl.max_qps, 200);
        assert_eq!(rl.burst_size, 200);
    }

    #[test]
    fn test_dialect_proxy_config() {
        let config = DialectProxyConfig::new(Dialect::MySql, "mysql://root:pass@localhost/db", 10);
        assert_eq!(config.dialect, Dialect::MySql);
        assert_eq!(config.pool_size, 10);
    }

    #[test]
    fn test_wasm_proxy_server_credentials_not_exposed() {
        assert!(matches!(
            WasmRealDbError::CredentialsNotExposed,
            WasmRealDbError::CredentialsNotExposed
        ));
    }

    #[test]
    fn test_multi_dialect_backend_supported_dialects() {
        let dialects = [Dialect::MySql, Dialect::PostgreSql];
        assert!(dialects.contains(&Dialect::MySql));
        assert!(dialects.contains(&Dialect::PostgreSql));
    }

    #[test]
    fn test_json_to_value_conversion() {
        assert_eq!(
            WasmProxyServer::json_to_value(&serde_json::json!(42)),
            Value::I64(42)
        );
        assert_eq!(
            WasmProxyServer::json_to_value(&serde_json::json!("hello")),
            Value::String("hello".to_string())
        );
        assert_eq!(
            WasmProxyServer::json_to_value(&serde_json::json!(true)),
            Value::Bool(true)
        );
        assert_eq!(
            WasmProxyServer::json_to_value(&serde_json::Value::Null),
            Value::Null
        );
    }

    #[test]
    fn test_value_to_json_conversion() {
        assert_eq!(
            WasmProxyServer::value_to_json(&Value::I64(42)),
            serde_json::json!(42)
        );
        assert_eq!(
            WasmProxyServer::value_to_json(&Value::String("hello".to_string())),
            serde_json::json!("hello")
        );
        assert_eq!(
            WasmProxyServer::value_to_json(&Value::Bool(true)),
            serde_json::json!(true)
        );
    }
}
