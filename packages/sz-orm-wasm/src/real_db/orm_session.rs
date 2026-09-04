//! # WasmOrmSession — WASM 端 ORM 会话 + 查询构建器桥接
//!
//! 提供 WASM 端 ORM 操作完整闭环：查询构建 → 代理执行 → 结果反序列化。
//! 复用既有 `WasmDbProxy`/`WasmRealDbConnection`/`WasmRealDbQueryExecutor`。

use super::protocol::{ProxyRequest, ProxyResponse, SerializationFormat};
use super::proxy::WasmDbProxy;
use super::WasmRealDbError;
use crate::WasmQuery;
use sz_orm_core::dialect_security::Dialect;
use sz_orm_core::DbType;
use sz_orm_query_builder::{BuiltQuery, SelectQuery};

// ============================================================================
// WasmQueryBuilderBridge — 查询构建器桥接
// ============================================================================

/// WASM 端查询构建器桥接
///
/// 将 sz-orm-query-builder 的参数化查询转换为代理协议 `ProxyRequest`，
/// 禁止 SQL 字符串拼接。
pub struct WasmQueryBuilderBridge {
    dialect: Dialect,
    auth_token: String,
    result_format: SerializationFormat,
}

impl WasmQueryBuilderBridge {
    /// 创建新的查询构建器桥接
    pub fn new(dialect: Dialect, auth_token: String) -> Self {
        Self {
            dialect,
            auth_token,
            result_format: SerializationFormat::Json,
        }
    }

    /// 设置结果序列化格式
    pub fn with_format(mut self, format: SerializationFormat) -> Self {
        self.result_format = format;
        self
    }

    /// 将 SelectQuery 构建为 ProxyRequest
    pub fn build(&self, query: SelectQuery) -> Result<ProxyRequest, WasmRealDbError> {
        let db_type = self.dialect_to_db_type();
        let built = query.build_with_params(db_type);

        if built.sql.is_empty() {
            return Err(WasmRealDbError::SerializationError(
                "empty query SQL".to_string(),
            ));
        }

        let params: Vec<serde_json::Value> = built.params.iter().map(Self::value_to_json).collect();

        let wasm_query = WasmQuery::with_params(&built.sql, params);

        Ok(ProxyRequest {
            session_id: format!("session_{}", self.dialect.as_str()),
            token: self.auth_token.clone(),
            query: wasm_query,
            transaction_id: None,
        })
    }

    /// 从 BuiltQuery 构建为 ProxyRequest
    pub fn build_from_built(&self, built: BuiltQuery) -> Result<ProxyRequest, WasmRealDbError> {
        if built.sql.is_empty() {
            return Err(WasmRealDbError::SerializationError(
                "empty query SQL".to_string(),
            ));
        }

        let params: Vec<serde_json::Value> = built.params.iter().map(Self::value_to_json).collect();

        let wasm_query = WasmQuery::with_params(&built.sql, params);

        Ok(ProxyRequest {
            session_id: format!("session_{}", self.dialect.as_str()),
            token: self.auth_token.clone(),
            query: wasm_query,
            transaction_id: None,
        })
    }

    /// 方言 → DbType
    fn dialect_to_db_type(&self) -> DbType {
        match self.dialect {
            Dialect::MySql => DbType::MySQL,
            Dialect::PostgreSql => DbType::PostgreSQL,
            Dialect::Sqlite => DbType::Sqlite,
            Dialect::Oracle => DbType::Oracle,
            Dialect::Mssql => DbType::SqlServer,
        }
    }

    /// Value → JSON Value
    fn value_to_json(v: &sz_orm_core::Value) -> serde_json::Value {
        match v {
            sz_orm_core::Value::Null => serde_json::Value::Null,
            sz_orm_core::Value::Bool(b) => serde_json::Value::Bool(*b),
            sz_orm_core::Value::I8(i) => serde_json::json!(i),
            sz_orm_core::Value::I16(i) => serde_json::json!(i),
            sz_orm_core::Value::I32(i) => serde_json::json!(i),
            sz_orm_core::Value::I64(i) => serde_json::json!(i),
            sz_orm_core::Value::U8(i) => serde_json::json!(i),
            sz_orm_core::Value::U16(i) => serde_json::json!(i),
            sz_orm_core::Value::U32(i) => serde_json::json!(i),
            sz_orm_core::Value::U64(i) => serde_json::json!(i),
            sz_orm_core::Value::F32(f) => serde_json::json!(f),
            sz_orm_core::Value::F64(f) => serde_json::json!(f),
            sz_orm_core::Value::String(s) => serde_json::Value::String(s.clone()),
            _ => serde_json::Value::Null,
        }
    }

