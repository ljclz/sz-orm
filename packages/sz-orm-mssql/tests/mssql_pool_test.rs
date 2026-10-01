//! T27: MssqlPoolHandle 池操作集成测试（3 tests，#[ignore]）
//!
//! 需要本机 SQL Server 2019+: 从环境变量获取凭据
//! SZ_ORM_MSSQL_HOST/PORT/USER/PASSWORD/DATABASE
//! 运行: cargo test -p sz-orm-mssql --test mssql_pool_test -- --ignored

use sz_orm_mssql::MssqlPoolHandle;

fn dsn() -> String {
    let host = std::env::var("SZ_ORM_MSSQL_HOST").expect("SZ_ORM_MSSQL_HOST not set");
    let port = std::env::var("SZ_ORM_MSSQL_PORT").expect("SZ_ORM_MSSQL_PORT not set");
    let user = std::env::var("SZ_ORM_MSSQL_USER").expect("SZ_ORM_MSSQL_USER not set");
    let password = std::env::var("SZ_ORM_MSSQL_PASSWORD").expect("SZ_ORM_MSSQL_PASSWORD not set");
    let database = std::env::var("SZ_ORM_MSSQL_DATABASE").expect("SZ_ORM_MSSQL_DATABASE not set");
    format!(
        "server={},{};user={};password={};database={};TrustServerCertificate=true",
        host, port, user, password, database
    )
}

#[tokio::test]
#[ignore]
async fn test_mssql_pool_connect_default() {
    let pool = MssqlPoolHandle::connect(&dsn()).await.expect("connect");
    assert_eq!(pool.max_size(), 10);
}

#[tokio::test]
#[ignore]
async fn test_mssql_pool_connect_with_max_size() {
    let pool = MssqlPoolHandle::connect_with_max_size(&dsn(), 5)
        .await
        .expect("connect");
    assert_eq!(pool.max_size(), 5);

    let pool_zero = MssqlPoolHandle::connect_with_max_size(&dsn(), 0)
        .await
        .expect("connect");
    assert_eq!(pool_zero.max_size(), 10);
}

#[tokio::test]
#[ignore]
async fn test_mssql_pool_acquire_and_reuse() {
    let pool = MssqlPoolHandle::connect(&dsn()).await.expect("connect");
    let guard = pool.acquire().await.expect("acquire");
    drop(guard);
    let guard2 = pool.acquire().await.expect("acquire reuse");
    drop(guard2);
    assert_eq!(pool.max_size(), 10);
}
