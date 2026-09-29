//! T5.3: Oracle #[ignore] 真实 DB 集成测试
//!
//! 需要本机 Oracle 23ai Free: 127.0.0.1:1521/freepdb1.FALSE (sz_orm_test/SzOrmTest2026)
//! 运行: cargo test -p sz-orm-oracle --test integration_test -- --ignored

use std::sync::Arc;
use sz_orm_core::ConnectionFactory;
use sz_orm_oracle::{OracleConnectionFactory, OraclePoolHandle};

fn oracle_user() -> String {
    std::env::var("SZ_ORM_ORACLE_USER").unwrap_or_else(|_| "sz_orm_test".to_string())
}

fn oracle_password() -> String {
    std::env::var("SZ_ORM_ORACLE_PASSWORD").unwrap_or_else(|_| "SzOrmTest2026".to_string())
}

fn oracle_cs() -> String {
    std::env::var("SZ_ORM_ORACLE_CONNECT_STRING")
        .unwrap_or_else(|_| "127.0.0.1:1521/freepdb1.FALSE".to_string())
}

fn pool() -> OracleConnectionFactory {
    let handle = Arc::new(
        OraclePoolHandle::connect(&oracle_user(), &oracle_password(), &oracle_cs())
            .expect("connect oracle"),
    );
    OracleConnectionFactory::new(handle)
}

#[tokio::test]
#[ignore]
async fn test_oracle_connection_management() {
    let factory = pool();
    let mut conn = factory.create().await.unwrap();
    assert!(conn.is_connected());
    conn.ping().await;
    conn.close().await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_oracle_sql_generation() {
    let factory = pool();
    let mut conn = factory.create().await.unwrap();
    conn.execute("DROP TABLE any_test_oracle").await.ok();
    conn.execute(
        "CREATE TABLE any_test_oracle (id NUMBER PRIMARY KEY, name VARCHAR2(255) NOT NULL)",
    )
    .await
    .unwrap();
    conn.execute("INSERT INTO any_test_oracle (id, name) VALUES (1, 'alice')")
        .await
        .unwrap();
    let rows = conn
        .query("SELECT id, name FROM any_test_oracle WHERE id = 1")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    conn.execute("DROP TABLE any_test_oracle").await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_oracle_transaction_isolation() {
    let factory = pool();
    let mut conn = factory.create().await.unwrap();
    conn.execute("DROP TABLE any_tx_oracle").await.ok();
    conn.execute("CREATE TABLE any_tx_oracle (id NUMBER PRIMARY KEY)")
        .await
        .unwrap();
    conn.begin_transaction().await.unwrap();
    conn.execute("INSERT INTO any_tx_oracle (id) VALUES (1)")
        .await
        .unwrap();
    conn.rollback().await.unwrap();
    let rows = conn.query("SELECT id FROM any_tx_oracle").await.unwrap();
    assert_eq!(rows.len(), 0);
    conn.execute("DROP TABLE any_tx_oracle").await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_oracle_bulk_operations() {
    let factory = pool();
    let mut conn = factory.create().await.unwrap();
    conn.execute("DROP TABLE any_bulk_oracle").await.ok();
    conn.execute("CREATE TABLE any_bulk_oracle (id NUMBER PRIMARY KEY, val NUMBER)")
        .await
        .unwrap();
    for i in 1..=10 {
        conn.execute(&format!(
            "INSERT INTO any_bulk_oracle (id, val) VALUES ({}, {})",
            i,
            i * 100
        ))
        .await
        .unwrap();
    }
    let rows = conn.query("SELECT id FROM any_bulk_oracle").await.unwrap();
    assert_eq!(rows.len(), 10);
    conn.execute("DROP TABLE any_bulk_oracle").await.unwrap();
}
