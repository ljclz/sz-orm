//! M1-T5: 真实数据库分页端到端测试
//!
//! 连真实 MySQL/PostgreSQL/SQLite 验证 offset/limit 分页。

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
async fn test_mysql_pagination_offset_limit() {
    let pool = match mysql_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_page");
    let create_sql = format!(
        "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(255))",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    for i in 0..100 {
        let insert_sql = format!("INSERT INTO `{}` (name) VALUES (?)", table);
        sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
            .bind(format!("User{}", i))
            .execute(&pool)
            .await
            .unwrap();
    }

    let page1_sql = format!("SELECT name FROM `{}` ORDER BY id LIMIT 10 OFFSET 0", table);
    let rows: Vec<(String,)> = sqlx::query_as(sqlx::AssertSqlSafe(page1_sql.as_str()))
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 10);
    assert_eq!(rows[0].0, "User0");

    let page5_sql = format!(
        "SELECT name FROM `{}` ORDER BY id LIMIT 10 OFFSET 40",
        table
    );
    let rows: Vec<(String,)> = sqlx::query_as(sqlx::AssertSqlSafe(page5_sql.as_str()))
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 10);
    assert_eq!(rows[0].0, "User40");

    let last_page_sql = format!(
        "SELECT name FROM `{}` ORDER BY id LIMIT 10 OFFSET 90",
        table
    );
    let rows: Vec<(String,)> = sqlx::query_as(sqlx::AssertSqlSafe(last_page_sql.as_str()))
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 10);
    assert_eq!(rows[0].0, "User90");

    sqlx::query(sqlx::AssertSqlSafe(
        (format!("DROP TABLE `{}`", table)).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_pg_pagination_offset_limit() {
    let pool = match pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_page");
    let create_sql = format!(
        "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, name TEXT)",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    for i in 0..100 {
        let insert_sql = format!("INSERT INTO \"{}\" (name) VALUES ($1)", table);
        sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
            .bind(format!("User{}", i))
            .execute(&pool)
            .await
            .unwrap();
    }

    let page1_sql = format!(
        "SELECT name FROM \"{}\" ORDER BY id LIMIT 10 OFFSET 0",
        table
    );
    let rows: Vec<(String,)> = sqlx::query_as(sqlx::AssertSqlSafe(page1_sql.as_str()))
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 10);

    let page5_sql = format!(
        "SELECT name FROM \"{}\" ORDER BY id LIMIT 10 OFFSET 40",
        table
    );
    let rows: Vec<(String,)> = sqlx::query_as(sqlx::AssertSqlSafe(page5_sql.as_str()))
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 10);

    sqlx::query(sqlx::AssertSqlSafe(
        (format!("DROP TABLE \"{}\"", table)).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_sqlite_pagination_offset_limit() {
    let pool = match sqlite_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_page");
    let create_sql = format!(
        "CREATE TABLE \"{}\" (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT)",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    for i in 0..100 {
        let insert_sql = format!("INSERT INTO \"{}\" (name) VALUES (?)", table);
        sqlx::query(sqlx::AssertSqlSafe(insert_sql.as_str()))
            .bind(format!("User{}", i))
            .execute(&pool)
            .await
            .unwrap();
    }

    let page1_sql = format!(
        "SELECT name FROM \"{}\" ORDER BY id LIMIT 10 OFFSET 0",
        table
    );
    let rows: Vec<(String,)> = sqlx::query_as(sqlx::AssertSqlSafe(page1_sql.as_str()))
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 10);
    assert_eq!(rows[0].0, "User0");

    let page5_sql = format!(
        "SELECT name FROM \"{}\" ORDER BY id LIMIT 10 OFFSET 40",
        table
    );
    let rows: Vec<(String,)> = sqlx::query_as(sqlx::AssertSqlSafe(page5_sql.as_str()))
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 10);
    assert_eq!(rows[0].0, "User40");
}
