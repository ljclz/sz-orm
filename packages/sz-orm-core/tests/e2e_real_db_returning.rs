//! M6: SQLite RETURNING e2e 测试
//!
//! SQLite 3.35+ 支持 RETURNING 子句，验证 INSERT/UPDATE/DELETE RETURNING。
//!
//! 运行：cargo test -p sz-orm-core --features e2e-real-db --test e2e_real_db_returning

#![cfg(feature = "e2e-real-db")]

use sqlx::AssertSqlSafe;
use sqlx::Row;

mod common;

#[tokio::test]
async fn test_e2e_sqlite_returning_insert() {
    let pool = match sqlx::SqlitePool::connect("sqlite::memory:").await.ok() {
        Some(p) => p,
        None => return,
    };
    sqlx::query(AssertSqlSafe(
        "CREATE TABLE t (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL)",
    ))
    .execute(&pool)
    .await
    .unwrap();

    let row = sqlx::query(AssertSqlSafe(
        "INSERT INTO t (name) VALUES (?) RETURNING id, name",
    ))
    .bind("Alice")
    .fetch_one(&pool)
    .await
    .unwrap();
    let id: i64 = row.get("id");
    let name: String = row.get("name");
    assert!(id > 0);
    assert_eq!(name, "Alice");
}

#[tokio::test]
async fn test_e2e_sqlite_returning_update() {
    let pool = match sqlx::SqlitePool::connect("sqlite::memory:").await.ok() {
        Some(p) => p,
        None => return,
    };
    sqlx::query(AssertSqlSafe(
        "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)",
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(AssertSqlSafe("INSERT INTO t (id, name) VALUES (?, ?)"))
        .bind(1_i64)
        .bind("Alice")
        .execute(&pool)
        .await
        .unwrap();

    let row = sqlx::query(AssertSqlSafe(
        "UPDATE t SET name = ? WHERE id = ? RETURNING id, name",
    ))
    .bind("Bob")
    .bind(1_i64)
    .fetch_one(&pool)
    .await
    .unwrap();
    let name: String = row.get("name");
    assert_eq!(name, "Bob");
}

#[tokio::test]
async fn test_e2e_sqlite_returning_delete() {
    let pool = match sqlx::SqlitePool::connect("sqlite::memory:").await.ok() {
        Some(p) => p,
        None => return,
    };
    sqlx::query(AssertSqlSafe(
        "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)",
    ))
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(AssertSqlSafe("INSERT INTO t (id, name) VALUES (?, ?)"))
        .bind(1_i64)
        .bind("Alice")
        .execute(&pool)
        .await
        .unwrap();

    let row = sqlx::query(AssertSqlSafe(
        "DELETE FROM t WHERE id = ? RETURNING id, name",
    ))
    .bind(1_i64)
    .fetch_one(&pool)
    .await
    .unwrap();
    let name: String = row.get("name");
    assert_eq!(name, "Alice");
}
