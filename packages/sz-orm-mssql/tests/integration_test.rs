//! T5.4: MSSQL #[ignore] 真实 DB 集成测试
//!
//! 需要本机 SQL Server 2019+: 从环境变量获取凭据
//! SZ_ORM_MSSQL_HOST/PORT/USER/PASSWORD/DATABASE
//! 运行: cargo test -p sz-orm-mssql --test integration_test -- --ignored

use std::sync::Arc;
use sz_orm_core::ConnectionFactory;
use sz_orm_mssql::{MssqlConnectionFactory, MssqlPoolHandle};

fn dsn() -> Option<String> {
    let host = std::env::var("SZ_ORM_MSSQL_HOST").ok()?;
    let port = std::env::var("SZ_ORM_MSSQL_PORT").ok()?;
    let user = std::env::var("SZ_ORM_MSSQL_USER").ok()?;
    let password = std::env::var("SZ_ORM_MSSQL_PASSWORD").ok()?;
    let database = std::env::var("SZ_ORM_MSSQL_DATABASE").ok()?;
    Some(format!(
        "server={},{};user={};password={};database={};TrustServerCertificate=true",
        host, port, user, password, database
    ))
}

#[tokio::test]
#[ignore]
async fn test_mssql_connection_config() {
    let dsn = dsn().expect("MSSQL env vars not set");
    let handle = Arc::new(MssqlPoolHandle::connect(&dsn).await.unwrap());
    let factory = MssqlConnectionFactory::new(handle);
    let mut conn = factory.create().await.unwrap();
    assert!(conn.is_connected());
    conn.close().await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_mssql_sql_generation() {
    let dsn = dsn().expect("MSSQL env vars not set");
    let handle = Arc::new(MssqlPoolHandle::connect(&dsn).await.unwrap());
    let factory = MssqlConnectionFactory::new(handle);
    let mut conn = factory.create().await.unwrap();
    conn.execute("IF OBJECT_ID('any_test_mssql', 'U') IS NOT NULL DROP TABLE any_test_mssql")
        .await
        .ok();
    conn.execute("CREATE TABLE any_test_mssql (id INT PRIMARY KEY, name NVARCHAR(255) NOT NULL)")
        .await
        .unwrap();
    conn.execute("INSERT INTO any_test_mssql (id, name) VALUES (1, 'alice')")
        .await
        .unwrap();
    let rows = conn
        .query("SELECT id, name FROM any_test_mssql WHERE id = 1")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    conn.execute("DROP TABLE any_test_mssql").await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_mssql_bulk_insert() {
    let dsn = dsn().expect("MSSQL env vars not set");
    let handle = Arc::new(MssqlPoolHandle::connect(&dsn).await.unwrap());
    let factory = MssqlConnectionFactory::new(handle);
    let mut conn = factory.create().await.unwrap();
    conn.execute("IF OBJECT_ID('any_bulk_mssql', 'U') IS NOT NULL DROP TABLE any_bulk_mssql")
        .await
        .ok();
    conn.execute("CREATE TABLE any_bulk_mssql (id INT PRIMARY KEY, val INT)")
        .await
        .unwrap();
    for i in 1..=10 {
        conn.execute(&format!(
            "INSERT INTO any_bulk_mssql (id, val) VALUES ({}, {})",
            i,
            i * 10
        ))
        .await
        .unwrap();
    }
    let rows = conn.query("SELECT id FROM any_bulk_mssql").await.unwrap();
    assert_eq!(rows.len(), 10);
    conn.execute("DROP TABLE any_bulk_mssql").await.unwrap();
}