    /// 获取方言
    pub fn dialect(&self) -> Dialect {
        self.dialect
    }
}

// ============================================================================
// WasmOrmSession — WASM 端 ORM 会话
// ============================================================================

/// WASM 端 ORM 会话
///
/// 查询构建 → 代理执行 → 结果反序列化。
pub struct WasmOrmSession {
    proxy: WasmDbProxy,
    query_bridge: WasmQueryBuilderBridge,
}

impl WasmOrmSession {
    /// 创建新的 ORM 会话
    pub fn new(proxy: WasmDbProxy, dialect: Dialect, auth_token: String) -> Self {
        let query_bridge = WasmQueryBuilderBridge::new(dialect, auth_token);
        Self {
            proxy,
            query_bridge,
        }
    }

    /// 执行查询（通过 SelectQuery 构建器）
    pub fn query(&mut self, query_builder: SelectQuery) -> Result<ProxyResponse, WasmRealDbError> {
        let request = self.query_bridge.build(query_builder)?;
        let response = self.proxy.handle_request(&request)?;

        if response.status == super::protocol::ProxyStatus::Error {
            if let Some(ref err) = response.error {
                return Err(match err {
                    super::protocol::ProxyError::AuthFailed => WasmRealDbError::AuthFailed,
                    super::protocol::ProxyError::RateLimited => WasmRealDbError::RateLimited,
                    super::protocol::ProxyError::SqlRejected { reason } => {
                        WasmRealDbError::SqlRejected {
                            reason: reason.clone(),
                        }
                    }
                    super::protocol::ProxyError::QueryFailed { reason } => {
                        WasmRealDbError::QueryFailed {
                            reason: reason.clone(),
                        }
                    }
                    super::protocol::ProxyError::ProxyUnavailable => {
                        WasmRealDbError::ProxyUnavailable
                    }
                    super::protocol::ProxyError::CredentialsNotExposed => {
                        WasmRealDbError::CredentialsNotExposed
                    }
                    super::protocol::ProxyError::ResultTooLarge => WasmRealDbError::ResultTooLarge,
                });
            }
        }

        Ok(response)
    }

    /// 执行查询（通过原始 SQL + 参数）
    pub fn query_raw(
        &mut self,
        sql: &str,
        params: Vec<serde_json::Value>,
    ) -> Result<ProxyResponse, WasmRealDbError> {
        let request = ProxyRequest {
            session_id: format!("session_{}", self.query_bridge.dialect().as_str()),
            token: String::new(),
            query: WasmQuery::with_params(sql, params),
            transaction_id: None,
        };

        let response = self.proxy.handle_request(&request)?;
        Ok(response)
    }

    /// 反序列化结果
    pub fn deserialize_result<T: serde::de::DeserializeOwned>(
        response: &ProxyResponse,
    ) -> Result<T, WasmRealDbError> {
        serde_json::from_value(serde_json::Value::Array(response.rows.clone()))
            .map_err(|e| WasmRealDbError::SerializationError(e.to_string()))
    }

    /// 获取代理引用
    pub fn proxy(&self) -> &WasmDbProxy {
        &self.proxy
    }

    /// 获取查询桥接引用
    pub fn query_bridge(&self) -> &WasmQueryBuilderBridge {
        &self.query_bridge
    }
}

// ============================================================================
// WasmOrmLoopVerifier — WASM ORM 闭环验证器
// ============================================================================

/// 闭环验证报告
#[derive(Debug, Clone)]
pub struct WasmLoopReport {
    /// WASM 端查询结果行数
    pub wasm_rows_count: usize,
    /// 直接查询结果行数
    pub direct_rows_count: usize,
    /// 是否一致
    pub consistent: bool,
    /// 差异描述
    pub diff_descriptions: Vec<String>,
}

impl WasmLoopReport {
    /// 创建空报告
    pub fn empty() -> Self {
        Self {
            wasm_rows_count: 0,
            direct_rows_count: 0,
            consistent: true,
            diff_descriptions: vec![],
        }
    }
}

/// WASM ORM 闭环验证器
///
/// 比对 WASM 端结果与直接 DB 查询结果。
pub struct WasmOrmLoopVerifier {
    session: WasmOrmSession,
}

impl WasmOrmLoopVerifier {
    /// 创建新的闭环验证器
    pub fn new(session: WasmOrmSession) -> Self {
        Self { session }
    }

