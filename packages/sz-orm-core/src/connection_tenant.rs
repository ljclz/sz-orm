//! # 连接级多租户隔离
//!
//! 在同一连接池中连接绑定到特定租户（通过 `SET app.tenant_id = ?`），
//! 避免每租户独立池的资源开销。支持三种连接亲和策略 + RAII 守卫。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::db_type::DbType;
use crate::pool::Pool;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 连接级隔离机制
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionLevelIsolation {
    /// 通过 `SET app.tenant_id = ?` 设置租户上下文
    SetTenantId,
    /// Schema 隔离，路由到 `tenant_{id}_{table}`
    SchemaIsolation,
    /// 连接绑定，连接专属租户
    ConnectionBinding,
}

/// 连接亲和策略
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionAffinityPolicy {
    /// 严格亲和：仅使用绑定到该租户的连接
    Strict,
    /// 优先亲和：优先使用绑定连接，无可用时获取任意连接
    Preferred,
    /// 无亲和：任意连接，每次设置租户上下文
    None,
}

/// 连接级多租户配置
#[derive(Debug, Clone)]
pub struct ConnectionLevelTenantConfig {
    /// 隔离机制
    pub isolation: ConnectionLevelIsolation,
    /// 亲和策略
    pub affinity_policy: ConnectionAffinityPolicy,
    /// 亲和超时（毫秒）
    pub affinity_timeout_ms: u64,
    /// 数据库类型
    pub db_type: DbType,
}

impl ConnectionLevelTenantConfig {
    /// 创建默认配置（SetTenantId, Preferred, 5000ms）
    pub fn new(db_type: DbType) -> Self {
        Self {
            isolation: ConnectionLevelIsolation::SetTenantId,
            affinity_policy: ConnectionAffinityPolicy::Preferred,
            affinity_timeout_ms: 5_000,
            db_type,
        }
    }

    /// 设置隔离机制
    pub fn with_isolation(mut self, isolation: ConnectionLevelIsolation) -> Self {
        self.isolation = isolation;
        self
    }

    /// 设置亲和策略
    pub fn with_affinity_policy(mut self, policy: ConnectionAffinityPolicy) -> Self {
        self.affinity_policy = policy;
        self
    }

    /// 设置亲和超时（毫秒）
    pub fn with_affinity_timeout_ms(mut self, ms: u64) -> Self {
        self.affinity_timeout_ms = ms;
        self
    }
}

/// 租户错误类型
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TenantError {
    /// 无可用绑定连接
    NoBoundConnection,
    /// 篡改被拒绝
    TamperingRejected,
    /// 清理失败
    CleanupFailed,
    /// 不支持的方言
    UnsupportedDialect,
    /// 租户 ID 为空
    EmptyTenantId,
}

impl std::fmt::Display for TenantError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TenantError::NoBoundConnection => write!(f, "no connection bound to tenant"),
            TenantError::TamperingRejected => write!(f, "tenant context tampering rejected"),
            TenantError::CleanupFailed => write!(f, "tenant context cleanup failed"),
            TenantError::UnsupportedDialect => {
                write!(f, "unsupported dialect for SET app.tenant_id")
            }
            TenantError::EmptyTenantId => write!(f, "tenant_id is empty"),
        }
    }
}

impl std::error::Error for TenantError {}

/// 连接 ID 类型
pub type ConnectionId = u64;

/// 连接租户绑定记录
#[derive(Debug, Clone)]
pub struct TenantBinding {
    /// 连接 ID
    pub connection_id: ConnectionId,
    /// 绑定的租户 ID
    pub tenant_id: String,
    /// 绑定时间戳
    pub bound_at: u64,
}

/// 连接租户绑定器
pub struct ConnectionTenantBinder {
    pool: Arc<Pool>,
    config: ConnectionLevelTenantConfig,
    tenant_bindings: Mutex<HashMap<String, Vec<ConnectionId>>>,
    next_connection_id: Mutex<u64>,
}

