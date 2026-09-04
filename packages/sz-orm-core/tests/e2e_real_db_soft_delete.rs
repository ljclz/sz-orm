//! M1-T6: 真实数据库软删除端到端测试
//!
//! 连真实 MySQL/PostgreSQL/SQLite 验证软删除（deleted_at 字段）行为。

#![cfg(feature = "e2e-real-db")]

mod common;

use common::cleanup::unique_table_name;

async fn mysql_pool() -> Option<sqlx::MySqlPool> {
    let url = std::env::var("MYSQL_URL").ok()?;
    sqlx::MySqlPool::connect(&url).await.ok()
}

async fn pg_pool() -> Option<sqlx::PgPool> {
    let url = std::env::var("POSTGRES_URL").ok()?;
    sqlx::PgPool::connect(&url).await.ok()
}

async fn sqlite_pool() -> Option<sqlx::SqlitePool> {
    sqlx::SqlitePool::connect("sqlite::memory:").await.ok()
}

#[tokio::test]
async fn test_mysql_soft_delete() {
    let pool = match mysql_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_sd");
    let create_sql = format!(
        "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(255), deleted_at TIMESTAMP NULL)",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    let insert_sql = format!(
        "INSERT INTO `{}` (name, deleted_at) VALUES (?, NULL), (?, NULL)",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Alice")
        .bind("Bob")
        .execute(&pool)
        .await
        .unwrap();

    let soft_delete_sql = format!("UPDATE `{}` SET deleted_at = NOW() WHERE name = ?", table);
    sqlx::query(sqlx::AssertSqlSafe(soft_delete_sql.as_str()))
        .bind("Alice")
        .execute(&pool)
        .await
        .unwrap();

    let active_sql = format!(
        "SELECT name FROM `{}` WHERE deleted_at IS NULL ORDER BY name",
        table
    );
    let rows: Vec<(String,)> = sqlx::query_as(sqlx::AssertSqlSafe(active_sql.as_str()))
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "Bob");

    let all_sql = format!("SELECT name FROM `{}` ORDER BY name", table);
    let rows: Vec<(String,)> = sqlx::query_as(sqlx::AssertSqlSafe(all_sql.as_str()))
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);

    sqlx::query(sqlx::AssertSqlSafe(
        (format!("DROP TABLE `{}`", table)).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_pg_soft_delete() {
    let pool = match pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_sd");
    let create_sql = format!(
        "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, name TEXT, deleted_at TIMESTAMP)",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    let insert_sql = format!(
        "INSERT INTO \"{}\" (name, deleted_at) VALUES ($1, NULL), ($2, NULL)",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Alice")
        .bind("Bob")
        .execute(&pool)
        .await
        .unwrap();

    let soft_delete_sql = format!(
        "UPDATE \"{}\" SET deleted_at = NOW() WHERE name = $1",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(soft_delete_sql.as_str()))
        .bind("Alice")
        .execute(&pool)
        .await
        .unwrap();

    let active_sql = format!(
        "SELECT name FROM \"{}\" WHERE deleted_at IS NULL ORDER BY name",
        table
    );
    let rows: Vec<(String,)> = sqlx::query_as(sqlx::AssertSqlSafe(active_sql.as_str()))
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "Bob");

    sqlx::query(sqlx::AssertSqlSafe(
        (format!("DROP TABLE \"{}\"", table)).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_sqlite_soft_delete() {
    let pool = match sqlite_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_sd");
    let create_sql = format!(
        "CREATE TABLE \"{}\" (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT, deleted_at TEXT)",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    let insert_sql = format!(
        "INSERT INTO \"{}\" (name, deleted_at) VALUES (?, NULL), (?, NULL)",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
        .bind("Alice")
        .bind("Bob")
        .execute(&pool)
        .await
        .unwrap();

    let soft_delete_sql = format!(
        "UPDATE \"{}\" SET deleted_at = datetime('now') WHERE name = ?",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(soft_delete_sql.as_str()))
        .bind("Alice")
        .execute(&pool)
        .await
        .unwrap();

    let active_sql = format!(
        "SELECT name FROM \"{}\" WHERE deleted_at IS NULL ORDER BY name",
        table
    );
    let rows: Vec<(String,)> = sqlx::query_as(sqlx::AssertSqlSafe(active_sql.as_str()))
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "Bob");
}
