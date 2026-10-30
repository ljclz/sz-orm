//! v9.3.0 T11：connection_tenant 安全行为测试
//!
//! 2026-10-27 SDET 整改：原断言对字面量调用 `str::replace`（测标准库而非产品代码）；
//! 现改为验证 `ConnectionTenantBinder` 的租户 ID 校验、SET SQL 单引号转义（CWE-89）
//! 与连接绑定/解绑生命周期。
//!
//! 运行方式：cargo test -p sz-orm-core --features connection-level-tenant --test connection_tenant_security_test

#![cfg(feature = "connection-level-tenant")]

mod common;

use std::sync::Arc;
use sz_orm_core::connection_tenant::{ConnectionLevelTenantConfig, ConnectionTenantBinder};
use sz_orm_core::{DbType, Pool, PoolConfigBuilder};

fn make_binder() -> ConnectionTenantBinder {
    let db = Arc::new(tokio::sync::Mutex::new(common::InMemoryDb::new()));
    let factory = Arc::new(common::MockConnectionFactory::new(db));
    let pool = Pool::new(
        PoolConfigBuilder::new()
            .max_size(2)
            .min_idle(0)
            .build()
            .expect("valid pool config"),
        factory,
    )
    .expect("pool");
    ConnectionTenantBinder::new(
        Arc::new(pool),
        ConnectionLevelTenantConfig::new(DbType::MySQL),
    )
}

#[tokio::test]
async fn test_set_tenant_sql_escapes_single_quote() {
    let binder = make_binder();
    let sql = binder.build_set_tenant_sql("tenant'; DROP TABLE--");
    assert_eq!(
        sql, "SET app.tenant_id = 'tenant''; DROP TABLE--'",
        "SET 租户 SQL 的单引号必须转义为两个单引号（CWE-89）"
    );
}

#[tokio::test]
async fn test_validate_tenant_id_rejects_empty() {
    let binder = make_binder();
    assert!(
        binder.validate_tenant_id("").is_err(),
        "空租户 ID 必须被校验拒绝"
    );
    assert!(binder.validate_tenant_id("normal_tenant").is_ok());
}

#[tokio::test]
async fn test_bind_unbind_connection_lifecycle() {
    let binder = make_binder();
    let conn = binder.bind_connection("tenant_a");
    assert_eq!(
        binder.find_bound_connections("tenant_a"),
        vec![conn],
        "绑定后应能按租户找到连接"
    );
    binder.unbind_connection("tenant_a", conn);
    assert!(
        binder.find_bound_connections("tenant_a").is_empty(),
        "解绑后该连接不再归属该租户"
    );
}