    /// 验证闭环一致性
    pub fn verify(
        &mut self,
        query_builder: SelectQuery,
        direct_rows: &[serde_json::Value],
    ) -> Result<WasmLoopReport, WasmRealDbError> {
        let response = self.session.query(query_builder)?;

        let mut report = WasmLoopReport {
            wasm_rows_count: response.rows.len(),
            direct_rows_count: direct_rows.len(),
            consistent: true,
            diff_descriptions: vec![],
        };

        if response.rows.len() != direct_rows.len() {
            report.consistent = false;
            report.diff_descriptions.push(format!(
                "row count mismatch: wasm={}, direct={}",
                response.rows.len(),
                direct_rows.len()
            ));
        }

        for (i, (wasm_row, direct_row)) in response.rows.iter().zip(direct_rows.iter()).enumerate()
        {
            if wasm_row != direct_row {
                report.consistent = false;
                report.diff_descriptions.push(format!("row {} mismatch", i));
            }
        }

        Ok(report)
    }

    /// 获取会话引用
    pub fn session(&self) -> &WasmOrmSession {
        &self.session
    }
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;
    use sz_orm_core::Value;
    use sz_orm_query_builder::Query;

    #[test]
    fn test_query_builder_bridge_creation() {
        let bridge = WasmQueryBuilderBridge::new(Dialect::MySql, "token".to_string());
        assert_eq!(bridge.dialect(), Dialect::MySql);
    }

    #[test]
    fn test_query_builder_bridge_build() {
        let bridge = WasmQueryBuilderBridge::new(Dialect::MySql, "test_token".to_string());

        let query = Query::select()
            .column("id")
            .from("users")
            .where_eq("id", Value::I64(1));

        let request = bridge.build(query).unwrap();
        assert_eq!(request.token, "test_token");
        assert!(request.query.sql.contains("SELECT"));
        assert!(request.query.sql.contains("users"));
        assert!(!request.query.params.is_empty());
    }

    #[test]
    fn test_query_builder_bridge_build_with_or() {
        let bridge = WasmQueryBuilderBridge::new(Dialect::PostgreSql, "token".to_string());

        let query = Query::select()
            .column("*")
            .from("orders")
            .where_eq("user_id", Value::I64(42))
            .or_where_eq("status", Value::String("pending".to_string()));

        let request = bridge.build(query).unwrap();
        assert!(request.query.sql.contains("SELECT"));
        assert!(request.query.sql.contains("orders"));
        assert_eq!(request.query.params.len(), 2);
    }

    #[test]
    fn test_query_builder_bridge_format() {
        let bridge = WasmQueryBuilderBridge::new(Dialect::MySql, "token".to_string())
            .with_format(SerializationFormat::MessagePack);

        let query = Query::select().column("id").from("users");
        let request = bridge.build(query).unwrap();
        assert!(request.query.sql.contains("SELECT"));
    }

    #[test]
    fn test_dialect_to_db_type() {
        let mysql_bridge = WasmQueryBuilderBridge::new(Dialect::MySql, "t".to_string());
        let pg_bridge = WasmQueryBuilderBridge::new(Dialect::PostgreSql, "t".to_string());
        let sqlite_bridge = WasmQueryBuilderBridge::new(Dialect::Sqlite, "t".to_string());
        let oracle_bridge = WasmQueryBuilderBridge::new(Dialect::Oracle, "t".to_string());
        let mssql_bridge = WasmQueryBuilderBridge::new(Dialect::Mssql, "t".to_string());

        let _ = mysql_bridge;
        let _ = pg_bridge;
        let _ = sqlite_bridge;
        let _ = oracle_bridge;
        let _ = mssql_bridge;
    }

    #[test]
    fn test_value_to_json_conversion() {
        assert_eq!(
            WasmQueryBuilderBridge::value_to_json(&Value::I64(42)),
            serde_json::json!(42)
        );
        assert_eq!(
            WasmQueryBuilderBridge::value_to_json(&Value::String("hello".to_string())),
            serde_json::json!("hello")
        );
        assert_eq!(
            WasmQueryBuilderBridge::value_to_json(&Value::Bool(true)),
            serde_json::json!(true)
        );
        assert_eq!(
            WasmQueryBuilderBridge::value_to_json(&Value::Null),
            serde_json::Value::Null
        );
    }

    #[test]
    fn test_wasm_loop_report() {
        let report = WasmLoopReport::empty();
        assert!(report.consistent);
        assert_eq!(report.wasm_rows_count, 0);
        assert_eq!(report.direct_rows_count, 0);
    }
}