impl ConnectionTenantBinder {
    /// 创建连接租户绑定器
    pub fn new(pool: Arc<Pool>, config: ConnectionLevelTenantConfig) -> Self {
        Self {
            pool,
            config,
            tenant_bindings: Mutex::new(HashMap::new()),
            next_connection_id: Mutex::new(1),
        }
    }

    /// 获取配置
    pub fn config(&self) -> &ConnectionLevelTenantConfig {
        &self.config
    }

    /// 判断是否支持 `SET app.tenant_id`
    pub fn supports_set_tenant_id(&self) -> bool {
        matches!(self.config.db_type, DbType::PostgreSQL | DbType::MySQL)
    }

    /// 生成 `SET app.tenant_id` SQL
    pub fn build_set_tenant_sql(&self, tenant_id: &str) -> String {
        format!("SET app.tenant_id = '{}'", tenant_id)
    }

    /// 生成清理租户上下文 SQL
    pub fn build_clear_tenant_sql(&self) -> String {
        "SET app.tenant_id = NULL".to_string()
    }

    /// 绑定连接到租户
    pub fn bind_connection(&self, tenant_id: &str) -> ConnectionId {
        let conn_id = {
            let mut next = self.next_connection_id.lock().unwrap();
            let id = *next;
            *next += 1;
            id
        };
        let mut bindings = self.tenant_bindings.lock().unwrap();
        bindings
            .entry(tenant_id.to_string())
            .or_default()
            .push(conn_id);
        conn_id
    }

    /// 查找绑定到指定租户的连接
    pub fn find_bound_connections(&self, tenant_id: &str) -> Vec<ConnectionId> {
        let bindings = self.tenant_bindings.lock().unwrap();
        bindings.get(tenant_id).cloned().unwrap_or_default()
    }

    /// 解绑连接
    pub fn unbind_connection(&self, tenant_id: &str, conn_id: ConnectionId) {
        let mut bindings = self.tenant_bindings.lock().unwrap();
        if let Some(conns) = bindings.get_mut(tenant_id) {
            conns.retain(|&id| id != conn_id);
        }
    }

    /// 获取绑定数量
    pub fn binding_count(&self, tenant_id: &str) -> usize {
        let bindings = self.tenant_bindings.lock().unwrap();
        bindings.get(tenant_id).map(|v| v.len()).unwrap_or(0)
    }

    /// 获取所有租户绑定
    pub fn all_bindings(&self) -> Vec<TenantBinding> {
        let bindings = self.tenant_bindings.lock().unwrap();
        let mut result = Vec::new();
        for (tenant_id, conn_ids) in bindings.iter() {
            for &conn_id in conn_ids {
                result.push(TenantBinding {
                    connection_id: conn_id,
                    tenant_id: tenant_id.clone(),
                    bound_at: now_ms(),
                });
            }
        }
        result
    }

    /// 获取连接池引用
    pub fn pool(&self) -> &Arc<Pool> {
        &self.pool
    }

    /// 验证租户 ID
    pub fn validate_tenant_id(&self, tenant_id: &str) -> Result<(), TenantError> {
        if tenant_id.is_empty() {
            return Err(TenantError::EmptyTenantId);
        }
        Ok(())
    }

    /// 确定实际隔离机制（处理方言降级）
    pub fn resolve_isolation(&self) -> ConnectionLevelIsolation {
        if self.config.isolation == ConnectionLevelIsolation::SetTenantId
            && !self.supports_set_tenant_id()
        {
            ConnectionLevelIsolation::SchemaIsolation
        } else {
            self.config.isolation.clone()
        }
    }
}

/// 租户连接守卫（RAII）
pub struct TenantConnectionGuard {
    binder: Arc<ConnectionTenantBinder>,
    tenant_id: String,
    connection_id: ConnectionId,
    active: bool,
}

