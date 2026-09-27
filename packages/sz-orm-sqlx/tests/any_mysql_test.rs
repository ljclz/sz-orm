//! T5.2: sqlx/any.rs MySQL #[ignore] 集成测试
//!
//! 需要本机 MySQL 9.6: mysql://root:test123@127.0.0.1:3306/sz_orm_test
//! 运行: cargo test -p sz-orm-sqlx --test any_mysql_test -- --ignored

use sz_orm_core::Connection;
use sz_orm_sqlx::{AnyBackend, AnyPool};

const DSN: &str = "mysql://root:test123@127.0.0.1:3306/sz_orm_test";

#[tokio::test]
#[ignore]
async fn test_any_backend_mysql_connect() {
    let pool = AnyPool::connect(DSN).await.unwrap();
    assert_eq!(pool.backend(), AnyBackend::MySql);
}

#[tokio::test]
#[ignore]
async fn test_any_connection_mysql_create_insert_query() {
    let pool = AnyPool::connect(DSN).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS any_test_t").await.unwrap();
    conn.execute("CREATE TABLE any_test_t (id INT PRIMARY KEY, name VARCHAR(255) NOT NULL)")
        .await
        .unwrap();
    conn.execute("INSERT INTO any_test_t (id, name) VALUES (1, 'alice')")
        .await
        .unwrap();
    let rows = conn
        .query("SELECT id, name FROM any_test_t WHERE id = 1")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    conn.execute("DROP TABLE IF EXISTS any_test_t").await.unwrap();
}

#[tokio::test]
#[ignore]
async fn test_any_connection_mysql_dialect_placeholder() {
    let pool = AnyPool::connect(DSN).await.unwrap();
    let dialect = pool.dialect();
    assert_eq!(dialect.db_type(), sz_orm_core::DbType::MySQL);
}

#[tokio::test]
#[ignore]
async fn test_any_connection_mysql_transaction() {
    let pool = AnyPool::connect(DSN).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS any_tx_test").await.unwrap();
    conn.execute("CREATE TABLE any_tx_test (id INT PRIMARY KEY)")
        .await
        .unwrap();
    conn.begin_transaction().await.unwrap();
    conn.execute("INSERT INTO any_tx_test (id) VALUES (1)").await.unwrap();
    conn.rollback().await.unwrap();
    let rows = conn.query("SELECT id FROM any_tx_test").await.unwrap();
    assert_eq!(rows.len(), 0);
    conn.execute("DROP TABLE IF EXISTS any_tx_test").await.unwrap();
}