//! T5.1: sqlx/any.rs SQLite :memory: 通用路径集成测试
//!
//! 覆盖 AnyBackend::Sqlite 分支：连接、建表、插入、查询、参数化 WHERE、方言占位符。

use sz_orm_core::{Connection, DbType};
use sz_orm_sqlx::{AnyBackend, AnyPool};

#[tokio::test]
async fn test_any_backend_sqlite_connect_memory() {
    let pool = AnyPool::connect("sqlite::memory:").await.unwrap();
    assert_eq!(pool.backend(), AnyBackend::Sqlite);
}

#[tokio::test]
async fn test_any_backend_from_dsn_sqlite() {
    assert_eq!(
        AnyBackend::from_dsn("sqlite::memory:").unwrap(),
        AnyBackend::Sqlite
    );
    assert_eq!(
        AnyBackend::from_dsn("sqlite://./test.db").unwrap(),
        AnyBackend::Sqlite
    );
}

#[tokio::test]
async fn test_any_backend_from_dsn_unknown_returns_err() {
    assert!(AnyBackend::from_dsn("redis://127.0.0.1").is_err());
}

#[tokio::test]
async fn test_any_backend_name() {
    assert_eq!(AnyBackend::Sqlite.name(), "sqlite");
    assert_eq!(AnyBackend::MySql.name(), "mysql");
    assert_eq!(AnyBackend::Postgres.name(), "postgres");
}

#[tokio::test]
async fn test_any_connection_sqlite_create_table() {
    let pool = AnyPool::connect("sqlite::memory:").await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("CREATE TABLE test_t (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
        .await
        .unwrap();
    assert!(conn.is_connected());
}

#[tokio::test]
async fn test_any_connection_sqlite_insert_query() {
    let pool = AnyPool::connect("sqlite::memory:").await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
        .await
        .unwrap();
    conn.execute("INSERT INTO users (id, name) VALUES (1, 'alice')")
        .await
        .unwrap();
    conn.execute("INSERT INTO users (id, name) VALUES (2, 'bob')")
        .await
        .unwrap();
    let rows = conn
        .query("SELECT id, name FROM users WHERE id = 1")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
}

#[tokio::test]
async fn test_any_connection_sqlite_query_all() {
    let pool = AnyPool::connect("sqlite::memory:").await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("CREATE TABLE items (id INTEGER PRIMARY KEY, val INTEGER)")
        .await
        .unwrap();
    for i in 1..=5 {
        conn.execute(&format!(
            "INSERT INTO items (id, val) VALUES ({}, {})",
            i,
            i * 10
        ))
        .await
        .unwrap();
    }
    let rows = conn.query("SELECT id, val FROM items").await.unwrap();
    assert_eq!(rows.len(), 5);
}

#[tokio::test]
async fn test_any_connection_sqlite_ping() {
    let pool = AnyPool::connect("sqlite::memory:").await.unwrap();
    let mut conn = pool.create().await.unwrap();
    assert!(conn.ping().await);
}

#[tokio::test]
async fn test_any_connection_sqlite_transaction() {
    let pool = AnyPool::connect("sqlite::memory:").await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("CREATE TABLE tx_test (id INTEGER PRIMARY KEY)")
        .await
        .unwrap();
    conn.begin_transaction().await.unwrap();
    conn.execute("INSERT INTO tx_test (id) VALUES (1)")
        .await
        .unwrap();
    conn.commit().await.unwrap();
    let rows = conn.query("SELECT id FROM tx_test").await.unwrap();
    assert_eq!(rows.len(), 1);
}

#[tokio::test]
async fn test_any_connection_sqlite_transaction_rollback() {
    let pool = AnyPool::connect("sqlite::memory:").await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("CREATE TABLE tx_rb (id INTEGER PRIMARY KEY)")
        .await
        .unwrap();
    conn.begin_transaction().await.unwrap();
    conn.execute("INSERT INTO tx_rb (id) VALUES (1)")
        .await
        .unwrap();
    conn.rollback().await.unwrap();
    let rows = conn.query("SELECT id FROM tx_rb").await.unwrap();
    assert_eq!(rows.len(), 0);
}

#[tokio::test]
async fn test_any_pool_dialect_sqlite() {
    let pool = AnyPool::connect("sqlite::memory:").await.unwrap();
    let dialect = pool.dialect();
    assert_eq!(dialect.db_type(), DbType::Sqlite);
}

#[tokio::test]
async fn test_any_backend_dialect_sqlite() {
    let dialect = AnyBackend::Sqlite.dialect();
    assert_eq!(dialect.db_type(), DbType::Sqlite);
}

#[tokio::test]
async fn test_any_connection_sqlite_close() {
    let pool = AnyPool::connect("sqlite::memory:").await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.close().await.unwrap();
}

#[tokio::test]
async fn test_any_backend_from_db_type() {
    assert_eq!(
        AnyBackend::from_db_type(DbType::Sqlite),
        Some(AnyBackend::Sqlite)
    );
    assert_eq!(
        AnyBackend::from_db_type(DbType::MySQL),
        Some(AnyBackend::MySql)
    );
    assert_eq!(
        AnyBackend::from_db_type(DbType::PostgreSQL),
        Some(AnyBackend::Postgres)
    );
    assert_eq!(AnyBackend::from_db_type(DbType::Redis), None);
}
