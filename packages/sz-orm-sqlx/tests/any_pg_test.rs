//! T5.2: sqlx/any.rs PostgreSQL #[ignore] 集成测试
//!
//! 需要本机 PostgreSQL 18: postgres://postgres:test123@127.0.0.1:5432/sz_orm_test
//! 运行: cargo test -p sz-orm-sqlx --test any_pg_test -- --ignored

use sz_orm_core::Connection;
use sz_orm_sqlx::{AnyBackend, AnyPool};

const DSN: &str = "postgres://postgres:test123@127.0.0.1:5432/sz_orm_test";

#[tokio::test]
#[ignore]
async fn test_any_backend_postgres_connect() {
    let pool = AnyPool::connect(DSN).await.unwrap();
    assert_eq!(pool.backend(), AnyBackend::Postgres);
}

#[tokio::test]
#[ignore]
async fn test_any_connection_pg_create_insert_query() {
    let pool = AnyPool::connect(DSN).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS any_pg_test")
        .await
        .unwrap();
    conn.execute("CREATE TABLE any_pg_test (id SERIAL PRIMARY KEY, name TEXT NOT NULL)")
        .await
        .unwrap();
    conn.execute("INSERT INTO any_pg_test (name) VALUES ('alice')")
        .await
        .unwrap();
    let rows = conn
        .query("SELECT id, name FROM any_pg_test WHERE name = 'alice'")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    conn.execute("DROP TABLE IF EXISTS any_pg_test")
        .await
        .unwrap();
}

#[tokio::test]
#[ignore]
async fn test_any_connection_pg_dialect_placeholder() {
    let pool = AnyPool::connect(DSN).await.unwrap();
    let dialect = pool.dialect();
    assert_eq!(dialect.db_type(), sz_orm_core::DbType::PostgreSQL);
}

#[tokio::test]
#[ignore]
async fn test_any_connection_pg_transaction() {
    let pool = AnyPool::connect(DSN).await.unwrap();
    let mut conn = pool.create().await.unwrap();
    conn.execute("DROP TABLE IF EXISTS any_pg_tx")
        .await
        .unwrap();
    conn.execute("CREATE TABLE any_pg_tx (id INT PRIMARY KEY)")
        .await
        .unwrap();
    conn.begin_transaction().await.unwrap();
    conn.execute("INSERT INTO any_pg_tx (id) VALUES (1)")
        .await
        .unwrap();
    conn.commit().await.unwrap();
    let rows = conn.query("SELECT id FROM any_pg_tx").await.unwrap();
    assert_eq!(rows.len(), 1);
    conn.execute("DROP TABLE IF EXISTS any_pg_tx")
        .await
        .unwrap();
}
