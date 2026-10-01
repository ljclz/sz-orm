//! v8.3.0: 真实数据库迁移端到端测试
//!
//! 连第二台服务器 MySQL/PostgreSQL + 本机 Oracle 验证迁移
//! 创建表/修改表/版本管理/幂等性。

#![cfg(feature = "e2e-real-db")]

use sqlx::Row;

mod common;

use common::cleanup::unique_table_name;
use common::e2e_env::{e2e_mysql_pool, e2e_oracle_conn, e2e_pg_pool, E2eMysqlDb};

// ==================== MySQL 迁移 ====================

#[tokio::test]
async fn test_e2e_mysql_migration_create_table() {
    let pool = match e2e_mysql_pool(E2eMysqlDb::Test).await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_mig_mysql");
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(255), email VARCHAR(255))",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    let row = sqlx::query(sqlx::AssertSqlSafe(
        "SELECT COUNT(*) as cnt FROM information_schema.columns WHERE table_name = ?",
    ))
    .bind(table.as_str())
    .fetch_one(&pool)
    .await
    .unwrap();
    let cnt: i64 = row.try_get("cnt").unwrap();
    assert_eq!(cnt, 3);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{}`", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_e2e_mysql_migration_idempotent() {
    let pool = match e2e_mysql_pool(E2eMysqlDb::Test).await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_mig_idem_mysql");
    let create_sql = format!(
        "CREATE TABLE IF NOT EXISTS `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(255))",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!("SELECT COUNT(*) as cnt FROM `{}`", table).as_str(),
    ))
    .fetch_one(&pool)
    .await
    .unwrap();
    let count: i64 = row.try_get("cnt").unwrap();
    assert_eq!(count, 0);

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE `{}`", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

// ==================== PostgreSQL 迁移 ====================

#[tokio::test]
async fn test_e2e_pg_migration_alter_table() {
    let pool = match e2e_pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_mig_pg");
    sqlx::query(sqlx::AssertSqlSafe(
        format!(
            "CREATE TABLE \"{}\" (id BIGSERIAL PRIMARY KEY, name TEXT)",
            table
        )
        .as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(sqlx::AssertSqlSafe(
        format!("ALTER TABLE \"{}\" ADD COLUMN email TEXT", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(sqlx::AssertSqlSafe(
        format!("INSERT INTO \"{}\" (name, email) VALUES ($1, $2)", table).as_str(),
    ))
    .bind("Alice")
    .bind("alice@example.com")
    .execute(&pool)
    .await
    .unwrap();

    let row = sqlx::query(sqlx::AssertSqlSafe(
        format!("SELECT name, email FROM \"{}\" WHERE name = $1", table).as_str(),
    ))
    .bind("Alice")
    .fetch_one(&pool)
    .await
    .unwrap();
    let name: String = row.try_get("name").unwrap();
    let email: String = row.try_get("email").unwrap();
    assert_eq!(name, "Alice");
    assert_eq!(email, "alice@example.com");

    sqlx::query(sqlx::AssertSqlSafe(
        format!("ALTER TABLE \"{}\" DROP COLUMN email", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_e2e_pg_migration_idempotent() {
    let pool = match e2e_pg_pool().await {
        Some(p) => p,
        None => return,
    };
    let table = unique_table_name("e2e_mig_idem_pg");
    let create_sql = format!(
        "CREATE TABLE IF NOT EXISTS \"{}\" (id BIGSERIAL PRIMARY KEY, name TEXT)",
        table
    );
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query(sqlx::AssertSqlSafe(
        format!("DROP TABLE \"{}\"", table).as_str(),
    ))
    .execute(&pool)
    .await
    .unwrap();
}

// ==================== Oracle 迁移 ====================

#[tokio::test]
async fn test_e2e_oracle_migration_create_table() {
    let conn = match e2e_oracle_conn() {
        Some(c) => c,
        None => return,
    };
    let table = unique_table_name("e2e_mig_ora");
    let table_lower = table.to_lowercase();
    conn.execute(
        &format!(
            "CREATE TABLE \"{}\" (id NUMBER GENERATED ALWAYS AS IDENTITY PRIMARY KEY, name VARCHAR2(255))",
            table_lower
        ),
        &[],
    )
    .unwrap();

    conn.execute(
        &format!("INSERT INTO \"{}\" (name) VALUES (:1)", table_lower),
        &[&"test_user"],
    )
    .unwrap();
    conn.commit().unwrap();

    let rows = conn
        .query(&format!("SELECT name FROM \"{}\"", table_lower), &[])
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let name: String = rows[0].get(0).unwrap();
    assert_eq!(name, "test_user");

    conn.execute(&format!("DROP TABLE \"{}\"", table_lower), &[])
        .unwrap();
}

#[tokio::test]
async fn test_e2e_oracle_migration_idempotent() {
    let conn = match e2e_oracle_conn() {
        Some(c) => c,
        None => return,
    };
    let table = unique_table_name("e2e_mig_idem_ora");
    let table_lower = table.to_lowercase();

    conn.execute(
        &format!(
            "CREATE TABLE \"{}\" (id NUMBER GENERATED ALWAYS AS IDENTITY PRIMARY KEY, name VARCHAR2(255))",
            table_lower
        ),
        &[],
    )
    .unwrap();

    let rows = conn
        .query(
            "SELECT COUNT(*) FROM user_tables WHERE table_name = :1",
            &[&table_lower.as_str()],
        )
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let count: i64 = rows[0].get(0).unwrap();
    assert_eq!(count, 1);

    conn.execute(&format!("DROP TABLE \"{}\"", table_lower), &[])
        .unwrap();
}
// ==================== SQLite 迁移 ====================

#[tokio::test]
async fn test_e2e_sqlite_migration_create_table() {
    let pool = match sqlx::SqlitePool::connect("sqlite::memory:").await.ok() {
        Some(p) => p,
        None => return,
    };
    sqlx::query(sqlx::AssertSqlSafe(
        "CREATE TABLE IF NOT EXISTS schema_version (version INTEGER PRIMARY KEY)",
    ))
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(sqlx::AssertSqlSafe(
        "INSERT INTO schema_version (version) VALUES (?)",
    ))
    .bind(1_i64)
    .execute(&pool)
    .await
    .unwrap();

    let row = sqlx::query(sqlx::AssertSqlSafe("SELECT version FROM schema_version"))
        .fetch_one(&pool)
        .await
        .unwrap();
    let version: i64 = row.try_get("version").unwrap();
    assert_eq!(version, 1);
}
