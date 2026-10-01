//! M5: MySQL RETURNING 替代方案 e2e 测试
//!
//! MySQL 不支持 RETURNING 子句，使用 3 种替代方案获取插入后数据：
//! ① LAST_INSERT_ID() 获取自增 ID
//! ② 二次 SELECT 获取全行数据
//! ③ INSERT ... ON DUPLICATE KEY UPDATE 实现 upsert 返回
//!
//! 运行：cargo test -p sz-orm-core --features e2e-real-db --test e2e_mysql_returning_alt

#![cfg(feature = "e2e-real-db")]

use sqlx::AssertSqlSafe;
use sqlx::Row;

mod common;

use common::cleanup::unique_table_name;
use common::e2e_env::{e2e_mysql_pool, E2eMysqlDb};

/// ① INSERT + LAST_INSERT_ID() 获取自增 ID
#[tokio::test]
async fn test_mysql_returning_via_last_insert_id() {
    let pool = match e2e_mysql_pool(E2eMysqlDb::Test).await {
        Some(p) => p,
        None => {
            eprintln!("MySQL 未配置，跳过");
            return;
        }
    };
    let table = unique_table_name("ret_alt_1");
    let create_sql = format!(
        "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(255) NOT NULL)",
        table
    );
    sqlx::query(AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    let insert_sql = format!("INSERT INTO `{}` (name) VALUES (?)", table);
    sqlx::query(AssertSqlSafe(insert_sql.as_str()))
        .bind("Alice")
        .execute(&pool)
        .await
        .unwrap();

    let row: sqlx::mysql::MySqlRow = sqlx::query("SELECT LAST_INSERT_ID() as id")
        .fetch_one(&pool)
        .await
        .unwrap();
    let id: i64 = row.get("id");
    assert!(id > 0, "LAST_INSERT_ID() 应返回有效 ID");

    let select_sql = format!("SELECT name FROM `{}` WHERE id = ?", table);
    let row2: sqlx::mysql::MySqlRow = sqlx::query(AssertSqlSafe(select_sql.as_str()))
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let name: String = row2.get("name");
    assert_eq!(name, "Alice");

    let drop_sql = format!("DROP TABLE `{}`", table);
    sqlx::query(AssertSqlSafe(drop_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();
}

/// ② INSERT + 二次 SELECT 获取全行数据
#[tokio::test]
async fn test_mysql_returning_via_second_select() {
    let pool = match e2e_mysql_pool(E2eMysqlDb::Test).await {
        Some(p) => p,
        None => {
            eprintln!("MySQL 未配置，跳过");
            return;
        }
    };
    let table = unique_table_name("ret_alt_2");
    let create_sql = format!(
        "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, name VARCHAR(255), age INT)",
        table
    );
    sqlx::query(AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    let insert_sql = format!("INSERT INTO `{}` (name, age) VALUES (?, ?)", table);
    sqlx::query(AssertSqlSafe(insert_sql.as_str()))
        .bind("Bob")
        .bind(30)
        .execute(&pool)
        .await
        .unwrap();

    let select_sql = format!(
        "SELECT id, name, age FROM `{}` WHERE name = ? ORDER BY id DESC LIMIT 1",
        table
    );
    let row: sqlx::mysql::MySqlRow = sqlx::query(AssertSqlSafe(select_sql.as_str()))
        .bind("Bob")
        .fetch_one(&pool)
        .await
        .unwrap();
    let id: i64 = row.get("id");
    let name: String = row.get("name");
    let age: i32 = row.get("age");
    assert!(id > 0);
    assert_eq!(name, "Bob");
    assert_eq!(age, 30);

    let drop_sql = format!("DROP TABLE `{}`", table);
    sqlx::query(AssertSqlSafe(drop_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();
}

/// ③ INSERT ... ON DUPLICATE KEY UPDATE 实现 upsert 返回
#[tokio::test]
async fn test_mysql_returning_via_on_duplicate_key() {
    let pool = match e2e_mysql_pool(E2eMysqlDb::Test).await {
        Some(p) => p,
        None => {
            eprintln!("MySQL 未配置，跳过");
            return;
        }
    };
    let table = unique_table_name("ret_alt_3");
    let create_sql = format!(
        "CREATE TABLE `{}` (id BIGINT AUTO_INCREMENT PRIMARY KEY, email VARCHAR(255) UNIQUE, name VARCHAR(255))",
        table
    );
    sqlx::query(AssertSqlSafe(create_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();

    let upsert_sql = format!(
        "INSERT INTO `{}` (email, name) VALUES (?, ?) ON DUPLICATE KEY UPDATE name = VALUES(name)",
        table
    );
    sqlx::query(AssertSqlSafe(upsert_sql.as_str()))
        .bind("alice@example.com")
        .bind("Alice")
        .execute(&pool)
        .await
        .unwrap();

    let select_sql = format!("SELECT name FROM `{}` WHERE email = ?", table);
    let row: sqlx::mysql::MySqlRow = sqlx::query(AssertSqlSafe(select_sql.as_str()))
        .bind("alice@example.com")
        .fetch_one(&pool)
        .await
        .unwrap();
    let name: String = row.get("name");
    assert_eq!(name, "Alice");

    sqlx::query(AssertSqlSafe(upsert_sql.as_str()))
        .bind("alice@example.com")
        .bind("Alice Updated")
        .execute(&pool)
        .await
        .unwrap();

    let row2: sqlx::mysql::MySqlRow = sqlx::query(AssertSqlSafe(select_sql.as_str()))
        .bind("alice@example.com")
        .fetch_one(&pool)
        .await
        .unwrap();
    let name2: String = row2.get("name");
    assert_eq!(name2, "Alice Updated");

    let drop_sql = format!("DROP TABLE `{}`", table);
    sqlx::query(AssertSqlSafe(drop_sql.as_str()))
        .execute(&pool)
        .await
        .unwrap();
}