impl TenantConnectionGuard {
    /// 创建守卫
    pub fn new(
        binder: Arc<ConnectionTenantBinder>,
        tenant_id: String,
        connection_id: ConnectionId,
    ) -> Self {
        Self {
            binder,
            tenant_id,
            connection_id,
            active: true,
        }
    }

    /// 获取租户 ID
    pub fn tenant_id(&self) -> &str {
        &self.tenant_id
    }

    /// 获取连接 ID
    pub fn connection_id(&self) -> ConnectionId {
        self.connection_id
    }

    /// 是否活跃
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// 获取清理 SQL
    pub fn clear_tenant_sql(&self) -> String {
        self.binder.build_clear_tenant_sql()
    }

    /// 手动释放（提前清理）
    pub fn release(&mut self) -> Result<(), TenantError> {
        if !self.active {
            return Ok(());
        }
        self.binder
            .unbind_connection(&self.tenant_id, self.connection_id);
        self.active = false;
        Ok(())
    }
}

impl Drop for TenantConnectionGuard {
    fn drop(&mut self) {
        if self.active {
            self.binder
                .unbind_connection(&self.tenant_id, self.connection_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pool::{ConnectionFactory, PoolConfigBuilder};

    struct MockFactory;

    #[async_trait::async_trait]
    impl ConnectionFactory for MockFactory {
        async fn create(&self) -> Result<Box<dyn crate::pool::Connection>, crate::DbError> {
            Err(crate::DbError::PoolError(
                crate::error::PoolError::InvalidConfig("mock".to_string()),
            ))
        }
    }

    fn make_pool() -> Arc<Pool> {
        let config = PoolConfigBuilder::new().max_size(1).build().unwrap();
        let factory: Arc<dyn ConnectionFactory> = Arc::new(MockFactory);
        Arc::new(Pool::new(config, factory).unwrap())
    }

    #[test]
    fn test_config_default() {
        let config = ConnectionLevelTenantConfig::new(DbType::PostgreSQL);
        assert_eq!(config.isolation, ConnectionLevelIsolation::SetTenantId);
        assert_eq!(config.affinity_policy, ConnectionAffinityPolicy::Preferred);
        assert_eq!(config.affinity_timeout_ms, 5_000);
        assert_eq!(config.db_type, DbType::PostgreSQL);
    }

    #[test]
    fn test_config_builder() {
        let config = ConnectionLevelTenantConfig::new(DbType::MySQL)
            .with_isolation(ConnectionLevelIsolation::ConnectionBinding)
            .with_affinity_policy(ConnectionAffinityPolicy::Strict)
            .with_affinity_timeout_ms(10_000);
        assert_eq!(
            config.isolation,
            ConnectionLevelIsolation::ConnectionBinding
        );
        assert_eq!(config.affinity_policy, ConnectionAffinityPolicy::Strict);
        assert_eq!(config.affinity_timeout_ms, 10_000);
    }

    #[test]
    fn test_isolation_serde() {
        let isolations = vec![
            ConnectionLevelIsolation::SetTenantId,
            ConnectionLevelIsolation::SchemaIsolation,
            ConnectionLevelIsolation::ConnectionBinding,
        ];
        for i in &isolations {
            let json = serde_json::to_string(i).unwrap();
            let decoded: ConnectionLevelIsolation = serde_json::from_str(&json).unwrap();
            assert_eq!(*i, decoded);
        }
    }

    #[test]
    fn test_affinity_policy_serde() {
        let policies = vec![
            ConnectionAffinityPolicy::Strict,
            ConnectionAffinityPolicy::Preferred,
            ConnectionAffinityPolicy::None,
        ];
        for p in &policies {
            let json = serde_json::to_string(p).unwrap();
            let decoded: ConnectionAffinityPolicy = serde_json::from_str(&json).unwrap();
            assert_eq!(*p, decoded);
        }
    }

    #[test]
    fn test_supports_set_tenant_id() {
        let pool = make_pool();
        let pg_config = ConnectionLevelTenantConfig::new(DbType::PostgreSQL);
        let pg_binder = ConnectionTenantBinder::new(pool.clone(), pg_config);
        assert!(pg_binder.supports_set_tenant_id());

        let mysql_config = ConnectionLevelTenantConfig::new(DbType::MySQL);
        let mysql_binder = ConnectionTenantBinder::new(pool.clone(), mysql_config);
        assert!(mysql_binder.supports_set_tenant_id());

        let sqlite_config = ConnectionLevelTenantConfig::new(DbType::Sqlite);
        let sqlite_binder = ConnectionTenantBinder::new(pool.clone(), sqlite_config);
        assert!(!sqlite_binder.supports_set_tenant_id());

        let oracle_config = ConnectionLevelTenantConfig::new(DbType::Oracle);
        let oracle_binder = ConnectionTenantBinder::new(pool, oracle_config);
        assert!(!oracle_binder.supports_set_tenant_id());
    }

    #[test]
    fn test_build_set_tenant_sql() {
        let pool = make_pool();
        let config = ConnectionLevelTenantConfig::new(DbType::PostgreSQL);
        let binder = ConnectionTenantBinder::new(pool, config);
        let sql = binder.build_set_tenant_sql("tenant_123");
        assert!(sql.contains("SET app.tenant_id"));
        assert!(sql.contains("tenant_123"));
    }

    #[test]
    fn test_build_clear_tenant_sql() {
        let pool = make_pool();
        let config = ConnectionLevelTenantConfig::new(DbType::PostgreSQL);
        let binder = ConnectionTenantBinder::new(pool, config);
        let sql = binder.build_clear_tenant_sql();
        assert!(sql.contains("NULL"));
    }

    #[test]
    fn test_bind_and_find_connection() {
        let pool = make_pool();
        let config = ConnectionLevelTenantConfig::new(DbType::PostgreSQL);
        let binder = ConnectionTenantBinder::new(pool, config);

        let conn_id1 = binder.bind_connection("tenant_1");
        let conn_id2 = binder.bind_connection("tenant_1");
        let conn_id3 = binder.bind_connection("tenant_2");

        assert_ne!(conn_id1, conn_id2);
        assert_ne!(conn_id1, conn_id3);

        let tenant1_conns = binder.find_bound_connections("tenant_1");
        assert_eq!(tenant1_conns.len(), 2);
        assert!(tenant1_conns.contains(&conn_id1));
        assert!(tenant1_conns.contains(&conn_id2));

        let tenant2_conns = binder.find_bound_connections("tenant_2");
        assert_eq!(tenant2_conns.len(), 1);
        assert!(tenant2_conns.contains(&conn_id3));

        assert_eq!(binder.binding_count("tenant_1"), 2);
        assert_eq!(binder.binding_count("tenant_2"), 1);
        assert_eq!(binder.binding_count("tenant_3"), 0);
    }

    #[test]
    fn test_unbind_connection() {
        let pool = make_pool();
        let config = ConnectionLevelTenantConfig::new(DbType::PostgreSQL);
        let binder = ConnectionTenantBinder::new(pool, config);

        let conn_id1 = binder.bind_connection("tenant_1");
        let conn_id2 = binder.bind_connection("tenant_1");
        assert_eq!(binder.binding_count("tenant_1"), 2);

        binder.unbind_connection("tenant_1", conn_id1);
        assert_eq!(binder.binding_count("tenant_1"), 1);

        let conns = binder.find_bound_connections("tenant_1");
        assert!(conns.contains(&conn_id2));
        assert!(!conns.contains(&conn_id1));

        binder.unbind_connection("tenant_1", conn_id2);
        assert_eq!(binder.binding_count("tenant_1"), 0);
    }

    #[test]
    fn test_validate_tenant_id() {
        let pool = make_pool();
        let config = ConnectionLevelTenantConfig::new(DbType::PostgreSQL);
        let binder = ConnectionTenantBinder::new(pool, config);

        assert!(binder.validate_tenant_id("tenant_1").is_ok());
        assert_eq!(
            binder.validate_tenant_id("").unwrap_err(),
            TenantError::EmptyTenantId
        );
    }

    #[test]
    fn test_resolve_isolation_pg() {
        let pool = make_pool();
        let config = ConnectionLevelTenantConfig::new(DbType::PostgreSQL);
        let binder = ConnectionTenantBinder::new(pool, config);
        assert_eq!(
            binder.resolve_isolation(),
            ConnectionLevelIsolation::SetTenantId
        );
    }

    #[test]
    fn test_resolve_isolation_sqlite_fallback() {
        let pool = make_pool();
        let config = ConnectionLevelTenantConfig::new(DbType::Sqlite);
        let binder = ConnectionTenantBinder::new(pool, config);
        assert_eq!(
            binder.resolve_isolation(),
            ConnectionLevelIsolation::SchemaIsolation
        );
    }

    #[test]
    fn test_resolve_isolation_schema_no_fallback() {
        let pool = make_pool();
        let config = ConnectionLevelTenantConfig::new(DbType::Sqlite)
            .with_isolation(ConnectionLevelIsolation::SchemaIsolation);
        let binder = ConnectionTenantBinder::new(pool, config);
        assert_eq!(
            binder.resolve_isolation(),
            ConnectionLevelIsolation::SchemaIsolation
        );
    }

    #[test]
    fn test_tenant_error_display() {
        let err = TenantError::NoBoundConnection;
        assert!(err.to_string().contains("no connection"));
        let err = TenantError::EmptyTenantId;
        assert!(err.to_string().contains("empty"));
        let err = TenantError::UnsupportedDialect;
        assert!(err.to_string().contains("unsupported"));
    }

    #[test]
    fn test_guard_drop_unbinds() {
        let pool = make_pool();
        let config = ConnectionLevelTenantConfig::new(DbType::PostgreSQL);
        let binder = Arc::new(ConnectionTenantBinder::new(pool, config));

        let conn_id = binder.bind_connection("tenant_1");
        assert_eq!(binder.binding_count("tenant_1"), 1);

        {
            let _guard =
                TenantConnectionGuard::new(binder.clone(), "tenant_1".to_string(), conn_id);
            assert_eq!(binder.binding_count("tenant_1"), 1);
        }

        assert_eq!(binder.binding_count("tenant_1"), 0);
    }

    #[test]
    fn test_guard_release() {
        let pool = make_pool();
        let config = ConnectionLevelTenantConfig::new(DbType::PostgreSQL);
        let binder = Arc::new(ConnectionTenantBinder::new(pool, config));

        let conn_id = binder.bind_connection("tenant_1");
        let mut guard = TenantConnectionGuard::new(binder, "tenant_1".to_string(), conn_id);
        assert!(guard.is_active());
        guard.release().unwrap();
        assert!(!guard.is_active());
    }

    #[test]
    fn test_guard_properties() {
        let pool = make_pool();
        let config = ConnectionLevelTenantConfig::new(DbType::PostgreSQL);
        let binder = Arc::new(ConnectionTenantBinder::new(pool, config));

        let guard = TenantConnectionGuard::new(binder, "tenant_42".to_string(), 999);
        assert_eq!(guard.tenant_id(), "tenant_42");
        assert_eq!(guard.connection_id(), 999);
        assert!(guard.is_active());
        assert!(guard.clear_tenant_sql().contains("NULL"));
    }

    #[test]
    fn test_all_bindings() {
        let pool = make_pool();
        let config = ConnectionLevelTenantConfig::new(DbType::PostgreSQL);
        let binder = ConnectionTenantBinder::new(pool, config);

        binder.bind_connection("tenant_1");
        binder.bind_connection("tenant_1");
        binder.bind_connection("tenant_2");

        let all = binder.all_bindings();
        assert_eq!(all.len(), 3);
    }
}
